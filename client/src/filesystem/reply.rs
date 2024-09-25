use std::{future::Future, time::Duration};

use fuser::{FileAttr, FileType, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEntry, ReplyWrite};

use crate::maybe_async::MaybeAsync;

#[derive(Debug)]
pub enum Error {
	NoEnt,
	NotDir,
	IsDir,
	Exist,
	FBig,
	IlSeq,
	NotSup,
	TimedOut,
	NoLink,
	IO,
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
			TimedOut => ETIMEDOUT,
			NoLink => ENOLINK,
			IO => EIO,
		}
	}
}

pub fn respond<T, F, R, C>(reply: R, f: C)
where
	R: Reply<T> + Send + 'static,
	F: Future<Output = Result<T, Error>> + Send + 'static,
	C: FnOnce() -> MaybeAsync<Result<T, Error>, F>,
{
	let resolve = |result| match result {
		Ok(val) => reply.ok(val),
		Err(err) => reply.error(err),
	};
	
	match f() {
		MaybeAsync::Sync(result) => resolve(result),
		MaybeAsync::Async(future) => {
			tokio::spawn(async {
				let result = future.await;
				resolve(result);
			});
		},
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
pub struct DirectoryReplyEntry {
	pub ino: u64,
	pub name: String,
	pub offset: i64,
	pub kind: FileType,
}

impl<I> Reply<I> for ReplyDirectory
where
	I: IntoIterator<Item = DirectoryReplyEntry>,
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

impl<T> Reply<T> for ReplyData
where
	T: AsRef<[u8]>,
{
	fn ok(self, val: T) {
		self.data(val.as_ref());
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
