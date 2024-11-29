mod info;
mod files;
mod create;
mod delete;

pub use info::*;
pub use files::*;
pub use create::*;
pub use delete::*;

use axum::{body::Body, extract::Path, http::StatusCode};
use axum_postcard::Postcard;
use diesel::{result::DatabaseErrorKind, RunQueryDsl as _, OptionalExtension as _, SqliteConnection};
use diesel::result::Error as DieselError;
use fye_shared::{NodeInfo, DirectoryInfo, FileInfo, NodeID, Hash};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{db, error::{transaction, async_transaction, Error}, hash::EMPTY_HASH, stream::{stream_to_file, HashStream}};
use crate::extractors::*;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::*;
	
	use std::error::Error as _;
	use std::io;
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
		
		let Err(err) = file_data(db.conn(), directories.dirs(), Path(NodeID(2)), OptHeader(None), OptHeader(None)).await else {panic!()};
		assert_eq!(err, Error::NotFound);
		
		let Err(err) = write_file_data(db.conn(), directories.dirs(), Path(NodeID(2)), Header(Hash(EMPTY_HASH.to_owned())), BodyStream::empty()).await else {panic!()};
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
		
		let (status, Header(location)) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::CREATED);
		let Location::Directory(id) = location else {panic!()};
		
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
		
		let (status, Header(location), Header(hash)) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		assert_eq!(status, StatusCode::CREATED);
		assert_eq!(hash.0, EMPTY_HASH);
		
		let Location::File(id) = location else {panic!()};
		
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
		
		let (_, Header(location)) = create_dir(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		let Location::Directory(id) = location else {panic!()};
		
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
		
		let (_, Header(location), _) = create_file(db.conn(), Path(ROOT), Postcard("deleted".to_owned())).await.unwrap();
		let Location::File(id) = location else {panic!()};
		
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
		
		let (_, Header(location)) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		let Location::Directory(dir_id) = location else {panic!()};
		
		let (_, Header(location), _) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let Location::File(file_id) = location else {panic!()};
		
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
		
		let (_, Header(dir_location)) = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap();
		
		let (_, Header(file_location), _) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		
		let err = create_dir(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap_err();
		assert_eq!(err, Error::AlreadyExists(dir_location.clone()));
		
		let err = create_file(db.conn(), Path(ROOT), Postcard("directory".to_owned())).await.unwrap_err();
		assert_eq!(err, Error::AlreadyExists(dir_location));
		
		let err = create_dir(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap_err();
		assert_eq!(err, Error::AlreadyExists(file_location.clone()));
		
		let err = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap_err();
		assert_eq!(err, Error::AlreadyExists(file_location));
	}
	
	#[tokio::test]
	async fn read_empty_file() {
		let mut db = TestDb::new();
		let directories = TestDirectories::new();
		
		let (_, Header(location), _) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let Location::File(id) = location else {panic!()};
		
		let (Header(hash), body) = file_data(db.conn(), directories.dirs(), Path(id), OptHeader(None), OptHeader(None)).await.unwrap();
		assert_eq!(hash, Hash(EMPTY_HASH.to_owned()));
		
		let mut stream = body.into_data_stream();
		assert!(stream.next().await.is_none());
	}
	
	#[tokio::test]
	async fn upload_failed_partial() {
		let mut db = TestDb::new();
		let directories = TestDirectories::new();
		
		let (_, Header(location), _) = create_file(db.conn(), Path(ROOT), Postcard("file".to_owned())).await.unwrap();
		let Location::File(id) = location else {panic!()};
		
		// should be repeatable
		for _ in 0..2 {
			let stream = PartialBody::new(b"Partial content".into());
			let err = write_file_data(db.conn(), directories.dirs(), Path(id), Header(Hash(EMPTY_HASH.to_owned())), BodyStream::from_stream(stream)).await.unwrap_err();
			// TODO: maybe the route should return a different error
			assert!(matches!(err, Error::Internal(_)));
			let err = err.source().unwrap().downcast_ref::<io::Error>().unwrap();
			assert_eq!(err.kind(), io::ErrorKind::Other);
			assert_eq!(err.get_ref().unwrap().to_string(), "no more body");
		}
		
		// file data is empty
		let (Header(hash), body) = file_data(db.conn(), directories.dirs(), Path(id), OptHeader(None), OptHeader(None)).await.unwrap();
		assert_eq!(hash, Hash(EMPTY_HASH.to_owned()));
		
		let mut stream = body.into_data_stream();
		assert!(stream.next().await.is_none());
		
		// directories are empty
		let dirs = directories.dirs();
		// TODO: race condition?
		assert!(dirs.uploads.read_dir().unwrap().next().is_none());
		assert!(dirs.files.read_dir().unwrap().next().is_none());
	}
	
	// TODO: add more test cases
}
