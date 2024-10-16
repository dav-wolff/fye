#![forbid(unsafe_code)]
#![deny(non_snake_case)]

use std::{collections::BTreeMap, fmt::Display, num::ParseIntError, str::FromStr};

use http::HeaderValue;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct NodeID(pub u64);

impl NodeID {
	pub const ROOT: NodeID = NodeID(1);
	
	pub fn increment(self) -> Self {
		match self {
			Self(u64::MAX) => {
				Self(Self::ROOT.0 + 1)
			},
			Self(id) => {
				Self(id + 1)
			},
			
		}
	}
}

impl Display for NodeID {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}

impl FromStr for NodeID {
	type Err = ParseIntError;
	
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(u64::from_str(s)?))
	}
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct DirectoryInfo {
	pub parent: NodeID,
	pub children: BTreeMap<String, NodeID>,
}

impl DirectoryInfo {
	pub fn with_parent(parent: NodeID) -> Self {
		Self {
			parent,
			children: Default::default(),
		}
	}
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Hash(pub String);

impl Hash {
	pub fn parse_header(header: &HeaderValue) -> Option<&str> {
		let str = header.to_str().ok()?;
		
		let str = str.strip_prefix('"')?
			.strip_suffix('"')?;
		
		Some(str)
	}
	
	pub fn from_header(header: &HeaderValue) -> Option<Self> {
		Some(Self(Self::parse_header(header)?.to_owned()))
	}
	
	pub fn to_header(&self) -> HeaderValue {
		format!("\"{}\"", self.0)
			.parse().expect("should be a valid header value")
	}
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct FileInfo {
	pub size: u64,
	pub hash: Hash,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub enum NodeInfo {
	Directory(DirectoryInfo),
	File(FileInfo),
}
