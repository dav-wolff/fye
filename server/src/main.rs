#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use axum::{extract::{Path, State}, http::{header, HeaderMap, StatusCode}, routing::{get, post}, Router};
use axum_postcard::Postcard;
use db::DirectoryEntry;
use diesel::{connection::SimpleConnection, r2d2::{Pool, PooledConnection, R2D2Connection}, result::DatabaseErrorKind, Connection, OptionalExtension, RunQueryDsl, SqliteConnection};
use fye_shared::{DirectoryInfo, FileInfo, NodeID, NodeInfo};
use r2d2::ManageConnection;
use tokio::net::TcpListener;
use diesel::result::Error as DieselError;

mod db;

mod error;
use error::*;

#[derive(Debug)]
struct ConnectionManager(String);

impl ManageConnection for ConnectionManager {
	type Connection = SqliteConnection;
	type Error = diesel::r2d2::Error;
	
	fn connect(&self) -> Result<Self::Connection, Self::Error> {
		let mut conn = SqliteConnection::establish(&self.0).map_err(diesel::r2d2::Error::ConnectionError)?;
		
		conn.batch_execute("
			PRAGMA foreign_keys = ON;
		").map_err(|err| diesel::r2d2::Error::ConnectionError(diesel::ConnectionError::CouldntSetupConfiguration(err)))?;
		
		Ok(conn)
	}
	
	fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
		conn.ping().map_err(diesel::r2d2::Error::QueryError)
	}
	
	fn has_broken(&self, conn: &mut Self::Connection) -> bool {
		std::thread::panicking() || conn.is_broken()
	}
}

#[tokio::main]
async fn main() {
	let db_manager = ConnectionManager("dev_data/fye.db".to_owned());
	let db_pool = Pool::builder()
		.test_on_check_out(true)
		.build(db_manager).unwrap();
	
	let app_state = AppState {
		db_pool,
	};
	
	let app = Router::new()
		.route("/api/node/:id", get(node_info))
		.route("/api/dir/:id", get(dir_info))
		.route("/api/dir/:id/new-dir", post(create_dir))
		.route("/api/dir/:id/new-file", post(create_file))
		.route("/api/dir/:id/delete-dir", post(delete_dir))
		.route("/api/dir/:id/delete-file", post(delete_file))
		.route("/api/file/:id", get(file_info))
		.route("/api/file/:id/data", get(file_data).patch(write_file_data))
		.with_state(app_state);
	
	let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

#[derive(Clone, Debug)]
struct AppState {
	db_pool: Pool<ConnectionManager>,
}

impl AppState {
	async fn get_connection(&self) -> Result<PooledConnection<ConnectionManager>, r2d2::Error> {
		if let Some(connection) = self.db_pool.try_get() {
			return Ok(connection);
		}
		
		let db_pool = self.db_pool.clone();
		
		tokio::task::spawn_blocking(move || {
			db_pool.get() // may block
		}).await.expect("db_pool.get() should not panic")
	}
}

async fn node_info(State(app_state): State<AppState>, Path(id): Path<NodeID>) -> Result<Postcard<NodeInfo>, Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	if let Some(file) = db::File::get(id)
		.first(conn)
		.optional().map_err(|_| Error::Database)?
	{
		Ok(Postcard(NodeInfo::File(FileInfo {
			size: file.size as u64,
		})))
	} else if let Some(dir) = db::Directory::get(id)
		.first(conn)
		.optional().map_err(|_| Error::Database)?
	{
		let children = dir.entries()
			.load(conn).map_err(|_| Error::Database)?
			.into_iter()
			.map(|entry| {
				let node_id = match (entry.directory, entry.file) {
					(Some(id), None) => id,
					(None, Some(id)) => id,
					_ => panic!("should be impossible due to the check on the directory_entries table"),
				};
				
				(entry.name, NodeID(node_id as u64))
			})
			.collect();
		
		Ok(Postcard(NodeInfo::Directory(DirectoryInfo {
			parent: NodeID(dir.parent as u64),
			children,
		})))
	} else {
		Err(Error::NotFound)
	}
}

