use std::{collections::HashMap, future::Future, sync::{Arc, RwLock}};

use crate::{maybe_async::MaybeAsync, remote_data_service::{CreateNodeError, DeleteDirectoryError, DeleteFileError, FetchDirectoryError, FetchFileError}};
use bytes::Bytes;
use fye_shared::{DirectoryInfo, FileInfo, NodeID, NodeInfo};

use crate::{remote_data_service::{FetchNodeError, RemoteDataService}, MaybeAsync, maybe_async::MaybeAsync::{Async, Sync}};

#[derive(Debug)]
pub struct LocalFileCache {
	remote_data_service: RemoteDataService,
	local_cache: Arc<RwLock<HashMap<NodeID, NodeInfo>>>,
}

impl LocalFileCache {
	pub fn new(remote_data_service: RemoteDataService) -> Self {
		Self {
			remote_data_service,
			local_cache: Default::default(),
		}
	}
	
	pub fn get_node_info(&self, id: NodeID) -> MaybeAsync!(Result<NodeInfo, FetchNodeError>) {
		if let Some(info) = self.local_cache.read().expect("poison").get(&id) {
			return Sync(Ok(info.clone()));
		}
		
		let request = self.remote_data_service.fetch_node_info(id);
		let local_cache = self.local_cache.clone();
		
		Async(async move {
			let info = request.await?;
			local_cache.write().expect("poison").insert(id, info.clone());
			Ok(info)
		})
	}
	
	pub fn get_dir_info(&self, id: NodeID) -> MaybeAsync!(Result<DirectoryInfo, FetchDirectoryError>) {
		// ignore NodeInfo::Directory and fetch fresh data
		if let Some(NodeInfo::Directory(dir_info)) = self.local_cache.read().expect("poison").get(&id) {
			return Sync(Ok(dir_info.clone()));
		}
		
		let request = self.remote_data_service.fetch_dir_info(id);
		let local_cache = self.local_cache.clone();
		
		Async(async move {
			let info = request.await?;
			local_cache.write().expect("poison").insert(id, NodeInfo::Directory(info.clone()));
			Ok(info)
		})
	}
	
	pub fn get_file_data(&self, id: NodeID) -> MaybeAsync!(Result<Bytes, FetchFileError>) {
		Async(self.remote_data_service.fetch_file_data(id))
	}
	
	pub fn write_file_data(&self, id: NodeID, offset: u64, data: Vec<u8>) -> impl Future<Output = Result<u32, FetchFileError>> {
		// TODO: implement offsets
		assert_eq!(offset, 0);
		
		let len = data.len();
		let request = self.remote_data_service.write_file_data(id, data);
		let local_cache = self.local_cache.clone();
		
		async move {
			request.await?;
			local_cache.write().expect("poison").remove(&id);
			Ok(len as u32)
		}
	}
	
	pub fn create_dir(&self, parent_id: NodeID, name: String) -> impl Future<Output = Result<NodeID, CreateNodeError>> {
		let request = self.remote_data_service.create_dir(parent_id, &name);
		let local_cache = self.local_cache.clone();
		
		async move {
			let id = request.await?;
			
			let mut local_cache = local_cache.write().expect("poison");
			local_cache.insert(id, NodeInfo::Directory(DirectoryInfo::with_parent(parent_id)));
			
			if let Some(NodeInfo::Directory(parent_info)) = local_cache.get_mut(&parent_id) {
				parent_info.children.insert(name, id);
			}
			
			Ok(id)
		}
	}
	
	pub fn create_file(&self, parent_id: NodeID, name: String) -> impl Future<Output = Result<NodeID, CreateNodeError>> {
		let request = self.remote_data_service.create_file(parent_id, &name);
		let local_cache = self.local_cache.clone();
		
		async move {
			let id = request.await?;
			
			let mut local_cache = local_cache.write().expect("poison");
			local_cache.insert(id, NodeInfo::File(FileInfo::default()));
			
			if let Some(NodeInfo::Directory(parent_info)) = local_cache.get_mut(&parent_id) {
				parent_info.children.insert(name, id);
			}
			
			Ok(id)
		}
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
	
	pub fn delete_dir(&self, parent_id: NodeID, name: String) -> impl Future<Output = Result<(), DeleteDirectoryError>> {
		let request = self.remote_data_service.delete_dir(parent_id, &name);
		let local_cache = self.local_cache.clone();
		
		async move {
			request.await?;
			
			let mut local_cache = local_cache.write().expect("poison");
			Self::delete_node_from_local_cache(&mut local_cache, parent_id, &name);
			
			Ok(())
		}
	}
	
	pub fn delete_file(&self, parent_id: NodeID, name: String) -> impl Future<Output = Result<(), DeleteFileError>> {
		let request = self.remote_data_service.delete_file(parent_id, &name);
		let local_cache = self.local_cache.clone();
		
		async move {
			request.await?;
			
			let mut local_cache = local_cache.write().expect("poison");
			Self::delete_node_from_local_cache(&mut local_cache, parent_id, &name);
			
			Ok(())
		}
	}
}
