use reqwest::{header, RequestBuilder, Response};
use serde::{de::DeserializeOwned, Serialize};

use super::Error;

pub trait RequestBuilderPostcard {
	fn postcard<T: Serialize + ?Sized>(self, value: &T) -> Self;
}

impl RequestBuilderPostcard for RequestBuilder {
	fn postcard<T: Serialize + ?Sized>(self, value: &T) -> Self {
		self.header(header::CONTENT_TYPE, "application/postcard")
			.body(postcard::to_stdvec(value).expect("should only error if out of memory"))
	}
}

pub trait ResponsePostcard {
	async fn postcard<T: DeserializeOwned>(self) -> Result<T, Error>;
}

impl ResponsePostcard for Response {
	async fn postcard<T: DeserializeOwned>(self) -> Result<T, Error> {
		// TODO: set Content-Type on server and check for it here
		// let content_type = self.headers().get(header::CONTENT_TYPE).ok_or(Error::ProtocolMismatch)?;
		
		// if content_type.as_bytes() != b"application/postcard" {
		// 	return Err(Error::ProtocolMismatch);
		// }
		
		let bytes = self.bytes().await.map_err(Error::network_error)?;
		
		postcard::from_bytes(&bytes).map_err(|_| Error::ProtocolMismatch)
	}
}
