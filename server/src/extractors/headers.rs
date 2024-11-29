use std::{convert::Infallible, future};

use axum::{extract::FromRequestParts, http::{header, request::Parts, HeaderName, HeaderValue}, response::{IntoResponse, IntoResponseParts, Response, ResponseParts}};
use futures::FutureExt;
use fye_shared::{Hash, NodeID};

use crate::error::Error;

use super::BoxedFuture;

pub trait HeaderType: 'static {
	type Data: Send;
	
	const HEADER_NAME: HeaderName;
	#[allow(clippy::declare_interior_mutable_const, reason = "false positive")]
	const MISSING_ERROR: Error;
	
	fn parse(header_value: &HeaderValue) -> Result<Self::Data, Error>;
	fn encode(data: Self::Data) -> HeaderValue;
}

#[derive(Debug)]
pub struct Header<H: HeaderType>(pub H::Data);

impl<S, H: HeaderType> FromRequestParts<S> for Header<H> {
	type Rejection = Error;
	
	fn from_request_parts<'p, 's, 'f>(parts: &mut Parts, _state: &'s S) -> BoxedFuture<'f, Result<Self, Self::Rejection>>
	where
		's: 'f,
		'p: 'f,
	{
		let result = parts.headers.get(H::HEADER_NAME).ok_or(H::MISSING_ERROR)
			.and_then(H::parse)
			.map(Self);
		future::ready(result).boxed()
	}
}

impl<H: HeaderType> IntoResponseParts for Header<H> {
	type Error = Infallible;
	
	fn into_response_parts(self, mut response: ResponseParts) -> Result<ResponseParts, Self::Error> {
		response.headers_mut().insert(H::HEADER_NAME, H::encode(self.0));
		
		Ok(response)
	}
}

impl <H: HeaderType> IntoResponse for Header<H> {
	fn into_response(self) -> Response {
		(self, ()).into_response()
	}
}

#[derive(Debug)]
pub struct OptHeader<H: HeaderType>(pub Option<H::Data>);

impl<S, H: HeaderType> FromRequestParts<S> for OptHeader<H> {
	type Rejection = Error;
	
	fn from_request_parts<'p, 's, 'f>(parts: &mut Parts, _state: &'s S) -> BoxedFuture<'f, Result<Self, Self::Rejection>>
	where
		's: 'f,
		'p: 'f,
	{
		let result = parts.headers.get(H::HEADER_NAME)
			.map(H::parse)
			.transpose()
			.map(Self);
		future::ready(result).boxed()
	}
}

#[derive(Debug)]
pub struct IfMatch;

impl HeaderType for IfMatch {
	type Data = Hash;
	
	const HEADER_NAME: HeaderName = header::IF_MATCH;
	const MISSING_ERROR: Error = Error::HashMissing;
	
	fn parse(header_value: &HeaderValue) -> Result<Self::Data, Error> {
		Hash::from_header(header_value).ok_or(Error::BadRequest)
	}
	
	fn encode(data: Self::Data) -> HeaderValue {
		data.to_header()
	}
}

#[derive(Debug)]
pub struct IfNoneMatch;

impl HeaderType for IfNoneMatch {
	type Data = Hash;
	
	const HEADER_NAME: HeaderName = header::IF_NONE_MATCH;
	const MISSING_ERROR: Error = Error::HashMissing;
	
	fn parse(header_value: &HeaderValue) -> Result<Self::Data, Error> {
		Hash::from_header(header_value).ok_or(Error::BadRequest)
	}
	
	fn encode(data: Self::Data) -> HeaderValue {
		data.to_header()
	}
}

#[derive(Debug)]
pub struct ETag;

impl HeaderType for ETag {
	type Data = Hash;
	
	const HEADER_NAME: HeaderName = header::ETAG;
	const MISSING_ERROR: Error = Error::HashMissing;
	
	fn parse(header_value: &HeaderValue) -> Result<Self::Data, Error> {
		Hash::from_header(header_value).ok_or(Error::BadRequest)
	}
	
	fn encode(data: Self::Data) -> HeaderValue {
		data.to_header()
	}
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Location {
	Directory(NodeID),
	File(NodeID),
}

impl HeaderType for Location {
	type Data = Self;
	
	const HEADER_NAME: HeaderName = header::LOCATION;
	const MISSING_ERROR: Error = Error::BadRequest;
	
	fn parse(_header_value: &HeaderValue) -> Result<Self::Data, Error> {
		unimplemented!("currently not used")
	}
	
	fn encode(data: Self::Data) -> HeaderValue {
		match data {
			Self::Directory(id) => format!("/api/dir/{id}"),
			Self::File(id) => format!("/api/file/{id}"),
		}.parse().expect("should be a valid header value")
	}
}
