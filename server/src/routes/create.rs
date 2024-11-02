use super::*;

fn get_entry_url(conn: &mut SqliteConnection, parent_id: NodeID, name: &str) -> Result<String, Error> {
	// why does rust-analyzer need a type annotation to know what type this is?
	let entry: db::DirectoryEntry = db::DirectoryEntry::get(parent_id, name)
		.first(conn).map_err(|_| Error::Database)?;
	
	Ok(match (entry.directory, entry.file) {
		(Some(dir_id), None) => format!("/api/dir/{dir_id}"),
		(None, Some(file_id)) => format!("/api/file/{file_id}"),
		_ => panic!("should be impossible due to the check on the directory_entries table"),
	})
}

pub async fn create_dir(mut conn: DbConnection<'_>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let id = transaction(&mut conn, |conn| {
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
				Ok(url) => Error::AlreadyExists(url),
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

pub async fn create_file(mut conn: DbConnection<'_>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<(StatusCode, HeaderMap), Error> {
	let id = transaction(&mut conn, |conn| {
		let id = db::next_available_id(conn).map_err(|_| Error::Database)?;
		
		let file = db::File {
			id: id.0 as i64,
			size: 0,
			hash: EMPTY_HASH.to_owned(), // TODO: avoid allocation
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
				Ok(url) => Error::AlreadyExists(url),
				Err(err) => err,
			},
			_ => Error::Database,
		})?;
		
		Ok(id)
	})?;
	
	let mut headers = HeaderMap::new();
	headers.insert(header::LOCATION, format!("/api/file/{id}").parse().expect("should be a valid header value"));
	headers.insert(header::ETAG, Hash(EMPTY_HASH.to_owned()).to_header()); // TODO: avoid unnecessary allocation
	
	Ok((StatusCode::CREATED, headers))
}