async fn dir_info(State(app_state): State<AppState>, Path(id): Path<NodeID>) -> Result<Postcard<DirectoryInfo>, Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	let dir = db::Directory::get(id)
		.first(conn).map_err(|err| match err {
			DieselError::NotFound => {
				match db::File::exists(conn, id) {
					Err(_) => Error::Database,
					Ok(true) => Error::NotADirectory,
					Ok(false) => Error::NotFound,
				}
			},
			_ => Error::Database, // TODO: what to do about unexpected error types?
		})?;
	
	let children = dir.entries()
		.load(conn).map_err(|_| Error::Database)?
		.into_iter()
		.map(|entry| {
			let node_id = match (entry.directory, entry.file) {
				(Some(id), None) => id,
				(None, Some(id)) => id,
				_ => panic!("should be impossible due to the check on the directory_entries table"),
			};
			
			(entry.name, NodeID(node_id as u64))
		})
		.collect();
	
	Ok(Postcard(DirectoryInfo {
		parent: NodeID(dir.parent as u64),
		children,
	}))
}

async fn file_info(State(app_state): State<AppState>, Path(id): Path<NodeID>) -> Result<Postcard<FileInfo>, Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	let file = db::File::get(id)
		.first(conn).map_err(|err| match err {
			DieselError::NotFound => {
				match db::Directory::exists(conn, id) {
					Err(_) => Error::Database,
					Ok(true) => Error::NotAFile,
					Ok(false) => Error::NotFound,
				}
			},
			_ => Error::Database, // TODO: what to do about unexpected error types?
		})?;
	
	Ok(Postcard(FileInfo {
		size: file.size as u64,
	}))
}

async fn file_data(State(_app_state): State<AppState>, Path(_id): Path<NodeID>) -> Result<Vec<u8>, Error> {
	Ok(Vec::new())
}

async fn write_file_data(State(_app_state): State<AppState>, Path(_id): Path<NodeID>, Postcard((_offset, _data)): Postcard<(u64, Vec<u8>)>) -> Result<Postcard<u32>, Error> {
	unimplemented!()
}

fn get_entry_url(conn: &mut SqliteConnection, parent_id: NodeID, name: &str) -> Result<String, Error> {
	// why does rust-analyzer need a type annotation to know what type this is?
	let entry: DirectoryEntry = db::DirectoryEntry::get(parent_id, name)
		.first(conn).map_err(|_| Error::Database)?;
	
	Ok(match (entry.directory, entry.file) {
		(Some(dir_id), None) => format!("/api/dir/{dir_id}"),
		(None, Some(file_id)) => format!("/api/file/{file_id}"),
		_ => panic!("should be impossible due to the check on the directory_entries table"),
	})
}

async fn create_dir(State(app_state): State<AppState>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	let id = transaction(conn, |conn| {
		let id = db::next_available_id(conn).map_err(|_| Error::Database)?;
		
		let dir = db::Directory {
			id: id.0 as i64,
			parent: parent_id.0 as i64,
		};
		
		dir.insert(conn).map_err(|_| Error::Database)?;
		
		let dir_entry = db::NewDirectoryEntry {
			parent: parent_id.0 as i64,
			name: &name,
			directory: Some(id.0 as i64),
			file: None,
		};
		
		dir_entry.insert(conn).map_err(|err| match err {
			// foreign key violation because parent doesn't exist in directories
			DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => Error::NotFound,
			// unique violation because entry already exists
			DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => match get_entry_url(conn, parent_id, &name) {
				Ok(id) => Error::AlreadyExists(format!("/api/dir/{id}")),
				Err(err) => err,
			},
			_ => Error::Database,
		})?;
		
		Ok(id)
	})?;
	
	let mut headers = HeaderMap::new();
	headers.insert(header::LOCATION, format!("/api/dir/{id}").parse().expect("should be a valid header value"));
	
	Ok((StatusCode::CREATED, headers))
}

