use std::{cmp, ffi::OsStr, time::{Duration, UNIX_EPOCH}};

use fuser::{FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEntry, ReplyWrite, Request};
use futures_util::{stream::FuturesOrdered, StreamExt};
use fye_shared::{DirectoryInfo, NodeID, NodeInfo};

use crate::{local_file_cache::LocalFileCache, remote_data_service::{CreateNodeError, DeleteDirectoryError, DeleteFileError, FetchDirectoryError, FetchFileError, FetchNodeError, NetworkError, WriteFileError}};

mod reply;
use reply::*;

const TTL: Duration = Duration::from_secs(1);

const DIR_PERMISSIONS: u16 = 0o700;
const FILE_PERMISSIONS: u16 = 0o600;

#[derive(Debug)]
pub struct FyeFilesystem {
	inner: &'static FyeFilesystemInner,
}

impl FyeFilesystem {
	pub fn new(local_file_cache: LocalFileCache) -> Self {
		let inner = FyeFilesystemInner {
			local_file_cache,
		};
		
		Self {
			inner: Box::leak(Box::new(inner)),
		}
	}
	
}

#[derive(Debug)]
struct FyeFilesystemInner {
	local_file_cache: LocalFileCache,
}

impl FyeFilesystemInner {
	async fn attr_for(&self, id: NodeID) -> Result<FileAttr, Error> {
		let info = self.local_file_cache.get_node_info(id).await.map_err(|_| Error::NoEnt)?; // TODO: handle errors besides missing
		
		let (size, kind, perm) = match info {
			NodeInfo::Directory(_) => (0, FileType::Directory, DIR_PERMISSIONS),
			NodeInfo::File(file_info) => (file_info.size, FileType::RegularFile, FILE_PERMISSIONS),
		};
		
		Ok(FileAttr {
			ino: id.0,
			size,
			blocks: 1,
			atime: UNIX_EPOCH,
			mtime: UNIX_EPOCH,
			ctime: UNIX_EPOCH,
			crtime: UNIX_EPOCH,
			kind,
			perm,
			nlink: 1,
			uid: 0,
			gid: 0,
			rdev: 0,
			flags: 0,
			blksize: 512,
		})
	}
	
	async fn get_node(&self, id: NodeID) -> Result<NodeInfo, Error> {
		self.local_file_cache.get_node_info(id).await
			.map_err(|err| match err {
				FetchNodeError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
				FetchNodeError::NetworkFailure(NetworkError::Other) => Error::NoLink,
				FetchNodeError::ServerError | FetchNodeError::ProtocolMismatch => Error::IO,
				FetchNodeError::NotFound => Error::NoEnt,
			})
	}
	
	async fn get_directory(&self, id: NodeID) -> Result<DirectoryInfo, Error> {
		match self.local_file_cache.get_dir_info(id).await {
			Err(FetchDirectoryError::NetworkFailure(NetworkError::Timeout)) => Err(Error::TimedOut),
			Err(FetchDirectoryError::NetworkFailure(NetworkError::Other)) => Err(Error::NoLink),
			Err(FetchDirectoryError::ServerError | FetchDirectoryError::ProtocolMismatch) => Err(Error::IO),
			Err(FetchDirectoryError::NotFound) => Err(Error::NoEnt),
			Err(FetchDirectoryError::NotADirectory) => Err(Error::NotDir),
			Ok(dir_info) => Ok(dir_info),
		}
	}
}

