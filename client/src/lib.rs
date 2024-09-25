#![feature(try_trait_v2)]
#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use std::{io, path::Path};

mod maybe_async;

mod remote_data_service;
mod filesystem;

use filesystem::FyeFilesystem;
use remote_data_service::RemoteDataService;
use reqwest::Url;
use tokio::runtime::Runtime;

pub fn mount(mountpoint: impl AsRef<Path>) -> Result<(), io::Error> {
	let runtime = Runtime::new().unwrap();
	let _guard = runtime.enter();
	
	let remote_data_fetcher = RemoteDataService::new(Url::parse("http://localhost:3000/api/").unwrap());
	let filesystem = FyeFilesystem::new(remote_data_fetcher);
	
	fuser::mount2(filesystem, &mountpoint, &[])?;
	
	Ok(())
}
