#![allow(clippy::enum_variant_names)]
#![allow(clippy::wildcard_in_or_patterns)]

use reqwest::{RequestBuilder, Response, StatusCode};

#[derive(Debug)]
pub enum NetworkError {
	Timeout,
	Other,
}

#[derive(Debug)]
pub enum Error {
	NetworkFailure(NetworkError),
	ServerError,
	ProtocolMismatch,
	NotFound,
	NotAFile,
	NotADirectory,
	AlreadyExists,
}

impl Error {
	pub fn network_error(err: reqwest::Error) -> Self {
		match err.is_timeout() {
			true => Error::NetworkFailure(NetworkError::Timeout),
			false => Error::NetworkFailure(NetworkError::Other),
		}
	}
}

pub async fn decode_errors(request: RequestBuilder, expected_status: StatusCode) -> Result<Response, Error> {
	let response = request.send().await.map_err(Error::network_error)?;
	
	if response.status() == expected_status {
		return Ok(response);
	}
	
	Err(match response.status() {
		StatusCode::BAD_GATEWAY => Error::NetworkFailure(NetworkError::Other),
		StatusCode::INTERNAL_SERVER_ERROR => Error::ServerError,
		StatusCode::NOT_FOUND => Error::NotFound,
		StatusCode::CONFLICT => {
			let body = response.bytes().await.map_err(Error::network_error)?;
			
			match &body[..] {
				b"Not A File" => Error::NotAFile,
				b"Not A Directory" => Error::NotADirectory,
				b"Already Exists" => Error::AlreadyExists,
				_ => Error::ProtocolMismatch,
			}
		},
		_ => Error::ProtocolMismatch,
	})
}

#[derive(Debug)]
pub enum FetchNodeError {
	NetworkFailure(NetworkError),
	ServerError,
	ProtocolMismatch,
	NotFound,
}

impl From<Error> for FetchNodeError {
	fn from(value: Error) -> Self {
		use Error::*;
		
		match value {
			NetworkFailure(err) => Self::NetworkFailure(err),
			ServerError => Self::ServerError,
			NotFound => Self::NotFound,
			ProtocolMismatch | _ => Self::ProtocolMismatch,
		}
	}
}

#[derive(Debug)]
pub enum FetchDirectoryError {
	NetworkFailure(NetworkError),
	ServerError,
	ProtocolMismatch,
	NotFound,
	NotADirectory,
}

impl From<Error> for FetchDirectoryError {
	fn from(value: Error) -> Self {
		use Error::*;
		
		match value {
			NetworkFailure(err) => Self::NetworkFailure(err),
			ServerError => Self::ServerError,
			NotFound => Self::NotFound,
			NotADirectory => Self::NotADirectory,
			ProtocolMismatch | _ => Self::ProtocolMismatch,
		}
	}
}

#[derive(Debug)]
pub enum FetchFileError {
	NetworkFailure(NetworkError),
	ServerError,
	ProtocolMismatch,
	NotFound,
	NotAFile,
}

impl From<Error> for FetchFileError {
	fn from(value: Error) -> Self {
		use Error::*;
		
		match value {
			NetworkFailure(err) => Self::NetworkFailure(err),
			ServerError => Self::ServerError,
			NotFound => Self::NotFound,
			NotAFile => Self::NotAFile,
			ProtocolMismatch | _ => Self::ProtocolMismatch,
		}
	}
}

#[derive(Debug)]
pub enum CreateNodeError {
	NetworkFailure(NetworkError),
	ServerError,
	ProtocolMismatch,
	ParentNotFound,
	ParentNotADirectory,
	AlreadyExists,
}

impl From<Error> for CreateNodeError {
	fn from(value: Error) -> Self {
		use Error::*;
		
		match value {
			NetworkFailure(err) => Self::NetworkFailure(err),
			ServerError => Self::ServerError,
			NotFound => Self::ParentNotFound,
			NotADirectory => Self::ParentNotADirectory,
			AlreadyExists => Self::AlreadyExists,
			ProtocolMismatch | _ => Self::ProtocolMismatch,
		}
	}
}
