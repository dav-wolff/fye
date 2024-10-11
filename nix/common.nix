{ lib
, craneLib
, pkg-config
, openssl
, sqlite
}:

let
	src = with lib; cleanSourceWith {
		src = craneLib.path ../.;
		filter = craneLib.filterCargoSources;
	};
	
	nameVersion = craneLib.crateNameFromCargoToml { inherit src; };
	
	args = {
		inherit (nameVersion) pname version;
		
		nativeBuildInputs = [
			pkg-config
		];
		
		buildInputs = [
			openssl
			sqlite
		];
	};
in {
	inherit (nameVersion) pname version;
	inherit args;
	
	cargoArtifacts = craneLib.buildDepsOnly (args // {
		inherit src;
	});
	
	sourceFor = crateSrc: lib.fileset.toSource {
		root = ../.;
		fileset = lib.fileset.unions [
			../Cargo.toml
			../Cargo.lock
			../shared
			../server/Cargo.toml
			../client/Cargo.toml
			crateSrc
		];
	};
}
