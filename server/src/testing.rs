#![cfg(test)]

use std::{io, pin::Pin, task::{Context, Poll}};

use bytes::Bytes;
use diesel::SqliteConnection;
use futures::Stream;
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
		
		std::fs::create_dir_all(&directories.uploads).unwrap();
		std::fs::create_dir_all(&directories.files).unwrap();
		
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

pub struct PartialBody {
	text: Vec<u8>,
	is_available: bool,
}

impl PartialBody {
	pub fn new(data: Vec<u8>) -> Self {
		Self {
			text: data,
			is_available: true,
		}
	}
}

impl Stream for PartialBody {
	type Item = Result<Vec<u8>, io::Error>;
	
	fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		if self.is_available {
			self.is_available = false;
			Poll::Ready(Some(Ok(self.text.clone())))
		} else {
			Poll::Ready(Some(Err(io::Error::other("no more body"))))
		}
	}
}
