{ callPackage
, craneLib
}:

let
	common = callPackage ./common.nix {
		inherit craneLib;
	};
in craneLib.buildPackage (common.args // {
	inherit (common) cargoArtifacts;
	
	pname = "${common.pname}-server";
	src = common.sourceFor ../server;
	cargoExtraArgs = "-p fye_server";
})
