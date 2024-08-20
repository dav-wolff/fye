{ lib
, craneLib
}:

let
	commonArgs = {
		pname = "fye";
		version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;
		
		src = with lib; cleanSourceWith {
			src = craneLib.path ../.;
			filter = craneLib.filterCargoSources;
		};
	};
	
	cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in craneLib.buildPackage commonArgs // {
	inherit cargoArtifacts;
	pname = "fye";
	cargoExtraArgs = "-p fye_client";
}
