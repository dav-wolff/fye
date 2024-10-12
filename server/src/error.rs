use axum::{http::{header, StatusCode}, response::{IntoResponse, Response}};
use diesel::{result::Error as DieselError, Connection, SqliteConnection};

#[derive(Clone, Debug)]
pub enum Error {
	Database,
	IO,
	NotFound,
	NotAFile,
	NotADirectory,
	AlreadyExists(String),
	DirectoryNotEmpty,
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		use Error::*;
		
		match self {
			Database => StatusCode::SERVICE_UNAVAILABLE.into_response(),
			IO => StatusCode::INTERNAL_SERVER_ERROR.into_response(), // TODO: is there a more appropriate status code
			NotFound => StatusCode::NOT_FOUND.into_response(),
			NotAFile => (StatusCode::CONFLICT, "Not A File").into_response(),
			NotADirectory => (StatusCode::CONFLICT, "Not A Directory").into_response(),
			AlreadyExists(location) => (StatusCode::CONFLICT, [(header::LOCATION, location)], "Already Exists").into_response(),
			DirectoryNotEmpty => (StatusCode::CONFLICT, "Directory Not Empty").into_response(),
		}
	}
}

/// Helper function that is useful because diesel's [`Connection::transaction`][`diesel::connection::Connection::transaction`] requires
/// that the error type implements [`From`]<[`diesel::result::Error`]>.
/// 
/// Converts all errors arising from the transaction itself to a [`Error::DbError`].
pub fn transaction<T>(conn: &mut SqliteConnection, callback: impl FnOnce(&mut SqliteConnection) -> Result<T, Error>) -> Result<T, Error> {
	Ok(conn.transaction(|conn| -> Result<T, TransactionError> {
		Ok(callback(conn)?)
	})?)
}

/// Helper type that is useful because diesel's [`Connection::transaction`][`diesel::connection::Connection::transaction`] requires
/// that the error type implements [`From`]<[`diesel::result::Error`]>.
/// 
/// If used directly it is easy to accidently convert any [`diesel::result::Error`]
/// into a [`Error::DbError`] without considering if that's the correct error.
/// To avoid this use [`transaction`] instead.
#[derive(Debug)]
pub enum TransactionError {
	ApplicationError(Error),
	DbError(#[allow(unused)] DieselError),
}

impl From<Error> for TransactionError {
	fn from(value: Error) -> Self {
		Self::ApplicationError(value)
	}
}

impl From<DieselError> for TransactionError {
	fn from(value: DieselError) -> Self {
		Self::DbError(value)
	}
}

impl From<TransactionError> for Error {
	fn from(value: TransactionError) -> Self {
		match value {
			TransactionError::ApplicationError(err) => err,
			TransactionError::DbError(_) => Error::Database,
		}
	}
}