impl Filesystem for FyeFilesystem {
	fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
		println!("getattr");
		let this = self.inner;
		respond(reply, async move || {
			let attr = this.attr_for(NodeID(ino)).await?;
			
			Ok(AttrReply {
				attr,
				ttl: TTL,
			})
		})
	}
	
	fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
		println!("lookup");
		let this = self.inner;
		let name = name.to_str().map(ToOwned::to_owned);
		respond(reply, async move || {
			let name = name.ok_or(Error::NoEnt)?;
			
			let dir_info = this.get_directory(NodeID(parent)).await?;
			
			let Some(&entry) = dir_info.children.get(&name) else {
				return Err(Error::NoEnt);
			};
			
			let attr = this.attr_for(entry).await;
			
			Ok(EntryReply {
				attr: attr.expect("child of directory should always exist"),
				ttl: TTL,
				generation: 0,
			})
		})
	}
	
	fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, reply: ReplyDirectory) {
		println!("readdir");
		let this = self.inner;
		respond(reply, async move || {
			let dir_info = this.get_directory(NodeID(ino)).await?;
			
			let mut futures: FuturesOrdered<_> = [
				// TODO: avoid unnecessary allocation
				(".".to_owned(), ino),
				("..".to_owned(), dir_info.parent.0),
			].into_iter()
				.chain(dir_info.children.iter().map(|(name, entry)| {
					// TODO: avoid unnecessary allocation
					(name.to_owned(), entry.0)
				}))
				.enumerate()
				.skip(offset as usize)
				.map(|(i, (name, entry))| async move {
					let node = this.get_node(NodeID(ino)).await?;
					
					let kind = match node {
						NodeInfo::Directory(_) => FileType::Directory,
						NodeInfo::File(_) => FileType::RegularFile,
					};
					
					Ok(DirectoryReplyEntry {
						ino: entry,
						name,
						offset: i as i64 + 1,
						kind,
					})
				})
				.collect();
			
			let mut entries = Vec::with_capacity(futures.len());
			
			while let Some(entry) = futures.next().await {
				entries.push(entry?);
			}
			
			Ok(entries)
		})
	}
	
	fn mkdir(&mut self, _req: &Request, parent: u64, name: &OsStr, _mode: u32, _umask: u32, reply: ReplyEntry) {
		println!("mkdir");
		let this = self.inner;
		let name = name.to_str().map(ToOwned::to_owned);
		respond(reply, async move || {
			let name = name.ok_or(Error::IlSeq)?;
			
			let id = this.local_file_cache.create_dir(NodeID(parent), name).await
				.map_err(|err| match err {
					CreateNodeError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
					CreateNodeError::NetworkFailure(NetworkError::Other) => Error::NoLink,
					CreateNodeError::ServerError | CreateNodeError::ProtocolMismatch => Error::IO,
					CreateNodeError::ParentNotFound => Error::NoEnt,
					CreateNodeError::ParentNotADirectory => Error::NotDir,
					CreateNodeError::AlreadyExists => Error::Exist,
				})?;
			
			let attr = FileAttr {
				ino: id.0,
				size: 0,
				blocks: 1,
				atime: UNIX_EPOCH,
				mtime: UNIX_EPOCH,
				ctime: UNIX_EPOCH,
				crtime: UNIX_EPOCH,
				kind: FileType::Directory,
				perm: DIR_PERMISSIONS,
				nlink: 1,
				uid: 0,
				gid: 0,
				rdev: 0,
				flags: 0,
				blksize: 512,
			};
			
			Ok(EntryReply {
				attr,
				ttl: TTL,
				generation: 0,
			})
		})
	}
	
	fn create(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, mode: u32, _umask: u32, _flags: i32, reply: ReplyCreate) {
		println!("create");
		let this = self.inner;
		let name = name.to_str().map(ToOwned::to_owned);
		respond(reply, async move || {
			let name = name.ok_or(Error::IlSeq)?;
			
			let file_kind = mode & libc::S_IFMT;
			let is_directory = if file_kind == libc::S_IFDIR {
				true
			} else if file_kind == libc::S_IFREG {
				false
			} else {
				return Err(Error::NotSup);
			};
			
			let result = if is_directory {
				this.local_file_cache.create_dir(NodeID(parent), name).await
			} else {
				this.local_file_cache.create_file(NodeID(parent), name).await
			};
			
			let id = result.map_err(|err| match err {
				CreateNodeError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
				CreateNodeError::NetworkFailure(NetworkError::Other) => Error::NoLink,
				CreateNodeError::ServerError | CreateNodeError::ProtocolMismatch => Error::IO,
				CreateNodeError::ParentNotFound => Error::NoEnt,
				CreateNodeError::ParentNotADirectory => Error::NotDir,
				CreateNodeError::AlreadyExists => Error::Exist,
			})?;
			
			let (kind, perm) = if is_directory {
				(FileType::Directory, DIR_PERMISSIONS)
			} else {
				(FileType::RegularFile, FILE_PERMISSIONS)
			};
			
			Ok(CreateReply {
				attr: FileAttr {
					ino: id.0,
					size: 0,
					blocks: 0,
					atime: UNIX_EPOCH,
					mtime: UNIX_EPOCH,
					ctime: UNIX_EPOCH,
					crtime: UNIX_EPOCH,
					kind,
					perm,
					nlink: 1,
					uid: 0,
					gid: 0,
					rdev: 0,
					flags: 0,
					blksize: 512,
				},
				ttl: TTL,
				generation: 0,
				fh: 0,
				flags: 0,
			})
		})
	}
	
	fn setattr(
		&mut self,
		_req: &Request<'_>,
		ino: u64,
		_mode: Option<u32>,
		_uid: Option<u32>,
		_gid: Option<u32>,
		_size: Option<u64>,
		_atime: Option<fuser::TimeOrNow>,
		_mtime: Option<fuser::TimeOrNow>,
		_ctime: Option<std::time::SystemTime>,
		_fh: Option<u64>,
		_crtime: Option<std::time::SystemTime>,
		_chgtime: Option<std::time::SystemTime>,
		_bkuptime: Option<std::time::SystemTime>,
		_flags: Option<u32>,
		reply: ReplyAttr,
	) {
		println!("setattr");
		let this = self.inner;
		// TODO: implement setattr
		respond(reply, async move || {
			let attr = this.attr_for(NodeID(ino)).await?;
			
			Ok(AttrReply {
				attr,
				ttl: TTL,
			})
		})
		// respond(reply, || -> MaybeAsync<_> {
			// if let Some(size) = size {
			// 	if size > MAX_FILE_SIZE {
			// 		Err(Error::FBig)?;
			// 	}
				
			// 	let file = get_file_mut(&mut self.node_infos, ino)?;
				
			// 	file.content.resize(size as usize, 0);
			// }
			
			// Sync(Ok(AttrReply {
			// 	attr: self.attr_for(ino)?,
			// 	ttl: TTL,
			// }))
		// })
	}
	
	fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
		println!("unlink");
		let this = self.inner;
		let name = name.to_str().map(ToOwned::to_owned);
		respond(reply, async move || {
			let name = name.ok_or(Error::IlSeq)?;
			
			this.local_file_cache.delete_file(NodeID(parent), name).await
				.map_err(|err| match err {
					DeleteFileError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
					DeleteFileError::NetworkFailure(NetworkError::Other) => Error::NoLink,
					DeleteFileError::ServerError | DeleteFileError::ProtocolMismatch => Error::IO,
					DeleteFileError::NotFound => Error::NoEnt,
					DeleteFileError::ParentNotADirectory => Error::NotDir,
					DeleteFileError::NotAFile => Error::IsDir,
				})?;
			
			Ok(())
		})
	}
	
	fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
		println!("rmdir");
		let this = self.inner;
		let name = name.to_str().map(ToOwned::to_owned);
		respond(reply, async move || {
			let name = name.ok_or(Error::IlSeq)?;
			
			this.local_file_cache.delete_dir(NodeID(parent), name).await
				.map_err(|err| match err {
					DeleteDirectoryError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
					DeleteDirectoryError::NetworkFailure(NetworkError::Other) => Error::NoLink,
					DeleteDirectoryError::ServerError | DeleteDirectoryError::ProtocolMismatch => Error::IO,
					DeleteDirectoryError::NotFound => Error::NoEnt,
					DeleteDirectoryError::NotADirectory => Error::NotDir,
					DeleteDirectoryError::NotEmpty => Error::NotEmpty,
				})?;
			
			Ok(())
		})
	}
	
	fn read(
		&mut self,
		_req: &Request,
		ino: u64,
		_fh: u64,
		offset: i64,
		size: u32,
		_flags: i32,
		_lock: Option<u64>,
		reply: ReplyData,
	) {
		println!("read");
		let this = self.inner;
		respond(reply, async move || {
			let data = this.local_file_cache.get_file_data(NodeID(ino)).await
				.map_err(|err| match err {
					FetchFileError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
					FetchFileError::NetworkFailure(NetworkError::Other) => Error::NoLink,
					FetchFileError::ServerError | FetchFileError::ProtocolMismatch => Error::IO,
					FetchFileError::NotFound => Error::NoEnt,
					FetchFileError::NotAFile => Error::IsDir,
				})?;
			
			let start = cmp::min(offset as usize, data.len());
			let end = cmp::min(offset as usize + size as usize, data.len());
			
			Ok(data.slice(start..end))
		})
	}
	
	fn write(
		&mut self,
		_req: &Request<'_>,
		ino: u64,
		_fh: u64,
		offset: i64,
		data: &[u8],
		_write_flags: u32,
		_flags: i32,
		_lock_owner: Option<u64>,
		reply: ReplyWrite,
	) {
		println!("write");
		let this = self.inner;
		let data = data.to_owned(); // TODO: can this (potentially large) allocation be avoided?
		respond(reply, async move || {
			this.local_file_cache.write_file_data(NodeID(ino), offset.try_into().unwrap(), data).await
				.map_err(|err| match err {
					WriteFileError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
					WriteFileError::NetworkFailure(NetworkError::Other) => Error::NoLink,
					WriteFileError::ServerError | WriteFileError::ProtocolMismatch => Error::IO,
					WriteFileError::NotFound => Error::NoEnt,
					WriteFileError::NotAFile => Error::IsDir,
					WriteFileError::Modified => todo!("What to do?"),
				})
		})
	}
}
