use std::{collections::HashMap, sync::RwLock};

use crate::remote_data_service::{CreateNodeError, DeleteDirectoryError, DeleteFileError, FetchDirectoryError, FetchFileError, WriteFileError};
use bytes::Bytes;
use fye_shared::{DirectoryInfo, FileInfo, NodeID, NodeInfo};

use crate::remote_data_service::{FetchNodeError, RemoteDataService};

#[derive(Debug)]
pub struct LocalFileCache {
	remote_data_service: RemoteDataService,
	local_cache: RwLock<HashMap<NodeID, NodeInfo>>,
}

impl LocalFileCache {
	pub fn new(remote_data_service: RemoteDataService) -> Self {
		Self {
			remote_data_service,
			local_cache: Default::default(),
		}
	}
	
	pub async fn get_node_info(&self, id: NodeID) -> Result<NodeInfo, FetchNodeError> {
		if let Some(info) = self.local_cache.read().expect("poison").get(&id) {
			return Ok(info.clone());
		}
		
		let info = self.remote_data_service.fetch_node_info(id).await?;
		self.local_cache.write().expect("poison").insert(id, info.clone());
		Ok(info)
	}
	
	pub async fn get_dir_info(&self, id: NodeID) -> Result<DirectoryInfo, FetchDirectoryError> {
		// ignore NodeInfo::Directory and fetch fresh data
		if let Some(NodeInfo::Directory(dir_info)) = self.local_cache.read().expect("poison").get(&id) {
			return Ok(dir_info.clone());
		}
		
		let info = self.remote_data_service.fetch_dir_info(id).await?;
		self.local_cache.write().expect("poison").insert(id, NodeInfo::Directory(info.clone()));
		Ok(info)
	}
	
	pub async fn get_file_data(&self, id: NodeID) -> Result<Bytes, FetchFileError> {
		let (hash, data) = self.remote_data_service.fetch_file_data(id).await?;
		self.local_cache.write().expect("poison").insert(id, NodeInfo::File(FileInfo {
			size: data.len() as u64,
			hash,
		}));
		
		Ok(data)
	}
	
	pub async fn write_file_data(&self, id: NodeID, offset: u64, data: Vec<u8>) -> Result<u32, WriteFileError> {
		// TODO: implement offsets
		assert_eq!(offset, 0);
		
		let len = data.len();
		
		let hash = {
			let cache = self.local_cache.read().expect("poison");
			cache.get(&id).and_then(|node| match node {
				NodeInfo::File(file_info) => Some(file_info.hash.clone()), // TODO: is clone necessary?
				NodeInfo::Directory(_) => None,
			})
		};
		
		// TODO: what to do when hash isn't available?
		let hash = hash.unwrap();
		
		self.remote_data_service.write_file_data(id, &hash, data).await?;
		self.local_cache.write().expect("poison").remove(&id);
		
		Ok(len as u32)
	}
	
	pub async fn create_dir(&self, parent_id: NodeID, name: String) -> Result<NodeID, CreateNodeError> {
		let id = self.remote_data_service.create_dir(parent_id, &name).await?;
		
		let mut local_cache = self.local_cache.write().expect("poison");
		local_cache.insert(id, NodeInfo::Directory(DirectoryInfo::with_parent(parent_id)));
		
		if let Some(NodeInfo::Directory(parent_info)) = local_cache.get_mut(&parent_id) {
			parent_info.children.insert(name, id);
		}
		
		Ok(id)
	}
	
	pub async fn create_file(&self, parent_id: NodeID, name: String) -> Result<NodeID, CreateNodeError> {
		let (id, hash) = self.remote_data_service.create_file(parent_id, &name).await?;
		
		let mut local_cache = self.local_cache.write().expect("poison");
		local_cache.insert(id, NodeInfo::File(FileInfo {
			size: 0,
			hash,
		}));
		
		if let Some(NodeInfo::Directory(parent_info)) = local_cache.get_mut(&parent_id) {
			parent_info.children.insert(name, id);
		}
		
		Ok(id)
	}
	
	fn delete_node_from_local_cache(local_cache: &mut HashMap<NodeID, NodeInfo>, parent_id: NodeID, name: &str) {
		let Some(NodeInfo::Directory(parent_info)) = local_cache.get_mut(&parent_id) else {
			return;
		};
		
		let Some(removed) = parent_info.children.remove(name) else {
			return;
		};
		
		let mut to_be_deleted = vec![removed];
		
		while let Some(node) = to_be_deleted.pop() {
			if let Some(NodeInfo::Directory(dir_info)) = local_cache.remove(&node) {
				to_be_deleted.extend(dir_info.children.values());
			}
		}
	}
	
	pub async fn delete_dir(&self, parent_id: NodeID, name: String) -> Result<(), DeleteDirectoryError> {
		self.remote_data_service.delete_dir(parent_id, &name).await?;
		
		let mut local_cache = self.local_cache.write().expect("poison");
		Self::delete_node_from_local_cache(&mut local_cache, parent_id, &name);
		
		Ok(())
	}
	
	pub async fn delete_file(&self, parent_id: NodeID, name: String) -> Result<(), DeleteFileError> {
		self.remote_data_service.delete_file(parent_id, &name).await?;
		
		let mut local_cache = self.local_cache.write().expect("poison");
		Self::delete_node_from_local_cache(&mut local_cache, parent_id, &name);
		
		Ok(())
	}
}
