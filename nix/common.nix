{ lib
, craneLib
, pkg-config
, clang
, llvmPackages
, openssl
, sqlite
}:

let
	src = with lib; cleanSourceWith {
		src = craneLib.path ../.;
		filter = craneLib.filterCargoSources;
	};
	
	nameVersion = craneLib.crateNameFromCargoToml { inherit src; };
	inherit (nameVersion) pname version;
	
	args = {
		inherit (nameVersion) pname version;
		
		nativeBuildInputs = [
			pkg-config
			clang # needed to use lld
			llvmPackages.bintools # lld
		];
		
		buildInputs = [
			openssl
			sqlite
		];
	};
in {
	inherit pname version args;
	
	cargoArtifacts = craneLib.buildDepsOnly (args // {
		inherit src;
	});
	
	sourceFor = crateSrc: lib.fileset.toSource {
		root = ../.;
		fileset = lib.fileset.unions [
			../Cargo.toml
			../Cargo.lock
			../.cargo
			../shared
			../server/Cargo.toml
			../client/Cargo.toml
			crateSrc
		];
	};
}
