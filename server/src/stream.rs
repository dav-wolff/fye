use std::{future::Future, io, pin::Pin, task::{Context, Poll}};

use blake3::{Hash, Hasher};
use bytes::Bytes;
use futures::Stream;
use tokio::{fs::File, io::AsyncWrite};
use pin_project::pin_project;

pub fn stream_to_file<S: Stream<Item = Result<Bytes, io::Error>>>(file: File, stream: S) -> impl Future<Output = Result<(), io::Error>> {
	StreamToFile {
		file,
		stream,
		current_bytes: Bytes::new(),
	}
}

#[pin_project]
struct StreamToFile<S: Stream<Item = Result<Bytes, io::Error>>> {
	#[pin]
	stream: S,
	#[pin]
	file: File,
	current_bytes: Bytes,
}

impl<S: Stream<Item = Result<Bytes, io::Error>>> StreamToFile<S> {
	fn write_current_bytes(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
		let this = self.project();
		let mut file = this.file;
		let current_bytes = this.current_bytes;
		
		loop {
			if current_bytes.is_empty() {
				return Poll::Ready(Ok(()));
			}
			
			let bytes_written = match file.as_mut().poll_write(cx, current_bytes) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
				Poll::Ready(Ok(bytes_written)) => bytes_written,
			};
			
			*current_bytes = current_bytes.slice(bytes_written..);
		}
	}
}

impl<S: Stream<Item = Result<Bytes, io::Error>>> Future for StreamToFile<S> {
	type Output = Result<(), io::Error>;
	
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		loop {
			match self.as_mut().write_current_bytes(cx) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
				Poll::Ready(Ok(())) => (),
			}
			
			let this = self.as_mut().project();
			let mut stream = this.stream;
			let current_bytes = this.current_bytes;
			
			*current_bytes = match stream.as_mut().poll_next(cx) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(None) => return Poll::Ready(Ok(())),
				Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err)),
				Poll::Ready(Some(Ok(bytes))) => bytes,
			};
		}
	}
}

#[pin_project]
pub struct HashStream<S: Stream<Item = Result<Bytes, io::Error>>> {
	#[pin]
	inner: S,
	hasher: Hasher,
}

impl<S: Stream<Item = Result<Bytes, io::Error>>> HashStream<S> {
	pub fn new(stream: S) -> Self {
		Self {
			inner: stream,
			hasher: Hasher::new(),
		}
	}
	
	pub fn total_size(&self) -> u64 {
		self.hasher.count()
	}
	
	pub fn hash(&self) -> Hash {
		self.hasher.finalize()
	}
}

impl<S: Stream<Item = Result<Bytes, io::Error>>> Stream for HashStream<S> {
	type Item = Result<Bytes, io::Error>;
	
	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.project();
		let inner = this.inner;
		let hasher = this.hasher;
		
		match inner.poll_next(cx) {
			Poll::Ready(Some(Ok(bytes))) => {
				hasher.update(&bytes);
				Poll::Ready(Some(Ok(bytes)))
			},
			result => result,
		}
	}
	
	fn size_hint(&self) -> (usize, Option<usize>) {
		self.inner.size_hint()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::*;
	
	use std::pin::pin;
	use futures::StreamExt;
	use crate::hash::EMPTY_HASH;
	
	#[tokio::test]
	async fn hash_empty() {
		let stream = bytes_stream_from(&[]);
		let mut hash_stream = pin!(HashStream::new(stream));
		
		assert!(hash_stream.next().await.is_none());
		
		assert_eq!(hash_stream.total_size(), 0);
		assert_eq!(&hash_stream.hash().to_hex(), EMPTY_HASH);
	}
	
	#[tokio::test]
	async fn hash_single_chunk() {
		let stream = bytes_stream_from(&[b"Hello"]);
		let mut hash_stream = pin!(HashStream::new(stream));
		
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"Hello"[..]);
		assert!(hash_stream.next().await.is_none());
		
		assert_eq!(hash_stream.total_size(), b"Hello".len() as u64);
		assert_eq!(hash_stream.hash(), blake3::hash(b"Hello"));
	}
	
	#[tokio::test]
	async fn hash_multiple_chunks() {
		let stream = bytes_stream_from(&[b"Multiple ", b"chunks ", b"in ", b"this ", b"slice"]);
		let mut hash_stream = pin!(HashStream::new(stream));
		
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"Multiple "[..]);
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"chunks "[..]);
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"in "[..]);
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"this "[..]);
		assert_eq!(hash_stream.next().await.unwrap().unwrap(), &b"slice"[..]);
		assert!(hash_stream.next().await.is_none());
		
		assert_eq!(hash_stream.total_size(), b"Multiple chunks in this slice".len() as u64);
		assert_eq!(hash_stream.hash(), blake3::hash(b"Multiple chunks in this slice"));
	}
}
