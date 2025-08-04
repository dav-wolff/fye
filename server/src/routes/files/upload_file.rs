use std::{io, ops::{Deref, DerefMut}, path::{Path, PathBuf}};

use tokio::fs::{self, File, OpenOptions};

pub struct UploadFile {
	path: PathBuf,
	file: File,
	is_moved: bool,
}

impl UploadFile {
	pub async fn new(path: PathBuf) -> Result<Self, io::Error> {
		let file = OpenOptions::new()
			.write(true)
			.create(true)
			.open(&path).await?;
		
		Ok(Self {
			path,
			file,
			is_moved: false,
		})
	}
	
	pub async fn move_to(mut self, destination: impl AsRef<Path>) -> Result<(), io::Error> {
		fs::rename(&self.path, destination).await?;
		self.is_moved = true;
		Ok(())
	}
}

impl Drop for UploadFile {
	fn drop(&mut self) {
		if self.is_moved {
			return;
		}
		
		let path = std::mem::take(&mut self.path);
		
		tokio::task::spawn_blocking(move || {
			if let Err(err) = std::fs::remove_file(&path) {
				eprintln!("could not clean up file at {}: {err}", path.as_os_str().to_string_lossy());
			}
		});
	}
}

impl Deref for UploadFile {
	type Target = File;
	
	fn deref(&self) -> &Self::Target {
		&self.file
	}
}

impl DerefMut for UploadFile {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.file
	}
}
