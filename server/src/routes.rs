mod info;
mod files;
mod create;
mod delete;

pub use info::*;
pub use files::*;
pub use create::*;
pub use delete::*;

use std::io;

use axum::{body::Body, extract::{Request, Path}, http::{header, StatusCode, HeaderMap}};
use axum_postcard::Postcard;
use diesel::{result::DatabaseErrorKind, RunQueryDsl as _, OptionalExtension as _, SqliteConnection};
use diesel::result::Error as DieselError;
use fye_shared::{NodeInfo, DirectoryInfo, FileInfo, NodeID, Hash};
use futures::TryStreamExt;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{db, error::{transaction, async_transaction, Error}, extractors::{DbConnection, Directories}, hash::EMPTY_HASH, stream::{stream_to_file, HashStream}};

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::*;
	
	use std::error::Error as _;
	use axum::http::Request;
	use futures::StreamExt;
	
	const ROOT: NodeID = NodeID(1);
	
	#[tokio::test]
	async fn node_id_not_found() {
		let mut db = TestDb::new();
		let directories = TestDirectories::new();
		
		let Err(err) = node_info(db.conn(), Path(NodeID(2))).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = dir_info(db.conn(), Path(NodeID(2))).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = file_info(db.conn(), Path(NodeID(2))).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = file_data(db.conn(), directories.dirs(), Path(NodeID(2)), HeaderMap::new()).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let request = Request::builder()
			.header(header::IF_MATCH, Hash(EMPTY_HASH.to_owned()).to_header())
			.body(Body::empty()).unwrap();
		let Err(err) = write_file_data(db.conn(), directories.dirs(), Path(NodeID(2)), request).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = delete_dir(db.conn(), Path(NodeID(2)), Postcard("something".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = delete_file(db.conn(), Path(NodeID(2)), Postcard("something".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotFound);
	}
	
	#[tokio::test]
	async fn entry_not_found() {
		let mut db = TestDb::new();
		
		let Err(err) = delete_dir(db.conn(), Path(ROOT), Postcard("doesn't exist".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		
		let Err(err) = delete_file(db.conn(), Path(ROOT), Postcard("doesn't exist".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotFound);
	}
	
	#[tokio::test]
	async fn new_dir() {
		let mut db = TestDb::new();
		
		let (status, headers) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::CREATED);
		assert_eq!(headers.len(), 1);
		
		let id = parse_location(&headers, NodeKind::Directory);
		
		let Postcard(parent) = dir_info(db.conn(), Path(ROOT)).await.unwrap();
		assert_eq!(parent.parent, ROOT);
		assert_eq!(parent.children.len(), 1);
		
		let (child_name, child_id) = parent.children.iter().next().unwrap();
		assert_eq!(child_name, "directory");
		assert_eq!(child_id, &id);
		
		let Postcard(parent_node) = node_info(db.conn(), Path(ROOT)).await.unwrap();
		assert_eq!(parent_node, NodeInfo::Directory(parent));
		
		let Postcard(dir) = dir_info(db.conn(), Path(id)).await.unwrap();
		assert_eq!(dir.parent, ROOT);
		assert!(dir.children.is_empty());
		
		let Postcard(node) = node_info(db.conn(), Path(id)).await.unwrap();
		assert_eq!(node, NodeInfo::Directory(dir));
		
		let Err(err) = file_info(db.conn(), Path(id)).await else {panic!()}; // Postcard doesn't implement Debug so unwrap_err doesn't work
		assert_eq!(err, Error::NotAFile);
	}
	
	#[tokio::test]
	async fn new_file() {
		let mut db = TestDb::new();
		
		let (status, headers) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::CREATED);
		assert_eq!(headers.len(), 2);
		assert_eq!(headers.get(header::ETAG).unwrap().to_str().unwrap(), format!("\"{EMPTY_HASH}\""));
		
		let id = parse_location(&headers, NodeKind::File);
		
		let Postcard(parent) = dir_info(db.conn(), Path(ROOT)).await.unwrap();
		assert_eq!(parent.parent, ROOT);
		assert_eq!(parent.children.len(), 1);
		
		let (child_name, child_id) = parent.children.iter().next().unwrap();
		assert_eq!(child_name, "file");
		assert_eq!(child_id, &id);
		
		let Postcard(parent_node) = node_info(db.conn(), Path(ROOT)).await.unwrap();
		assert_eq!(parent_node, NodeInfo::Directory(parent));
		
		let Postcard(file) = file_info(db.conn(), Path(id)).await.unwrap();
		assert_eq!(file, FileInfo {
			size: 0,
			hash: Hash(EMPTY_HASH.to_owned()),
		});
		
		let Postcard(node) = node_info(db.conn(), Path(id)).await.unwrap();
		assert_eq!(node, NodeInfo::File(file));
		
		let Err(err) = dir_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotADirectory);
	}
	
	#[tokio::test]
	async fn deleted_dir() {
		let mut db = TestDb::new();
		
		let (_, headers) = create_dir(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		let id = parse_location(&headers, NodeKind::Directory);
		
		let status = delete_dir(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::NO_CONTENT);
		
		let Postcard(parent) = dir_info(db.conn(), Path(ROOT)).await.unwrap();
		assert!(parent.children.is_empty());
		
		let Err(err) = node_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = dir_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = file_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
	}
	
	#[tokio::test]
	async fn deleted_file() {
		let mut db = TestDb::new();
		
		let (_, headers) = create_file(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		let id = parse_location(&headers, NodeKind::File);
		
		let status = delete_file(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::NO_CONTENT);
		
		let Postcard(parent) = dir_info(db.conn(), Path(ROOT)).await.unwrap();
		assert!(parent.children.is_empty());
		
		let Err(err) = node_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = dir_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = file_info(db.conn(), Path(id)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
	}
	
	#[tokio::test]
	async fn delete_wrong_type() {
		let mut db = TestDb::new();
		
		let (_, headers) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		let dir_id = parse_location(&headers, NodeKind::Directory);
		
		let (status, headers) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::CREATED);
		assert_eq!(headers.len(), 2);
		
		let file_id = parse_location(&headers, NodeKind::File);
		
		let Err(err) = delete_file(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotAFile);
		
		let Err(err) = delete_dir(db.conn(), Path(ROOT), Postcard("file".to_owned())).await else {panic!()};
		assert_eq!(err, Error::NotADirectory);
		
		let Postcard(dir) = dir_info(db.conn(), Path(dir_id)).await.unwrap();
		assert_eq!(dir.parent, ROOT);
		assert!(dir.children.is_empty());
		
		let Postcard(file) = file_info(db.conn(), Path(file_id)).await.unwrap();
		assert_eq!(file, FileInfo {
			size: 0,
			hash: Hash(EMPTY_HASH.to_owned()),
		});
		
		let Postcard(parent) = dir_info(db.conn(), Path(ROOT)).await.unwrap();
		assert_eq!(parent.children.len(), 2);
		assert_eq!(parent.children.get("directory"), Some(&dir_id));
		assert_eq!(parent.children.get("file"), Some(&file_id));
	}
	
	#[tokio::test]
	async fn already_exists() {
		let mut db = TestDb::new();
		
		let (_, headers) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		let dir_id = parse_location(&headers, NodeKind::Directory);
		
		let (_, headers) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let file_id = parse_location(&headers, NodeKind::File);
		
		let err = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap_err();
		let Error::AlreadyExists(location) = err else {panic!("wrong error")};
		let id = parse_location_str(&location, NodeKind::Directory);
		assert_eq!(id, dir_id);
		
		let err = create_file(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap_err();
		let Error::AlreadyExists(location) = err else {panic!("wrong error")};
		let id = parse_location_str(&location, NodeKind::Directory);
		assert_eq!(id, dir_id);
		
		let err = create_dir(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap_err();
		let Error::AlreadyExists(location) = err else {panic!("wrong error")};
		let id = parse_location_str(&location, NodeKind::File);
		assert_eq!(id, file_id);
		
		let err = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap_err();
		let Error::AlreadyExists(location) = err else {panic!("wrong error")};
		let id = parse_location_str(&location, NodeKind::File);
		assert_eq!(id, file_id);
	}
	
	#[tokio::test]
	async fn read_empty_file() {
		let mut db = TestDb::new();
		let directories = TestDirectories::new();
		
		let (_, headers) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let id = parse_location(&headers, NodeKind::File);
		
		let (headers, body) = file_data(db.conn(), directories.dirs(), Path(id), HeaderMap::new()).await.unwrap();
		assert_eq!(headers.len(), 1);
		assert_eq!(headers.get(header::ETAG).unwrap(), Hash(EMPTY_HASH.to_owned()).to_header());
		
		let mut stream = body.into_data_stream();
		assert!(stream.next().await.is_none());
	}
	
	#[tokio::test]
	async fn upload_failed_partial() {
		let mut db = TestDb::new();
		let directories = TestDirectories::new();
		
		let (_, headers) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let id = parse_location(&headers, NodeKind::File);
		
		// should be repeatable
		for _ in 0..2 {
			let stream = PartialBody::new(b"Partial content".into());
			let request = Request::builder()
				.header(header::IF_MATCH, Hash(EMPTY_HASH.to_owned()).to_header())
				.body(Body::from_stream(stream)).unwrap();
			let err = write_file_data(db.conn(), directories.dirs(), Path(id), request).await.unwrap_err();
			// TODO: maybe the route should return a different error
			assert!(matches!(err, Error::Internal(_)));
			let err = err.source().unwrap().downcast_ref::<io::Error>().unwrap();
			assert_eq!(err.kind(), io::ErrorKind::Other);
			assert_eq!(err.get_ref().unwrap().to_string(), "no more body");
		}
		
		// file data is empty
		let (headers, body) = file_data(db.conn(), directories.dirs(), Path(id), HeaderMap::new()).await.unwrap();
		assert_eq!(headers.len(), 1);
		assert_eq!(headers.get(header::ETAG).unwrap(), Hash(EMPTY_HASH.to_owned()).to_header());
		
		let mut stream = body.into_data_stream();
		assert!(stream.next().await.is_none());
		
		// directories are empty
		let dirs = directories.dirs();
		assert!(dirs.uploads.read_dir().unwrap().next().is_none());
		assert!(dirs.files.read_dir().unwrap().next().is_none());
	}
	
	// TODO: add more test cases
}
