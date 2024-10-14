{ lib
, callPackage
, craneLib
, openssl
, sqlite
}:

let
	common = callPackage ./common.nix {
		inherit craneLib;
	};
in craneLib.buildPackage (common.args // {
	inherit (common) cargoArtifacts;
	
	pname = "${common.pname}-server";
	src = common.sourceFor ../server;
	cargoExtraArgs = "--bin fye-server";
	
	postFixup = ''
		wrapProgram $out/bin/${common.pname}-server \
			--set LD_LIBRARY_PATH ${lib.makeLibraryPath [
				openssl
				sqlite
			]}
	'';
})
