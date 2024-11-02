#![cfg(test)]

use std::io;

use axum::http::{header, HeaderMap};
use bytes::Bytes;
use diesel::SqliteConnection;
use futures::Stream;
use fye_shared::NodeID;
use tempfile::TempDir;

use crate::{db, extractors::{DbConnection, Directories}};

pub struct TestDb {
	conn: SqliteConnection,
}

impl TestDb {
	pub fn new() -> Self {
		Self {
			conn: db::establish_connection(":memory:", true).unwrap(),
		}
	}
	
	pub fn conn(&mut self) -> DbConnection<'_> {
		DbConnection::from_single(&mut self.conn)
	}
}

#[derive(PartialEq, Eq, Debug)]
pub enum NodeKind {
	Directory,
	File,
}

pub fn parse_location(headers: &HeaderMap, expected_kind: NodeKind) -> NodeID {
	let location = headers.get(header::LOCATION).unwrap().to_str().unwrap();
	
	parse_location_str(location, expected_kind)
}

pub fn parse_location_str(location: &str, expected_kind: NodeKind) -> NodeID {
	let (kind, id) = match (location.strip_prefix("/api/dir/"), location.strip_prefix("/api/file/")) {
		(Some(id), None) => (NodeKind::Directory, id),
		(None, Some(id)) => (NodeKind::File, id),
		(Some(_), Some(_)) => unreachable!(),
		(None, None) => panic!("invalid location"),
	};
	
	assert_eq!(kind, expected_kind);
	
	id.parse().unwrap()
}

pub fn bytes_stream_from(chunks: &'static [&'static [u8]]) -> impl Stream<Item = Result<Bytes, io::Error>> {
	let iter = chunks.into_iter()
		.map(|chunk| Ok(Bytes::from(&chunk[..])));
	
	futures::stream::iter(iter)
}

pub struct TestDirectories {
	temp_dir: Option<TempDir>,
	directories: Directories,
}

impl TestDirectories {
	pub fn new() -> Self {
		let temp_dir = tempfile::tempdir().unwrap();
		let directories = Directories {
			uploads: temp_dir.path().join("uploads").into(),
			files: temp_dir.path().join("files").into(),
		};
		
		Self {
			temp_dir: Some(temp_dir),
			directories,
		}
	}
	
	pub fn dirs(&self) -> Directories {
		self.directories.clone()
	}
}

impl Drop for TestDirectories {
	fn drop(&mut self) {
		if std::thread::panicking() {
			let path = self.temp_dir.take().expect("should only be taken during drop")
				.into_path();
			
			eprintln!("Panicked with active temp directory: {}", path.to_string_lossy());
		}
	}
}
