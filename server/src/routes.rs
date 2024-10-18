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
use tokio::fs::{self, File, OpenOptions};
use tokio_util::io::ReaderStream;

use crate::{db, error::{transaction, async_transaction, Error}, extractors::{DbConnection, Directories}, hash::EMPTY_HASH, stream::{stream_to_file, HashStream}};

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::*;
	
	#[tokio::test]
	async fn create_empty_dir() {
		let mut db = TestDb::new();
		
		let (status, headers) = create_dir(db.conn(), Path(NodeID(1)), Postcard("directory".to_owned())).await.unwrap();
		
		assert_eq!(status, StatusCode::CREATED);
		assert_eq!(headers.len(), 1);
		
		let (kind, id) = parse_location(&headers);
		assert_eq!(kind, NodeKind::Directory);
		
		let Postcard(dir_info) = dir_info(db.conn(), Path(id)).await.unwrap();
		
		assert_eq!(dir_info.parent, NodeID(1));
		assert!(dir_info.children.is_empty());
		
		let Postcard(node_info) = node_info(db.conn(), Path(id)).await.unwrap();
		assert_eq!(node_info, NodeInfo::Directory(dir_info));
		
		let Err(err) = file_info(db.conn(), Path(id)).await else {panic!()}; // Postcard doesn't implement Debug so unwrap_err doesn't work
		assert_eq!(err, Error::NotAFile);
	}
}
