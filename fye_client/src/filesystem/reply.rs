use std::time::Duration;

use fuser::{FileAttr, FileType, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEntry, ReplyWrite};

#[derive(Debug)]
pub enum Error {
	NoEnt,
	NotDir,
	IsDir,
	Exist,
	FBig,
	IlSeq,
	NotSup,
}

impl From<Error> for i32 {
	fn from(value: Error) -> Self {
		use Error::*;
		use libc::*;
		
		match value {
			NoEnt => ENOENT,
			NotDir => ENOTDIR,
			IsDir => EISDIR,
			Exist => EEXIST,
			FBig => EFBIG,
			IlSeq => EILSEQ,
			NotSup => ENOTSUP,
		}
	}
}

pub fn respond<T, R, F>(reply: R, f: F)
where
	R: Reply<T>,
	F: FnOnce() -> Result<T, Error>,
{
	match f() {
		Ok(val) => reply.ok(val),
		Err(err) => reply.error(err),
	}
}

pub trait Reply<T> {
	fn ok(self, val: T);
	fn error(self, err: Error);
}

#[derive(Debug)]
pub struct AttrReply {
	pub attr: FileAttr,
	pub ttl: Duration,
}

impl Reply<AttrReply> for ReplyAttr {
	fn ok(self, val: AttrReply) {
		self.attr(&val.ttl, &val.attr);
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}

#[derive(Debug)]
pub struct EntryReply {
	pub attr: FileAttr,
	pub ttl: Duration,
	pub generation: u64,
}

impl Reply<EntryReply> for ReplyEntry {
	fn ok(self, val: EntryReply) {
		self.entry(&val.ttl, &val.attr, val.generation);
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}

#[derive(Debug)]
pub struct DirectoryReplyEntry<'a> {
	pub ino: u64,
	pub name: &'a str,
	pub offset: i64,
	pub kind: FileType,
}

impl<'a, I> Reply<I> for ReplyDirectory
where
	I: Iterator<Item = DirectoryReplyEntry<'a>>,
{
	fn ok(mut self, iter: I) {
		for entry in iter {
			if self.add(entry.ino, entry.offset, entry.kind, entry.name) {
				break;
			}
		}
		
		self.ok();
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}

#[derive(Debug)]
pub struct CreateReply {
	pub attr: FileAttr,
	pub ttl: Duration,
	pub generation: u64,
	pub fh: u64,
	pub flags: u32,
}

impl Reply<CreateReply> for ReplyCreate {
	fn ok(self, val: CreateReply) {
		self.created(&val.ttl, &val.attr, val.generation, val.fh, val.flags);
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}

impl<'a> Reply<&'a [u8]> for ReplyData {
	fn ok(self, val: &'a [u8]) {
		self.data(val);
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}

impl Reply<u32> for ReplyWrite {
	fn ok(self, val: u32) {
		self.written(val);
	}
	
	fn error(self, err: Error) {
		self.error(err.into());
	}
}
