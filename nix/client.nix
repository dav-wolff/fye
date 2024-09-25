{ callPackage
, craneLib
}:

let
	common = callPackage ./common.nix {
		inherit craneLib;
	};
in craneLib.buildPackage (common.args // {
	inherit (common) cargoArtifacts;
	
	src = common.sourceFor ../client;
	cargoExtraArgs = "-p fye_client";
})
