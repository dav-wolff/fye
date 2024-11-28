use super::*;

pub async fn node_info(mut conn: DbConnection<'_>, Path(id): Path<NodeID>) -> Result<Postcard<NodeInfo>, Error> {
	let conn = &mut *conn;
	
	if let Some(file) = db::File::get(id)
		.first(conn)
		.optional().map_err(|err| Error::internal(err, "failed looking up node"))?
	{
		Ok(Postcard(NodeInfo::File(FileInfo {
			size: file.size as u64,
			hash: Hash(file.hash),
		})))
	} else if let Some(dir) = db::Directory::get(id)
		.first(conn)
		.optional().map_err(|err| Error::internal(err, "failed looking up node"))?
	{
		let children = dir.entries()
			.load(conn).map_err(|err| Error::internal(err, "failed looking up directory entries"))?
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

pub async fn dir_info(mut conn: DbConnection<'_>, Path(id): Path<NodeID>) -> Result<Postcard<DirectoryInfo>, Error> {
	let conn = &mut *conn;
	
	let dir = db::Directory::get(id)
		.first(conn).map_err(|err| match err {
			DieselError::NotFound => {
				match db::File::exists(conn, id) {
					Err(err) => Error::internal(err, "failed looking up node"),
					Ok(true) => Error::NotADirectory,
					Ok(false) => Error::NotFound,
				}
			},
			err => Error::internal(err, "failed looking up node"), // TODO: what to do about unexpected error types?
		})?;
	
	let children = dir.entries()
		.load(conn).map_err(|err| Error::internal(err, "failed looking up directory entries"))?
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
