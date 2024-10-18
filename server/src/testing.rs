#![cfg(test)]

use axum::http::{header, HeaderMap};
use diesel::SqliteConnection;
use fye_shared::NodeID;

use crate::{db, extractors::DbConnection};

pub struct TestDb {
	conn: SqliteConnection,
}

impl TestDb {
	pub fn new() -> Self {
		Self {
			conn: db::establish_connection(":memory:").unwrap(),
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

pub fn parse_location(headers: &HeaderMap) -> (NodeKind, NodeID) {
	let location = headers.get(header::LOCATION).unwrap().to_str().unwrap();
	
	let (kind, id) = match (location.strip_prefix("/api/dir/"), location.strip_prefix("/api/file/")) {
		(Some(id), None) => (NodeKind::Directory, id),
		(None, Some(id)) => (NodeKind::File, id),
		(Some(_), Some(_)) => unreachable!(),
		(None, None) => panic!("invalid location"),
	};
	
	(kind, id.parse().unwrap())
}
