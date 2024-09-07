use std::{cmp, collections::HashMap, ffi::OsStr, time::{Duration, UNIX_EPOCH}};

use fuser::{FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use reply::*;

mod reply;

const TTL: Duration = Duration::from_secs(1);
const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

#[derive(Debug)]
enum INode {
	Directory(Directory),
	File(File),
}

#[derive(Debug)]
struct Directory {
	parent: u64,
	children: HashMap<String, u64>,
}

#[derive(Debug)]
struct File {
	content: Vec<u8>,
}

type INodes = HashMap<u64, INode>;

#[derive(Debug)]
pub struct FyeFilesystem {
	inodes: INodes,
	current_inode: u64,
}

impl FyeFilesystem {
	pub fn new() -> Self {
		let mut inodes = HashMap::new();
		inodes.insert(fuser::FUSE_ROOT_ID, INode::Directory(Directory {
			parent: fuser::FUSE_ROOT_ID,
			children: Default::default(),
		}));
		
		Self {
			inodes,
			current_inode: fuser::FUSE_ROOT_ID + 1,
		}
	}
	
	fn attr_for(&self, ino: u64) -> Result<FileAttr, reply::Error> {
		let inode = self.inodes.get(&ino)
			.ok_or(reply::Error::NoEnt)?;
		
		let (size, kind, perm) = match inode {
			INode::Directory(_) => (0, FileType::Directory, 0o700),
			INode::File(file) => (file.content.len() as u64, FileType::RegularFile, 0o600),
		};
		
		Ok(FileAttr {
			ino,
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
}

fn get_directory(inodes: &INodes, ino: u64) -> Result<&Directory, reply::Error> {
	match inodes.get(&ino) {
		None => Err(reply::Error::NoEnt),
		Some(INode::File(_)) => Err(reply::Error::NotDir),
		Some(INode::Directory(directory)) => Ok(directory),
	}
}

fn get_directory_mut(inodes: &mut INodes, ino: u64) -> Result<&mut Directory, reply::Error> {
	match inodes.get_mut(&ino) {
		None => Err(reply::Error::NoEnt),
		Some(INode::File(_)) => Err(reply::Error::NotDir),
		Some(INode::Directory(directory)) => Ok(directory),
	}
}

fn get_file(inodes: &INodes, ino: u64) -> Result<&File, reply::Error> {
	match inodes.get(&ino) {
		None => Err(reply::Error::NoEnt),
		Some(INode::Directory(_)) => Err(reply::Error::IsDir),
		Some(INode::File(file)) => Ok(file),
	}
}

fn get_file_mut(inodes: &mut INodes, ino: u64) -> Result<&mut File, reply::Error> {
	match inodes.get_mut(&ino) {
		None => Err(reply::Error::NoEnt),
		Some(INode::Directory(_)) => Err(reply::Error::IsDir),
		Some(INode::File(file)) => Ok(file),
	}
}

impl Filesystem for FyeFilesystem {
	fn getattr(&mut self, req: &Request, ino: u64, reply: ReplyAttr) {
		println!("getattr");
		respond(reply, || {
			let attr = self.attr_for(ino)?;
			
			Ok(AttrReply {
				attr,
				ttl: TTL,
			})
		})
	}
	
	fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
		println!("lookup");
		respond(reply, || {
			let directory = get_directory(&self.inodes, parent)?;
			
			let Some(&entry) = name.to_str().and_then(|name| directory.children.get(name)) else {
				return Err(reply::Error::NoEnt);
			};
			
			Ok(EntryReply {
				attr: self.attr_for(entry).expect("child of directory should always exist"),
				ttl: TTL,
				generation: 0,
			})
		})
	}
	
	fn readdir(&mut self, req: &Request, ino: u64, _fh: u64, offset: i64, reply: ReplyDirectory) {
		println!("readdir");
		respond(reply, || {
			let directory = get_directory(&self.inodes, ino)?;
			
			let iter = [
				(".", ino),
				("..", directory.parent),
			].into_iter()
				.chain(directory.children.iter().map(|(name, entry)| -> (&str, u64) {
					(name, *entry)
				}))
				.enumerate()
				.skip(offset as usize)
				.map(|(i, (name, entry))| {
					let kind = match self.inodes.get(&ino).expect("child of directory should always exist") {
						INode::Directory(_) => FileType::Directory,
						INode::File(_) => FileType::RegularFile,
					};
					
					DirectoryReplyEntry {
						ino: entry,
						name,
						offset: i as i64 + 1,
						kind,
					}
				});
			
			Ok(iter)
		})
	}
	
	fn mkdir(&mut self, req: &Request, parent: u64, name: &OsStr, _mode: u32, _umask: u32, reply: ReplyEntry) {
		println!("mkdir");
		respond(reply, || {
			let name = name.to_str().ok_or(reply::Error::IlSeq)?;
			
			let directory = get_directory_mut(&mut self.inodes, parent)?;
			
			if directory.children.contains_key(name) {
				return Err(reply::Error::Exist);
			}
			
			let current_inode = self.current_inode;
			self.current_inode += 1;
			
			directory.children.insert(name.to_owned(), current_inode);
			
			self.inodes.insert(current_inode, INode::Directory(Directory {
				parent,
				children: Default::default(),
			}));
			
			Ok(EntryReply {
				attr: self.attr_for(current_inode).expect("child of directory should always exist"),
				ttl: TTL,
				generation: 0,
			})
		})
	}
	
	fn create(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, mode: u32, _umask: u32, _flags: i32, reply: fuser::ReplyCreate) {
		println!("create");
		respond(reply, || {
			let name = name.to_str().ok_or(reply::Error::IlSeq)?;
			
			let file_kind = mode & libc::S_IFMT;
			let is_directory = if file_kind == libc::S_IFDIR {
				true
			} else if file_kind == libc::S_IFREG {
				false
			} else {
				return Err(reply::Error::NotSup)
			};
			
			let directory = get_directory_mut(&mut self.inodes, parent)?;
			
			if directory.children.contains_key(name) {
				return Err(reply::Error::Exist);
			}
			
			let current_inode = self.current_inode;
			self.current_inode += 1;
			
			directory.children.insert(name.to_owned(), current_inode);
			
			self.inodes.insert(current_inode, if is_directory {
				INode::Directory(Directory {
					parent,
					children: Default::default(),
				})
			} else {
				INode::File(File {
					content: Default::default(),
				})
			});
			
			let (kind, perm) = if is_directory {
				(FileType::Directory, 0o700)
			} else {
				(FileType::RegularFile, 0o600)
			};
			
			Ok(CreateReply {
				attr: FileAttr {
					ino: current_inode,
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
		req: &Request<'_>,
		ino: u64,
		mode: Option<u32>,
		uid: Option<u32>,
		gid: Option<u32>,
		size: Option<u64>,
		_atime: Option<fuser::TimeOrNow>,
		_mtime: Option<fuser::TimeOrNow>,
		_ctime: Option<std::time::SystemTime>,
		fh: Option<u64>,
		_crtime: Option<std::time::SystemTime>,
		_chgtime: Option<std::time::SystemTime>,
		_bkuptime: Option<std::time::SystemTime>,
		flags: Option<u32>,
		reply: ReplyAttr,
	) {
		println!("setattr");
		respond(reply, || {
			if let Some(size) = size {
				if size > MAX_FILE_SIZE {
					return Err(reply::Error::FBig)
				}
				
				let file = get_file_mut(&mut self.inodes, ino)?;
				
				file.content.resize(size as usize, 0);
			}
			
			Ok(AttrReply {
				attr: self.attr_for(ino)?,
				ttl: TTL,
			})
		})
	}
	
	fn read(
		&mut self,
		req: &Request,
		ino: u64,
		_fh: u64,
		offset: i64,
		size: u32,
		_flags: i32,
		_lock: Option<u64>,
		reply: ReplyData,
	) {
		println!("read");
		respond(reply, || {
			let file = get_file(&self.inodes, ino)?;
			
			let start = cmp::min(offset as usize, file.content.len());
			let end = cmp::min(offset as usize + size as usize, file.content.len());
			Ok(&file.content[start..end])
		})
	}
	
	fn write(
		&mut self,
		req: &Request<'_>,
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
		// TODO: can offset be negative? why?
		
		respond(reply, || {
			let file = get_file_mut(&mut self.inodes, ino)?;
			
			let end = offset as usize + data.len();
			
			if end > file.content.len() {
				file.content.resize(end, 0);
			}
			
			let dest: &mut [u8] = &mut file.content[offset as usize..end];
			dest.copy_from_slice(data);
			
			Ok(dest.len() as u32)
		})
	}
}
