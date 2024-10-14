{ lib
, callPackage
, craneLib
, openssl
}:

let
	common = callPackage ./common.nix {
		inherit craneLib;
	};
in craneLib.buildPackage (common.args // {
	inherit (common) cargoArtifacts;
	
	src = common.sourceFor ../client;
	cargoExtraArgs = "--bin fye";
	
	postFixup = ''
		wrapProgram $out/bin/${common.pname} \
			--set LD_LIBRARY_PATH ${lib.makeLibraryPath [
				openssl
			]}
	'';
})
