use super::*;

fn get_entry_url(conn: &mut SqliteConnection, parent_id: NodeID, name: &str) -> Result<Location, Error> {
	// why does rust-analyzer need a type annotation to know what type this is?
	let entry: db::DirectoryEntry = db::DirectoryEntry::get(parent_id, name)
		.first(conn).map_err(|err| Error::internal(err, "failed looking up directory entry"))?;
	
	Ok(match (entry.directory, entry.file) {
		(Some(id), None) => Location::Directory(NodeID(id as u64)),
		(None, Some(id)) => Location::File(NodeID(id as u64)),
		_ => panic!("should be impossible due to the check on the directory_entries table"),
	})
}

pub async fn create_dir(
	mut conn: DbConnection<'_>,
	Path(parent_id): Path<NodeID>,
	Postcard(name): Postcard<String>
) -> Result<(StatusCode, Header<Location>), Error> {
	let id = transaction(&mut conn, |conn| {
		let id = db::next_available_id(conn).map_err(|err| Error::internal(err, "failed generating next id"))?;
		
		let dir = db::Directory {
			id: id.0 as i64,
			parent: parent_id.0 as i64,
		};
		
		dir.insert(conn).map_err(|err| Error::internal(err, "failed inserting new node"))?;
		
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
			err => Error::internal(err, "failed inserting new directory entry"),
		})?;
		
		Ok(id)
	})?;
	
	Ok((StatusCode::CREATED, Header(Location::Directory(id))))
}

pub async fn create_file(
	mut conn: DbConnection<'_>,
	Path(parent_id): Path<NodeID>,
	Postcard(name): Postcard<String>
) -> Result<(StatusCode, Header<Location>, Header<ETag>), Error> {
	let id = transaction(&mut conn, |conn| {
		let id = db::next_available_id(conn).map_err(|err| Error::internal(err, "failed generating next id"))?;
		
		let file = db::File {
			id: id.0 as i64,
			size: 0,
			hash: EMPTY_HASH.to_owned(), // TODO: avoid allocation
		};
		
		file.insert(conn).map_err(|err| Error::internal(err, "failed inserting new node"))?;
		
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
			err => Error::internal(err, "failed inserting new directory entry"),
		})?;
		
		Ok(id)
	})?;
	
	let hash = Hash(EMPTY_HASH.to_owned()); // TODO: avoid unnecessary allocation
	
	Ok((StatusCode::CREATED, Header(Location::File(id)), Header(hash)))
}
