#![feature(async_closure)]
#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use std::{io, path::Path};
use reqwest::Url;
use tokio::runtime::Runtime;

mod remote_data_service;
mod local_file_cache;
mod filesystem;

use remote_data_service::RemoteDataService;
use local_file_cache::LocalFileCache;
use filesystem::FyeFilesystem;

pub fn mount(mountpoint: impl AsRef<Path>) -> Result<(), io::Error> {
	let runtime = Runtime::new().unwrap();
	let _guard = runtime.enter();
	
	let remote_data_service = RemoteDataService::new(Url::parse("http://localhost:3000/api/").unwrap());
	let local_file_cache = LocalFileCache::new(remote_data_service);
	let filesystem = FyeFilesystem::new(local_file_cache);
	
	fuser::mount2(filesystem, &mountpoint, &[])?;
	
	Ok(())
}