async fn create_file(State(app_state): State<AppState>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	let id = transaction(conn, |conn| {
		let id = db::next_available_id(conn).map_err(|_| Error::Database)?;
		
		let file = db::File {
			id: id.0 as i64,
			size: 0,
		};
		
		file.insert(conn).map_err(|_| Error::Database)?;
		
		let dir_entry = db::NewDirectoryEntry {
			parent: parent_id.0 as i64,
			name: &name,
			directory: None,
			file: Some(id.0 as i64),
		};
		
		dir_entry.insert(conn).map_err(|err| match err {
			// foreign key violation because parent doesn't exist in directories
			DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => Error::NotFound,
			// unique violation because entry already exists
			DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => match get_entry_url(conn, parent_id, &name) {
				Ok(id) => Error::AlreadyExists(format!("/api/file/{id}")),
				Err(err) => err,
			},
			_ => Error::Database,
		})?;
		
		Ok(id)
	})?;
	
	let mut headers = HeaderMap::new();
	headers.insert(header::LOCATION, format!("/api/file/{id}").parse().expect("should be a valid header value"));
	
	Ok((StatusCode::CREATED, headers))
}

async fn delete_dir(State(app_state): State<AppState>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	transaction(conn, |conn| {
		// TODO: this should be possible with one sql query
		// why does rust-analyzer need a type annotation to know what type this is?
		let entry: DirectoryEntry = db::DirectoryEntry::get(parent_id, &name)
			.first(conn).map_err(|err| match err {
				DieselError::NotFound => {
					match db::File::exists(conn, parent_id) {
						Err(_) => Error::Database,
						Ok(true) => Error::NotADirectory,
						Ok(false) => Error::NotFound,
					}
				},
				_ => Error::Database,
			})?;
		
		let id = match (entry.directory, entry.file) {
			(Some(id), None) => id,
			(None, Some(_)) => return Err(Error::NotADirectory),
			_ => panic!("should be impossible due to the check on the directory_entries table"),
		};
		
		match db::Directory::delete(conn, NodeID(id as u64)) {
			// foreign key violation because directory_entries.parent has foreign key on directory meaning the directory is not empty
			Err(DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _)) => Err(Error::DirectoryNotEmpty),
			Err(_) => Err(Error::Database),
			Ok(false) => panic!("should be impossible as the foreign key constraint on the directory_entries table means the directory must exist"),
			Ok(true) => Ok(()),
		}
	})?;
	
	Ok(StatusCode::NO_CONTENT)
}

async fn delete_file(State(app_state): State<AppState>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	let conn = &mut app_state.get_connection().await.map_err(|_| Error::Database)?;
	
	transaction(conn, |conn| {
		// TODO: this should be possible with one sql query
		// why does rust-analyzer need a type annotation to know what type this is?
		let entry: DirectoryEntry = db::DirectoryEntry::get(parent_id, &name)
			.first(conn).map_err(|err| match err {
				DieselError::NotFound => {
					match db::File::exists(conn, parent_id) {
						Err(_) => Error::Database,
						Ok(true) => Error::NotADirectory,
						Ok(false) => Error::NotFound,
					}
				},
				_ => Error::Database,
			})?;
		
		let id = match (entry.directory, entry.file) {
			(None, Some(id)) => id,
			(Some(_), None) => return Err(Error::NotAFile),
			_ => panic!("should be impossible due to the check on the directory_entries table"),
		};
		
		if db::Directory::delete(conn, NodeID(id as u64)).map_err(|_| Error::Database)? {
			Ok(())
		} else {
			panic!("should be impossible as the foreign key constraint on the directory_entries table means the directory must exist");
		}
	})?;
	
	Ok(StatusCode::NO_CONTENT)
}
