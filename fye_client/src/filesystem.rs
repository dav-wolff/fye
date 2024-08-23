use std::{cmp, collections::HashMap, ffi::OsStr, time::{Duration, UNIX_EPOCH}};
use libc::{EEXIST, EFBIG, EILSEQ, EISDIR, ENOENT, ENOTDIR, ENOTSUP};

use fuser::{FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};

const TTL: Duration = Duration::from_secs(1);
const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024;

#[derive(Debug)]
enum INode {
	Directory {
		parent: u64,
		children: HashMap<String, u64>,
	},
	File {
		content: Vec<u8>,
	},
}

use INode::*;

#[derive(Debug)]
pub struct FyeFilesystem {
	inodes: HashMap<u64, INode>,
	current_inode: u64,
}

impl FyeFilesystem {
	pub fn new() -> Self {
		let mut inodes = HashMap::new();
		inodes.insert(fuser::FUSE_ROOT_ID, Directory {
			parent: fuser::FUSE_ROOT_ID,
			children: Default::default(),
		});
		
		Self {
			inodes,
			current_inode: fuser::FUSE_ROOT_ID,
		}
	}
	
	pub fn attr_for(&self, ino: u64) -> Option<FileAttr> {
		let inode = self.inodes.get(&ino)?;
		
		let (size, kind, perm) = match inode {
			Directory {..} => (0, FileType::Directory, 0o700),
			File {content} => (content.len() as u64, FileType::RegularFile, 0o600),
		};
		
		Some(FileAttr {
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

impl Filesystem for FyeFilesystem {
	fn getattr(&mut self, req: &Request, ino: u64, reply: ReplyAttr) {
		match self.attr_for(ino) {
			None => {
				reply.error(ENOENT);
			},
			Some(attr) => {
				reply.attr(&TTL, &attr);
			}
		}
	}
	
	fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
		let children = match self.inodes.get(&parent) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(File {..}) => {
				reply.error(ENOTDIR);
				return;
			},
			Some(Directory {children, ..}) => children,
		};
		
		let Some(&entry) = name.to_str().and_then(|name| children.get(name)) else {
			reply.error(ENOENT);
			return;
		};
		
		reply.entry(&TTL, &self.attr_for(entry).expect("child of directory should always exist"), 0);
	}
	
	fn readdir(&mut self, req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
		let (parent, children) = match self.inodes.get(&ino) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(File {..}) => {
				reply.error(ENOTDIR);
				return;
			},
			Some(Directory {parent, children}) => (parent, children),
		};
		
		let iter = [
			(".", ino),
			("..", *parent),
		].into_iter()
			.chain(children.iter().map(|(name, entry)| -> (&str, u64) {
				(name, *entry)
			}))
			.enumerate()
			.skip(offset as usize);
		
		for (i, (name, entry)) in iter {
			let kind = match self.inodes.get(&ino).expect("child of directory should always exist") {
				Directory {..} => FileType::Directory,
				File {..} => FileType::RegularFile,
			};
			
			if reply.add(entry, i as i64 + 1, kind, name) {
				break;
			}
		}
		
		reply.ok();
	}
	
	fn mkdir(&mut self, req: &Request, parent: u64, name: &OsStr, _mode: u32, _umask: u32, reply: ReplyEntry) {
		let Some(name) = name.to_str() else {
			reply.error(EILSEQ);
			return;
		};
		
		let children = match self.inodes.get_mut(&parent) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(File {..}) => {
				reply.error(ENOTDIR);
				return;
			},
			Some(Directory {children, ..}) => children,
		};
		
		if children.contains_key(name) {
			reply.error(EEXIST);
			return;
		}
		
		self.current_inode += 1;
		children.insert(name.to_owned(), self.current_inode);
		
		self.inodes.insert(self.current_inode, Directory {
			parent,
			children: Default::default(),
		});
		
		reply.entry(&TTL, &self.attr_for(self.current_inode).expect("child of directory should always exist"), 0);
	}
	
	fn create(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, mode: u32, _umask: u32, _flags: i32, reply: fuser::ReplyCreate) {
		let Some(name) = name.to_str() else {
			reply.error(EILSEQ);
			return;
		};
		
		let file_kind = mode & libc::S_IFMT;
		let is_directory = if file_kind == libc::S_IFDIR {
			true
		} else if file_kind == libc::S_IFREG {
			false
		} else {
			reply.error(ENOTSUP);
			return;
		};
		
		let children = match self.inodes.get_mut(&parent) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(File {..}) => {
				reply.error(ENOTDIR);
				return;
			},
			Some(Directory {children, ..}) => children,
		};
		
		if children.contains_key(name) {
			reply.error(EEXIST);
			return;
		}
		
		self.current_inode += 1;
		children.insert(name.to_owned(), self.current_inode);
		
		self.inodes.insert(self.current_inode, if is_directory {
			Directory {
				parent,
				children: Default::default(),
			}
		} else {
			File {
				content: Default::default(),
			}
		});
		
		let (kind, perm) = if is_directory {
			(FileType::Directory, 0o700)
		} else {
			(FileType::RegularFile, 0o600)
		};
		
		reply.created(&TTL, &FileAttr {
			ino: self.current_inode,
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
		}, 0, 0, 0);
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
		if let Some(size) = size {
			if size > MAX_FILE_SIZE {
				reply.error(EFBIG);
				return;
			}
			
			let content = match self.inodes.get_mut(&ino) {
				None => {
					reply.error(ENOENT);
					return;
				},
				Some(Directory {..}) => {
					reply.error(EISDIR);
					return;
				},
				Some(File {content}) => content,
			};
			
			content.resize(size as usize, 0);
		}
		
		println!("setattr({mode:?}, {uid:?}, {gid:?}, {size:?}, {_atime:?}, {_mtime:?}, {_ctime:?}, {fh:?}, {_crtime:?}, {_chgtime:?}, {_bkuptime:?}, {flags:?})");
		reply.attr(&TTL, &self.attr_for(ino).unwrap());
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
		let content = match self.inodes.get(&ino) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(Directory {..}) => {
				reply.error(EISDIR);
				return;
			},
			Some(File {content}) => content,
		};
		
		let start = cmp::min(offset as usize, content.len());
		let end = cmp::min(offset as usize + size as usize, content.len());
		reply.data(&content[start..end]);
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
		// TODO: can offset be negative? why?
		
		let content = match self.inodes.get_mut(&ino) {
			None => {
				reply.error(ENOENT);
				return;
			},
			Some(Directory {..}) => {
				reply.error(EISDIR);
				return;
			},
			Some(File {content}) => content,
		};
		
		let end = offset as usize + data.len();
		
		if end > content.len() {
			content.resize(end, 0);
		}
		
		let dest: &mut [u8] = &mut content[offset as usize..end];
		dest.copy_from_slice(data);
		
		reply.written(dest.len() as u32);
	}
}
