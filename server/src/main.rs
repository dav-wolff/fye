#![feature(async_closure)]
#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use std::{any::Any, ops::Deref, path::PathBuf};

use axum::{http::StatusCode, response::{IntoResponse, Response}, routing::{get, post}, Router};
use diesel::r2d2::Pool;
use extractors::{AppState, ConnectionManager, Directories};
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;

mod testing;
mod hash;
mod db;
mod stream;
mod extractors;
mod error;
mod routes;

#[tokio::main]
#[expect(clippy::needless_return)]
async fn main() {
	let db_manager = ConnectionManager::new("dev_data/fye.db".to_owned());
	let db_pool = Pool::builder()
		.test_on_check_out(true)
		.build(db_manager).unwrap();
	
	let directories = Directories {
		uploads: PathBuf::from("dev_data/uploads").into(),
		files: PathBuf::from("dev_data/files").into(),
	};
	
	std::fs::create_dir_all(&directories.uploads).unwrap();
	std::fs::create_dir_all(&directories.files).unwrap();
	
	let app_state = AppState::new(db_pool, directories);
	
	let app = Router::new()
		.route("/api/node/:id", get(routes::node_info))
		.route("/api/dir/:id", get(routes::dir_info))
		.route("/api/dir/:id/new-dir", post(routes::create_dir))
		.route("/api/dir/:id/new-file", post(routes::create_file))
		.route("/api/dir/:id/delete-dir", post(routes::delete_dir))
		.route("/api/dir/:id/delete-file", post(routes::delete_file))
		.route("/api/file/:id", get(routes::file_info))
		.route("/api/file/:id/data", get(routes::file_data).put(routes::write_file_data))
		.layer(CatchPanicLayer::custom(handle_panic))
		.with_state(app_state);
	
	let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
	axum::serve(listener, app).await.unwrap();
}

fn handle_panic(panic: Box<dyn Any + Send>) -> Response {
	if let Some(str) = panic.downcast_ref::<&str>().copied()
		.or_else(|| panic.downcast_ref::<String>().map(Deref::deref))
	{
		eprintln!("Panic in route: {str}");
	} else {
		eprintln!("Panic in route (no message)")
	}
	
	StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
