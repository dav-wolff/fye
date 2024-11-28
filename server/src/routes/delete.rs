use super::*;

pub async fn delete_dir(mut conn: DbConnection<'_>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	transaction(&mut conn, |conn| {
		// TODO: this should be possible with one sql query
		// why does rust-analyzer need a type annotation to know what type this is?
		let entry: db::DirectoryEntry = db::DirectoryEntry::get(parent_id, &name)
			.first(conn).map_err(|err| match err {
				DieselError::NotFound => {
					match db::File::exists(conn, parent_id) {
						Err(err) => Error::internal(err, "failed looking up node"),
						Ok(true) => Error::NotADirectory,
						Ok(false) => Error::NotFound,
					}
				},
				err => Error::internal(err, "failed looking up node"),
			})?;
		
		let id = match (entry.directory, entry.file) {
			(Some(id), None) => id,
			(None, Some(_)) => return Err(Error::NotADirectory),
			_ => panic!("should be impossible due to the check on the directory_entries table"),
		};
		
		match db::Directory::delete(conn, NodeID(id as u64)) {
			// foreign key violation because directory_entries.parent has foreign key on directory meaning the directory is not empty
			Err(DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _)) => Err(Error::DirectoryNotEmpty),
			Err(err) => Err(Error::internal(err, "failed deleting node")),
			Ok(false) => panic!("should be impossible as the foreign key constraint on the directory_entries table means the directory must exist"),
			Ok(true) => Ok(()),
		}
	})?;
	
	Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_file(mut conn: DbConnection<'_>, Path(parent_id): Path<NodeID>, Postcard(name): Postcard<String>) -> Result<StatusCode, Error> {
	transaction(&mut conn, |conn| {
		// TODO: this should be possible with one sql query
		// why does rust-analyzer need a type annotation to know what type this is?
		let entry: db::DirectoryEntry = db::DirectoryEntry::get(parent_id, &name)
			.first(conn).map_err(|err| match err {
				DieselError::NotFound => {
					match db::File::exists(conn, parent_id) {
						Err(err) => Error::internal(err, "failed looking up node"),
						Ok(true) => Error::NotADirectory,
						Ok(false) => Error::NotFound,
					}
				},
				err => Error::internal(err, "failed looking up node"),
			})?;
		
		let id = match (entry.directory, entry.file) {
			(None, Some(id)) => id,
			(Some(_), None) => return Err(Error::NotAFile),
			_ => panic!("should be impossible due to the check on the directory_entries table"),
		};
		
		if db::File::delete(conn, NodeID(id as u64)).map_err(|err| Error::internal(err, "failed deleting node"))? {
			Ok(())
		} else {
			panic!("should be impossible as the foreign key constraint on the directory_entries table means the file must exist");
		}
	})?;
	
	Ok(StatusCode::NO_CONTENT)
}
