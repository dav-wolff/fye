use std::{cmp, collections::HashMap, ffi::OsStr, future::Future, time::{Duration, UNIX_EPOCH}};

use fuser::{FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use futures_util::{stream::FuturesOrdered, StreamExt};
use fye_shared::{DirectoryInfo, NodeID, NodeInfo};

use crate::{maybe_async::MaybeAsync::{self, Async, Sync}, remote_data_service::{CreateNodeError, FetchFileError, FetchNodeError, NetworkError, RemoteDataService}, MaybeAsync};

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
	pub fn new(remote_data_fetcher: RemoteDataService) -> Self {
		let inner = FyeFilesystemInner {
			remote_data_service: remote_data_fetcher,
			node_infos: Default::default(),
		};
		
		Self {
			inner: Box::leak(Box::new(inner)),
		}
	}
	
}

#[derive(Debug)]
struct FyeFilesystemInner {
	remote_data_service: RemoteDataService,
	node_infos: HashMap<NodeID, NodeInfo>,
}

impl FyeFilesystemInner {
	fn attr_for(&self, id: NodeID) -> MaybeAsync!(Result<FileAttr, reply::Error>) {
		let info = self.node_infos.get(&id)
			.map(|info| Sync(Ok(info.clone())))
			.unwrap_or_else(|| Async(self.remote_data_service.fetch_node_info(id)));
		
		info.map(move |info| {
			let info = info.map_err(|_| reply::Error::NoEnt)?; // TODO: handle errors besides missing
			
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
		})
	}
	
	fn get_node(&self, id: NodeID) -> MaybeAsync!(Result<NodeInfo, reply::Error>) {
		self.node_infos.get(&id)
			.map(|info| Sync(Ok(info.clone())))
			.unwrap_or_else(|| Async(self.remote_data_service.fetch_node_info(id)))
			.map(|info| info.map_err(|err| match err {
				FetchNodeError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
				FetchNodeError::NetworkFailure(NetworkError::Other) => Error::NoLink,
				FetchNodeError::ServerError | FetchNodeError::ProtocolMismatch => Error::IO,
				FetchNodeError::NotFound => Error::NoEnt,
			}))
	}
	
	fn get_directory(&self, id: NodeID) -> MaybeAsync!(Result<DirectoryInfo, reply::Error>) {
		let info = self.node_infos.get(&id)
			.map(|info| Sync(Ok(info.clone())))
			.unwrap_or_else(|| Async(self.remote_data_service.fetch_node_info(id)));
		
		info.map(|info| match info {
			Err(FetchNodeError::NetworkFailure(NetworkError::Timeout)) => Err(Error::TimedOut),
			Err(FetchNodeError::NetworkFailure(NetworkError::Other)) => Err(Error::NoLink),
			Err(FetchNodeError::ServerError | FetchNodeError::ProtocolMismatch) => Err(Error::IO),
			Err(FetchNodeError::NotFound) => Err(Error::NoEnt),
			Ok(NodeInfo::File(_)) => Err(reply::Error::NotDir),
			Ok(NodeInfo::Directory(dir_info)) => Ok(dir_info),
		})
	}
}

impl Filesystem for FyeFilesystem {
	fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
		println!("getattr");
		let this = self.inner;
		respond(reply, || {
			this.attr_for(NodeID(ino)).map(|attr| {
				let attr = attr?;
				
				Ok(AttrReply {
					attr,
					ttl: TTL,
				})
			})
		})
	}
	
	fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
		println!("lookup");
		let this = self.inner;
		respond(reply, || {
			let name = name.to_str().ok_or(reply::Error::NoEnt)?.to_owned();
			
			this.get_directory(NodeID(parent)).chain(move |dir_info| {
				let dir_info = dir_info?;
				
				let Some(&entry) = dir_info.children.get(&name) else {
					return Sync(Err(reply::Error::NoEnt));
				};
				
				this.attr_for(entry).map(|attr| {
					Ok(EntryReply {
						attr: attr.expect("child of directory should always exist"),
						ttl: TTL,
						generation: 0,
					})
				})
			})
		})
	}
	
	fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, reply: ReplyDirectory) {
		println!("readdir");
		let this = self.inner;
		respond(reply, || {
			this.get_directory(NodeID(ino)).chain(move |dir_info| {
				let dir_info = dir_info?;
				
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
						let node = this.get_node(NodeID(ino)).value().await?;
						
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
				
				Async(async move {
					let mut entries = Vec::with_capacity(futures.len());
					
					while let Some(entry) = futures.next().await {
						entries.push(entry?);
					}
					
					Ok(entries)
				})
			})
		})
	}
	
	fn mkdir(&mut self, _req: &Request, parent: u64, name: &OsStr, _mode: u32, _umask: u32, reply: ReplyEntry) {
		println!("mkdir");
		let this = self.inner;
		respond(reply, || {
			let name = name.to_str().ok_or(reply::Error::IlSeq)?.to_owned();
			
			MaybeAsync::Async(async move {
				let id = this.remote_data_service.create_dir(NodeID(parent), name).await
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
		})
	}
	
	fn create(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, mode: u32, _umask: u32, _flags: i32, reply: fuser::ReplyCreate) {
		println!("create");
		let this = self.inner;
		respond(reply, || {
			let file_kind = mode & libc::S_IFMT;
			let is_directory = if file_kind == libc::S_IFDIR {
				true
			} else if file_kind == libc::S_IFREG {
				false
			} else {
				Err(reply::Error::NotSup)?;
				unreachable!();
			};
			
			let name = name.to_str().ok_or(reply::Error::IlSeq)?.to_owned();
			
			Async(async move {
				let result = if is_directory {
					this.remote_data_service.create_dir(NodeID(parent), name).await
				} else {
					this.remote_data_service.create_file(NodeID(parent), name).await
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
		respond(reply, || {
			this.attr_for(NodeID(ino)).map(|attr| {
				let attr = attr?;
				
				Ok(AttrReply {
					attr,
					ttl: TTL,
				})
			})
		})
		// respond(reply, || -> MaybeAsync<_> {
			// if let Some(size) = size {
			// 	if size > MAX_FILE_SIZE {
			// 		Err(reply::Error::FBig)?;
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
		respond(reply, || -> MaybeAsync<_, _> {
			Async(async move {
				let data = this.remote_data_service.fetch_file_data(NodeID(ino)).await
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
		reply: fuser::ReplyWrite,
	) {
		println!("write");
		let this = self.inner;
		respond(reply, || {
			let data = data.to_owned();
			
			Async(async move {
				this.remote_data_service.write_file_data(NodeID(ino), offset.try_into().unwrap(), data).await
					.map_err(|err| match err {
						FetchFileError::NetworkFailure(NetworkError::Timeout) => Error::TimedOut,
						FetchFileError::NetworkFailure(NetworkError::Other) => Error::NoLink,
						FetchFileError::ServerError | FetchFileError::ProtocolMismatch => Error::IO,
						FetchFileError::NotFound => Error::NoEnt,
						FetchFileError::NotAFile => Error::IsDir,
					})
			})
		})
	}
}
