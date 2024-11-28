use std::{backtrace::{Backtrace, BacktraceStatus}, fmt::{Debug, Display, Formatter}};

use axum::{http::{header, StatusCode}, response::{IntoResponse, Response}};
use diesel::{connection::{AnsiTransactionManager, TransactionManager}, result::Error as DieselError, Connection, SqliteConnection};

#[derive(PartialEq, Debug)]
pub enum Error {
	Internal(Box<InternalError>),
	BadRequest,
	HashMissing,
	NotFound,
	NotAFile,
	NotADirectory,
	AlreadyExists(String),
	DirectoryNotEmpty,
	Modified,
	NotModified,
}

pub struct InternalError {
	context: String,
	cause: Box<dyn std::error::Error + Send + Sync>,
	backtrace: Backtrace,
}

impl Debug for InternalError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}: {}", self.context, self.cause)
	}
}

impl Display for InternalError {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		if self.backtrace.status() == BacktraceStatus::Captured {
			writeln!(f, "Backtrace for \"{}\":", self.context)?;
			writeln!(f, "{}", self.backtrace)?;
		}
		
		writeln!(f, "{self:?}")
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Internal(internal_error) => write!(f, "{internal_error:?}"),
			_ => Debug::fmt(self, f),
		}
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Internal(internal_error) => Some(&*internal_error.cause),
			_ => None,
		}
	}
}

impl PartialEq for InternalError {
	fn eq(&self, _other: &Self) -> bool {
		// can't compare inner errors
		false // like NaN != NaN
	}
}

impl Error {
	pub fn internal<E>(cause: E, context: impl Into<String>) -> Self
	where
		E: std::error::Error + Send + Sync + 'static,
	{
		Self::Internal(Box::new(InternalError {
			context: context.into(),
			cause: Box::new(cause),
			backtrace: Backtrace::capture(),
		}))
	}
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		use Error::*;
		
		match self {
			BadRequest => StatusCode::BAD_REQUEST.into_response(),
			HashMissing => StatusCode::PRECONDITION_REQUIRED.into_response(),
			NotFound => StatusCode::NOT_FOUND.into_response(),
			NotAFile => (StatusCode::CONFLICT, "Not A File").into_response(),
			NotADirectory => (StatusCode::CONFLICT, "Not A Directory").into_response(),
			AlreadyExists(location) => (StatusCode::CONFLICT, [(header::LOCATION, location)], "Already Exists").into_response(),
			DirectoryNotEmpty => (StatusCode::CONFLICT, "Directory Not Empty").into_response(),
			Modified => StatusCode::PRECONDITION_FAILED.into_response(),
			NotModified => StatusCode::NOT_MODIFIED.into_response(),
			Internal(internal_error) => {
				eprintln!("{internal_error}");
				StatusCode::INTERNAL_SERVER_ERROR.into_response()
			},
		}
	}
}

/// Helper function that is useful because diesel's [`Connection::transaction`][`diesel::connection::Connection::transaction`] requires
/// that the error type implements [`From`]<[`diesel::result::Error`]>.
/// 
/// Converts all errors arising from the transaction itself to a [`Error::DbError`].
pub fn transaction<T, C>(conn: &mut SqliteConnection, callback: C) -> Result<T, Error>
where
	C: FnOnce(&mut SqliteConnection) -> Result<T, Error>,
{
	let result = conn.transaction(|conn| -> Result<T, TransactionError> {
		Ok(callback(conn)?)
	});
	
	match result {
		Ok(val) => Ok(val),
		Err(TransactionError::ApplicationError(err)) => Err(err),
		Err(TransactionError::DbError(err)) => Err(Error::internal(err, "error managing transaction")),
	}
}

// TODO: not cancel-safe
pub async fn async_transaction<T, C>(conn: &mut SqliteConnection, callback: C) -> Result<T, Error>
where
	C: async FnOnce(&mut SqliteConnection) -> Result<T, Error>,
{
	AnsiTransactionManager::begin_transaction(conn).map_err(|err| Error::internal(err, "could not begin transaction"))?;
	
	match callback(conn).await {
		Ok(result) => {
			AnsiTransactionManager::commit_transaction(conn).map_err(|err| Error::internal(err, "could not commit transaction"))?;
			Ok(result)
		},
		Err(error) => match AnsiTransactionManager::rollback_transaction(conn) {
			Ok(()) | Err(DieselError::BrokenTransactionManager) => Err(error),
			Err(err) => Err(Error::internal(err, format!("could not rollback transaction after error: {error}"))),
		}
	}
}

/// Helper type that is useful because diesel's [`Connection::transaction`][`diesel::connection::Connection::transaction`] requires
/// that the error type implements [`From`]<[`diesel::result::Error`]>.
/// 
/// If used directly it is easy to accidently convert any [`diesel::result::Error`]
/// into a [`Error::DbError`] without considering if that's the correct error.
/// To avoid this use [`transaction`] instead.
#[derive(Debug)]
enum TransactionError {
	ApplicationError(Error),
	DbError(DieselError),
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
