#![forbid(unsafe_code)]
#![deny(non_snake_case)]
#![allow(unused)]

use std::{io, path::Path};

mod filesystem;
use filesystem::FyeFilesystem;

pub fn mount(mountpoint: impl AsRef<Path>) -> Result<(), io::Error> {
	let filesystem = FyeFilesystem::new();
	
	fuser::mount2(filesystem, &mountpoint, &[])?;
	
	Ok(())
}
