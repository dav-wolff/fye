use std::future::Future;

use bytes::Bytes;
use fye_shared::{DirectoryInfo, NodeID, NodeInfo};
use reqwest::{header, Client, StatusCode, Url};

mod error;
use error::*;
pub use error::{NetworkError, FetchNodeError, FetchDirectoryError, FetchFileError, CreateNodeError};

mod reqwest_postcard;
use reqwest_postcard::*;

#[derive(Debug)]
pub struct RemoteDataService {
	base_url: Url,
	client: Client,
}

impl RemoteDataService {
	pub fn new(base_url: Url) -> Self {
		let client = Client::builder()
			.user_agent(concat!("FyeClient/", env!("CARGO_PKG_VERSION")))
			.build().expect("creating reqwest client should not fail");
		
		Self {
			base_url,
			client,
		}
	}
	
	pub fn fetch_node_info(&self, id: NodeID) -> impl Future<Output = Result<NodeInfo, FetchNodeError>> {
		let url = self.base_url.join(&format!("node/{id}")).expect("url should be valid");
		let request = self.client.get(url);
		
		async {
			let data = decode_errors(request, StatusCode::OK).await?
				.postcard().await?;
			
			Ok(data)
		}
	}
	
	pub fn fetch_dir_info(&self, id: NodeID) -> impl Future<Output = Result<DirectoryInfo, FetchDirectoryError>> {
		let url = self.base_url.join(&format!("dir/{id}")).expect("url should be valid");
		let request = self.client.get(url);
		
		async {
			let data = decode_errors(request, StatusCode::OK).await?
				.postcard().await?;
			
			Ok(data)
		}
	}
	
	pub fn fetch_file_data(&self, id: NodeID) -> impl Future<Output = Result<Bytes, FetchFileError>> {
		let url = self.base_url.join(&format!("file/{id}/data")).expect("url should be valid");
		let request = self.client.get(url);
		
		async {
			let data = decode_errors(request, StatusCode::OK).await?
				.bytes().await.map_err(Error::network_error)?;
			
			Ok(data)
		}
	}
	
	pub fn write_file_data(&self, id: NodeID, offset: u64, data: Vec<u8>) -> impl Future<Output = Result<u32, FetchFileError>> {
		let url = self.base_url.join(&format!("file/{id}/data")).expect("url should be valid");
		let request = self.client.patch(url)
			.postcard(&(offset, data));
		
		async {
			let bytes_written = decode_errors(request, StatusCode::OK).await?
				.postcard().await?;
			
			Ok(bytes_written)
		}
	}
	
	pub fn create_dir(&self, parent_id: NodeID, name: &str) -> impl Future<Output = Result<NodeID, CreateNodeError>> {
		let url = self.base_url.join(&format!("dir/{parent_id}/new-dir")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		async {
			let response = decode_errors(request, StatusCode::CREATED).await?;
			let location = response.headers().get(header::LOCATION).ok_or(CreateNodeError::ProtocolMismatch)?
				.to_str().map_err(|_| Error::ProtocolMismatch)?;
			
			let index = location.rfind('/').ok_or(Error::ProtocolMismatch)?;
			let (_, id) = location.split_at(index + 1);
			
			Ok(id.parse().map_err(|_| Error::ProtocolMismatch)?)
		}
	}
	
	pub fn create_file(&self, parent_id: NodeID, name: &str) -> impl Future<Output = Result<NodeID, CreateNodeError>> {
		let url = self.base_url.join(&format!("dir/{parent_id}/new-file")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		async {
			let response = decode_errors(request, StatusCode::CREATED).await?;
			let location = response.headers().get(header::LOCATION).ok_or(CreateNodeError::ProtocolMismatch)?
				.to_str().map_err(|_| Error::ProtocolMismatch)?;
			
			let index = location.rfind('/').ok_or(Error::ProtocolMismatch)?;
			let (_, id) = location.split_at(index + 1);
			
			Ok(id.parse().map_err(|_| Error::ProtocolMismatch)?)
		}
	}
}
