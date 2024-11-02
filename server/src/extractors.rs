use std::{convert::Infallible, future::{self, Future}, marker::{PhantomData, Send}, ops::{Deref, DerefMut}, path::Path, pin::Pin, sync::{atomic::{AtomicBool, Ordering}, Arc}};

use axum::{extract::FromRequestParts, http::request::Parts};
use diesel::{r2d2::R2D2Connection, SqliteConnection};
use futures::FutureExt;
use r2d2::{ManageConnection, Pool, PooledConnection};

use crate::{db, error::Error};

#[derive(Clone, Debug)]
pub struct AppState {
	db_pool: Pool<ConnectionManager>,
	directories: Directories,
}

impl AppState {
	pub fn new(db_pool: Pool<ConnectionManager>, directories: Directories) -> Self {
		Self {
			db_pool,
			directories,
		}
	}
}

#[derive(Clone, Debug)]
pub struct Directories {
	pub uploads: Arc<Path>,
	pub files: Arc<Path>,
}

impl FromRequestParts<AppState> for Directories {
	type Rejection = Infallible;
	
	fn from_request_parts<'p, 's, 'f>(_parts: &mut Parts, state: &'s AppState) -> BoxedFuture<'f, Result<Self, Self::Rejection>>
	where
		's: 'f,
		'p: 'f,
	{
		future::ready(Ok(state.directories.clone())).boxed()
	}
}

#[derive(Debug)]
pub struct ConnectionManager {
	url: String,
	did_run_migrations: AtomicBool,
}

impl ConnectionManager {
	pub fn new(db_url: String) -> Self {
		Self {
			url: db_url,
			did_run_migrations: false.into(),
		}
	}
}

impl ManageConnection for ConnectionManager {
	type Connection = SqliteConnection;
	type Error = diesel::r2d2::Error;
	
	fn connect(&self) -> Result<Self::Connection, Self::Error> {
		let did_run_migrations = self.did_run_migrations.load(Ordering::SeqCst);
		let result = db::establish_connection(&self.url, !did_run_migrations);
		
		if !did_run_migrations {
			// possible race leading to migrations being run twice which is fine
			self.did_run_migrations.store(true, Ordering::SeqCst);
		}
		
		result
	}
	
	fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
		conn.ping().map_err(diesel::r2d2::Error::QueryError)
	}
	
	fn has_broken(&self, conn: &mut Self::Connection) -> bool {
		std::thread::panicking() || conn.is_broken()
	}
}

pub struct DbConnection<'a>(ConnectionKind<'a>);

impl<'a> DbConnection<'a> {
	fn new(conn: PooledConnection<ConnectionManager>) -> Self {
		Self(ConnectionKind::Pooled(conn, PhantomData))
	}
	
	#[cfg(test)]
	pub fn from_single(conn: &'a mut SqliteConnection) -> Self {
		Self(ConnectionKind::Single(conn))
	}
}

enum ConnectionKind<'a> {
	Pooled(PooledConnection<ConnectionManager>, PhantomData<&'a ()>),
	#[cfg(test)]
	Single(&'a mut SqliteConnection),
}

type BoxedFuture<'a, O> = Pin<Box<dyn Future<Output = O> + Send + 'a>>;

impl FromRequestParts<AppState> for DbConnection<'static> {
	type Rejection = Error;
	
	fn from_request_parts<'p, 's, 'f>(_parts: &mut Parts, state: &'s AppState) -> BoxedFuture<'s, Result<Self, Self::Rejection>>
	where
		's: 'f,
		'p: 'f,
	{
		async {
			if let Some(conn) = state.db_pool.try_get() {
				return Ok(DbConnection::new(conn));
			}
			
			let db_pool = state.db_pool.clone();
			
			tokio::task::spawn_blocking(move || {
				match db_pool.get() { // may block
					Ok(conn) => Ok(DbConnection::new(conn)),
					Err(_) => Err(Error::Database),
				}
			}).await.expect("db_pool.get() should not panic")
		}.boxed()
	}
}

impl<'a> Deref for ConnectionKind<'a> {
	type Target = SqliteConnection;
	
	fn deref(&self) -> &Self::Target {
		match self {
			ConnectionKind::Pooled(conn, _) => conn.deref(),
			#[cfg(test)]
			ConnectionKind::Single(conn) => conn,
		}
	}
}

impl<'a> DerefMut for ConnectionKind<'a> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		match self {
			ConnectionKind::Pooled(ref mut conn, _) => conn.deref_mut(),
			#[cfg(test)]
			ConnectionKind::Single(ref mut conn) => conn,
		}
	}
}

impl<'a> Deref for DbConnection<'a> {
	type Target = SqliteConnection;
	
	fn deref(&self) -> &Self::Target {
		self.0.deref()
	}
}

impl<'a> DerefMut for DbConnection<'a> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.0.deref_mut()
	}
}
