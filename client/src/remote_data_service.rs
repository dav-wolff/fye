use bytes::Bytes;
use fye_shared::{DirectoryInfo, Hash, NodeID, NodeInfo};
use reqwest::{header, Client, StatusCode, Url};

mod error;
pub use error::*;

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
	
	pub async fn fetch_node_info(&self, id: NodeID) -> Result<NodeInfo, FetchNodeError> {
		let url = self.base_url.join(&format!("node/{id}")).expect("url should be valid");
		let request = self.client.get(url);
		
		let data = decode_errors(request, StatusCode::OK).await?
			.postcard().await?;
		
		Ok(data)
	}
	
	pub async fn fetch_dir_info(&self, id: NodeID) -> Result<DirectoryInfo, FetchDirectoryError> {
		let url = self.base_url.join(&format!("dir/{id}")).expect("url should be valid");
		let request = self.client.get(url);
		
		let data = decode_errors(request, StatusCode::OK).await?
			.postcard().await?;
		
		Ok(data)
	}
	
	pub async fn fetch_file_data(&self, id: NodeID) -> Result<(Hash, Bytes), FetchFileError> {
		let url = self.base_url.join(&format!("file/{id}/data")).expect("url should be valid");
		let request = self.client.get(url);
		
		let response = decode_errors(request, StatusCode::OK).await?;
		let hash = Hash::from_header(
			response.headers().get(header::ETAG).ok_or(FetchFileError::ProtocolMismatch)?
		).ok_or(FetchFileError::ProtocolMismatch)?;
		let data = response.bytes().await.map_err(Error::network_error)?;
		
		Ok((hash, data))
	}
	
	pub async fn write_file_data(&self, id: NodeID, expected_hash: &Hash, data: Vec<u8>) -> Result<(), WriteFileError> {
		let url = self.base_url.join(&format!("file/{id}/data")).expect("url should be valid");
		let request = self.client.put(url)
			.header(header::IF_MATCH, expected_hash.to_header())
			.body(data);
		
		decode_errors(request, StatusCode::NO_CONTENT).await?;
		
		Ok(())
	}
	
	pub async fn create_dir(&self, parent_id: NodeID, name: &str) -> Result<NodeID, CreateNodeError> {
		let url = self.base_url.join(&format!("dir/{parent_id}/new-dir")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		let response = decode_errors(request, StatusCode::CREATED).await?;
		let location = response.headers().get(header::LOCATION).ok_or(CreateNodeError::ProtocolMismatch)?
			.to_str().map_err(|_| Error::ProtocolMismatch)?;
		
		let index = location.rfind('/').ok_or(Error::ProtocolMismatch)?;
		let (_, id) = location.split_at(index + 1);
		
		Ok(id.parse().map_err(|_| Error::ProtocolMismatch)?)
	}
	
	pub async fn create_file(&self, parent_id: NodeID, name: &str) -> Result<(NodeID, Hash), CreateNodeError> {
		let url = self.base_url.join(&format!("dir/{parent_id}/new-file")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		let response = decode_errors(request, StatusCode::CREATED).await?;
		let headers = response.headers();
		let location = headers.get(header::LOCATION).ok_or(CreateNodeError::ProtocolMismatch)?
			.to_str().map_err(|_| Error::ProtocolMismatch)?;
		let hash = Hash::from_header(
			headers.get(header::ETAG).ok_or(CreateNodeError::ProtocolMismatch)?
		).ok_or(CreateNodeError::ProtocolMismatch)?;
		
		let index = location.rfind('/').ok_or(Error::ProtocolMismatch)?;
		let (_, id) = location.split_at(index + 1);
		let id = id.parse().map_err(|_| Error::ProtocolMismatch)?;
		
		Ok((id, hash))
	}
	
	pub async fn delete_dir(&self, parent_id: NodeID, name: &str) -> Result<(), DeleteDirectoryError> {
		let url = self.base_url.join(&format!("dir/{parent_id}/delete-dir")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		decode_errors(request, StatusCode::NO_CONTENT).await?;
		
		Ok(())
	}
	
	pub async fn delete_file(&self, parent_id: NodeID, name: &str) -> Result<(), DeleteFileError> {
		let url = self.base_url.join(&format!("dir/{parent_id}/delete-file")).expect("url should be valid");
		let request = self.client.post(url)
			.postcard(name); // &str and String are serialized the same
		
		decode_errors(request, StatusCode::NO_CONTENT).await?;
		
		Ok(())
	}
}
