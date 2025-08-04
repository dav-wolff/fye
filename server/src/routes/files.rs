use super::*;

mod upload_file;
use upload_file::*;
pub mod write_lock;
use write_lock::*;

fn get_file_info(conn: &mut SqliteConnection, id: NodeID) -> Result<db::File, Error> {
	db::File::get(id)
		.first(conn).map_err(|err| match err {
			DieselError::NotFound => {
				match db::Directory::exists(conn, id) {
					Err(err) => Error::internal(err, "failed looking up node"),
					Ok(true) => Error::NotAFile,
					Ok(false) => Error::NotFound,
				}
			},
			err => Error::internal(err, "failed looking up node"),
		})
}

pub async fn file_info(mut conn: DbConnection<'_>, Path(id): Path<NodeID>) -> Result<Postcard<FileInfo>, Error> {
	let file_info = get_file_info(&mut conn, id)?;
	
	Ok(Postcard(FileInfo {
		size: file_info.size as u64,
		hash: Hash(file_info.hash),
	}))
}

pub async fn file_data(
	mut conn: DbConnection<'_>,
	directories: Directories,
	Path(id): Path<NodeID>,
	OptHeader(if_match): OptHeader<IfMatch>,
	OptHeader(none_match): OptHeader<IfNoneMatch>
) -> Result<(Header<ETag>, Body), Error> {
	let file_info = get_file_info(&mut conn, id)?;
	
	let hash = Hash(file_info.hash.clone()); // TODO: avoid clone
	
	if if_match.is_some_and(|expected| expected != hash) {
		return Err(Error::Modified);
	}
	
	if none_match.is_some_and(|expected| expected == hash) {
		return Err(Error::NotModified);
	}
	
	let body = if file_info.hash == EMPTY_HASH {
		Body::empty()
	} else {
		let file = File::open(directories.files.join(&file_info.hash)).await.map_err(|err| Error::internal(err, "could not open requested file"))?;
		let stream = ReaderStream::new(file);
		Body::from_stream(stream)
	};
	
	Ok((Header(hash), body))
}

pub async fn write_file_data(
	mut conn: DbConnection<'_>,
	directories: Directories,
	file_write_lock: FileWriteLock,
	Path(id): Path<NodeID>,
	Header(prev_hash): Header<IfMatch>,
	body_stream: BodyStream
) -> Result<StatusCode, Error> {
	let _guard = file_write_lock.lock(id).await;
	
	let file_info = get_file_info(&mut conn, id)?;
	
	if prev_hash != Hash(file_info.hash) {
		return Err(Error::Modified);
	}
	
	let mut file = UploadFile::new(directories.uploads.join(id.0.to_string())).await
		.map_err(|err| Error::internal(err, "could not open new file for upload"))?;
	
	let mut hash_stream = HashStream::new(body_stream);
	stream_to_file(&mut hash_stream, &mut file).await
		.map_err(|err| Error::internal(err, "failed writing to file for upload"))?;
	
	let hash = hash_stream.hash().to_hex();
	let total_size = hash_stream.total_size();
	
	async_transaction(&mut conn, async |conn| {
		let found = db::File::update_content(conn, id, &prev_hash.0, &hash, total_size)
			.map_err(|err| Error::internal(err, "failed updating node"))?;
		
		if !found {
			return Err(Error::Modified);
		}
		
		// part of the transaction, so updating the hash gets rolled back if the move fails
		file.move_to(directories.files.join(hash.as_str())).await
			.map_err(|err| Error::internal(err, "could not move uploaded file to files directory"))?;
		
		Ok(())
	}).await?;
	
	Ok(StatusCode::NO_CONTENT)
}
