pub const EMPTY_HASH: &str = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";

#[cfg(test)]
mod tests {
	use super::*;
	use blake3::Hasher;
	
	#[test]
	fn empty_hash() {
		let hasher = Hasher::new();
		let empty_hash = hasher.finalize().to_hex();
		
		assert_eq!(EMPTY_HASH, &empty_hash);
	}
}
