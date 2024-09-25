use axum::{http::{header, StatusCode}, response::{IntoResponse, Response}};

#[derive(Clone, Debug)]
pub enum Error {
	NotFound,
	NotAFile,
	NotADirectory,
	AlreadyExists(String),
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		use Error::*;
		
		match self {
			NotFound => StatusCode::NOT_FOUND.into_response(),
			NotAFile => (StatusCode::CONFLICT, "Not A File").into_response(),
			NotADirectory => (StatusCode::CONFLICT, "Not A Directory").into_response(),
			AlreadyExists(location) => (StatusCode::CONFLICT, [(header::LOCATION, location)], "Already Exists").into_response(),
		}
	}
}
