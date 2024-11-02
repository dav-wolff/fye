use super::*;

fn get_file_info(conn: &mut SqliteConnection, id: NodeID) -> Result<db::File, Error> {
	db::File::get(id)
		.first(conn).map_err(|err| match err {
			DieselError::NotFound => {
				match db::Directory::exists(conn, id) {
					Err(_) => Error::Database,
					Ok(true) => Error::NotAFile,
					Ok(false) => Error::NotFound,
				}
			},
			_ => Error::Database, // TODO: what to do about unexpected error types?
		})
}

pub async fn file_info(mut conn: DbConnection<'_>, Path(id): Path<NodeID>) -> Result<Postcard<FileInfo>, Error> {
	let file_info = get_file_info(&mut conn, id)?;
	
	Ok(Postcard(FileInfo {
		size: file_info.size as u64,
		hash: Hash(file_info.hash),
	}))
}

pub async fn file_data(mut conn: DbConnection<'_>, directories: Directories, Path(id): Path<NodeID>, headers: HeaderMap) -> Result<(HeaderMap, Body), Error> {
	let if_match = headers.get(header::IF_MATCH)
		.map(|header| Hash::parse_header(header).ok_or(Error::BadRequest))
		.transpose()?;
	let none_match = headers.get(header::IF_NONE_MATCH)
		.map(|header| Hash::parse_header(header).ok_or(Error::BadRequest))
		.transpose()?;
	
	let file_info = get_file_info(&mut conn, id)?;
	
	if if_match.is_some_and(|hash| hash != file_info.hash) {
		return Err(Error::Modified);
	}
	
	if none_match.is_some_and(|hash| hash == file_info.hash) {
		return Err(Error::NotModified);
	}
	
	let mut headers = HeaderMap::new();
	headers.insert(header::ETAG, Hash(file_info.hash.clone()).to_header()); // TODO: avoid clone
	
	if file_info.hash == EMPTY_HASH {
		return Ok((headers, Body::empty()));
	}
	
	let file = File::open(directories.files.join(&file_info.hash)).await.map_err(|_| Error::IO)?;
	let stream = ReaderStream::new(file);
	let body = Body::from_stream(stream);
	
	Ok((headers, body))
}

pub async fn write_file_data(mut conn: DbConnection<'_>, directories: Directories, Path(id): Path<NodeID>, request: Request) -> Result<StatusCode, Error> {
	// TODO: delete upload file in case of error
	
	let prev_hash = Hash::from_header(request.headers().get(header::IF_MATCH).ok_or(Error::HashMissing)?).ok_or(Error::BadRequest)?;
	
	let file_info = get_file_info(&mut conn, id)?;
	
	if prev_hash != Hash(file_info.hash) {
		return Err(Error::Modified);
	}
	
	let upload_location = directories.uploads.join(id.0.to_string());
	let file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&upload_location).await
		.map_err(|_| Error::IO)?; // TODO: handle case of file already existing
	
	let stream = request.into_body().into_data_stream();
	let mut hash_stream = HashStream::new(stream.map_err(io::Error::other));
	stream_to_file(file, &mut hash_stream).await.map_err(|_| Error::IO)?;
	
	let hash = hash_stream.hash().to_hex();
	let total_size = hash_stream.total_size();
	
	async_transaction(&mut conn, async |conn| {
		let found = db::File::update_content(conn, id, &prev_hash.0, &hash, total_size)
			.map_err(|_| Error::Database)?;
		
		if !found {
			return Err(Error::Modified);
		}
		
		// part of the transaction, so updating the hash gets rolled back if the move fails
		fs::rename(upload_location, directories.files.join(hash.as_str())).await.map_err(|_| Error::IO)?;
		
		Ok(())
	}).await?;
	
	Ok(StatusCode::NO_CONTENT)
}
