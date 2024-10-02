#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use std::{collections::{hash_map, HashMap}, sync::{Arc, RwLock}};

use axum::{extract::{Path, State}, http::{header, HeaderMap, StatusCode}, routing::{get, post}, Router};
use axum_postcard::Postcard;
use fye_shared::{DirectoryInfo, FileInfo, NodeID, NodeInfo};
use tokio::net::TcpListener;

mod error;
use error::*;

#[tokio::main]
async fn main() {
	let mut nodes = HashMap::new();
	nodes.insert(NodeID::ROOT, Node::Directory(DirectoryInfo {
		parent: NodeID::ROOT,
		children: Default::default(),
	}));
	
	let app_state = AppState {
		nodes: RwLock::new(Nodes {
			current_id: NodeID(NodeID::ROOT.0 + 1),
			nodes,
		}),
	};
	
	let app = Router::new()
		.route("/api/node/:id", get(node_info))
		.route("/api/dir/:id", get(dir_info))
		.route("/api/dir/:id/new-dir", post(create_dir))
		.route("/api/dir/:id/new-file", post(create_file))
		.route("/api/dir/:id/delete-dir", post(delete_dir))
		.route("/api/dir/:id/delete-file", post(delete_file))
		.route("/api/file/:id", get(file_info))
		.route("/api/file/:id/data", get(file_data).patch(write_file_data))
		.with_state(Arc::new(app_state));
	
	let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

#[derive(Debug)]
struct AppState {
	nodes: RwLock<Nodes>,
}

#[derive(Debug)]
struct Nodes {
	current_id: NodeID,
	nodes: HashMap<NodeID, Node>,
}

#[derive(Debug)]
enum Node {
	Directory(DirectoryInfo),
	File(Vec<u8>),
}

async fn node_info(State(app_state): State<Arc<AppState>>, Path(id): Path<NodeID>) -> Result<Postcard<NodeInfo>, Error> {
	let nodes = app_state.nodes.read().expect("poison");
	
	let node_info = match nodes.nodes.get(&id).ok_or(Error::NotFound)? {
		Node::Directory(dir_info) => NodeInfo::Directory(dir_info.clone()),
		Node::File(data) => NodeInfo::File(FileInfo {
			size: data.len() as u64,
		}),
	};
	
	Ok(Postcard(node_info))
}

async fn dir_info(State(app_state): State<Arc<AppState>>, Path(id): Path<NodeID>) -> Result<Postcard<DirectoryInfo>, Error> {
	let nodes = app_state.nodes.read().expect("poison");
	
	match nodes.nodes.get(&id) {
		None => Err(Error::NotFound),
		Some(Node::File(_)) => Err(Error::NotADirectory),
		Some(Node::Directory(dir_info)) => Ok(Postcard(dir_info.clone())),
	}
}

async fn file_info(State(app_state): State<Arc<AppState>>, Path(id): Path<NodeID>) -> Result<Postcard<FileInfo>, Error> {
	let nodes = app_state.nodes.read().expect("poison");
	
	match nodes.nodes.get(&id) {
		None => Err(Error::NotFound),
		Some(Node::Directory(_)) => Err(Error::NotAFile),
		Some(Node::File(data)) => Ok(Postcard(FileInfo {
			size: data.len() as u64,
		})),
	}
}

async fn file_data(State(app_state): State<Arc<AppState>>, Path(id): Path<NodeID>) -> Result<Vec<u8>, Error> {
	let nodes = app_state.nodes.read().expect("poison");
	
	match nodes.nodes.get(&id) {
		None => Err(Error::NotFound),
		Some(Node::Directory(_)) => Err(Error::NotAFile),
		Some(Node::File(data)) => Ok(data.clone()),
	}
}

async fn write_file_data(State(app_state): State<Arc<AppState>>, Path(id): Path<NodeID>, Postcard((offset, data)): Postcard<(u64, Vec<u8>)>) -> Result<Postcard<u32>, Error> {
	let mut nodes = app_state.nodes.write().expect("poison");
	
	let file_data = match nodes.nodes.get_mut(&id) {
		None => return Err(Error::NotFound),
		Some(Node::Directory(_)) => return Err(Error::NotAFile),
		Some(Node::File(file_data)) => file_data,
	};
	
	let end = offset as usize + data.len();
	
	if end > file_data.len() {
		file_data.resize(end, 0);
	}
	
	let dest = &mut file_data[offset as usize..end];
	dest.copy_from_slice(&data);
	
	Ok(Postcard(dest.len() as u32))
}

fn new_node(nodes: &RwLock<Nodes>, parent_id: NodeID, name: String, node: Node) -> Result<NodeID, Error> {
	let mut nodes = nodes.write().expect("poison");
	
	let parent_info = match nodes.nodes.get(&parent_id) {
		None => return Err(Error::NotFound),
		Some(Node::File(_)) => return Err(Error::NotADirectory),
		Some(Node::Directory(info)) => info,
	};
	
	if let Some(id) = parent_info.children.get(&name) {
		return Err(Error::AlreadyExists(format!("/api/dir/{id}")));
	}
	
	loop {
		let id = NodeID(match nodes.current_id {
			NodeID(u64::MAX) => {
				nodes.current_id = NodeID(NodeID::ROOT.0 + 1);
				u64::MAX
			},
			NodeID(id) => {
				nodes.current_id = NodeID(id + 1);
				id
			},
		});
		
		if let hash_map::Entry::Vacant(entry) = nodes.nodes.entry(id) {
			entry.insert(node);
			
			let Some(Node::Directory(parent_info)) = nodes.nodes.get_mut(&parent_id) else {
				unreachable!("already checked the entry exists and is a directory");
			};
			
			let prev_entry = parent_info.children.insert(name, id);
			assert!(prev_entry.is_none(), "already checked that no such entry exists");
			
			break Ok(id);
		}
	}
}

async fn create_dir(State(app_state): State<Arc<AppState>>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let id = new_node(&app_state.nodes, parent_id, name, Node::Directory(DirectoryInfo::with_parent(parent_id)))?;
	
	let mut headers = HeaderMap::new();
	headers.insert(header::LOCATION, format!("/api/dir/{id}").parse().expect("should be a valid header value"));
	
	Ok((StatusCode::CREATED, headers))
}

async fn create_file(State(app_state): State<Arc<AppState>>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let id = new_node(&app_state.nodes, parent_id, name, Node::File(Vec::new()))?;
	
	let mut headers = HeaderMap::new();
	headers.insert(header::LOCATION, format!("/api/file/{id}").parse().expect("should be a valid header value"));
	
	Ok((StatusCode::CREATED, headers))
}

async fn delete_dir(State(app_state): State<Arc<AppState>>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	let mut nodes = app_state.nodes.write().expect("poison");
	
	// check this early, if an error occurs there's no need for exclusive access
	let parent_info = match nodes.nodes.get(&parent_id) {
		None => return Err(Error::NotFound),
		Some(Node::File(_)) => return Err(Error::NotADirectory),
		Some(Node::Directory(info)) => info,
	};
	
	let Some(&entry) = parent_info.children.get(&name) else {
		return Err(Error::NotFound);
	};
	
	match nodes.nodes.get(&entry) {
		None => return Err(Error::NotFound),
		Some(Node::File(_)) => return Err(Error::NotADirectory),
		Some(Node::Directory(dir)) if !dir.children.is_empty() => return Err(Error::DirectoryNotEmpty),
		_ => (),
	}
	
	let Some(Node::Directory(ref mut parent_info)) = nodes.nodes.get_mut(&parent_id) else {
		unreachable!("existed earlier during exclusive lock");
	};
	
	parent_info.children.remove(&name).expect("should exist");
	nodes.nodes.remove(&entry).expect("should exist");
	
	Ok(StatusCode::NO_CONTENT)
}

async fn delete_file(State(app_state): State<Arc<AppState>>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	let mut nodes = app_state.nodes.write().expect("poison");
	
	let parent_info = match nodes.nodes.get(&parent_id) {
		None => return Err(Error::NotFound),
		Some(Node::File(_)) => return Err(Error::NotADirectory),
		Some(Node::Directory(info)) => info,
	};
	
	let Some(&entry) = parent_info.children.get(&name) else {
		return Err(Error::NotFound);
	};
	
	match nodes.nodes.get(&entry) {
		None => return Err(Error::NotFound),
		Some(Node::Directory(_)) => return Err(Error::NotAFile),
		Some(Node::File(_)) => (),
	}
	
	let Some(Node::Directory(ref mut parent_info)) = nodes.nodes.get_mut(&parent_id) else {
		unreachable!("existed earlier during exclusive lock");
	};
	
	parent_info.children.remove(&name).expect("should exist");
	nodes.nodes.remove(&entry).expect("should exist");
	
	Ok(StatusCode::NO_CONTENT)
}
