{
	inputs = {
		nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
		flake-utils.url = "github:numtide/flake-utils";
		
		crane = {
			url = "github:ipetkov/crane";
		};
		
		fenix = {
			url = "github:nix-community/fenix";
			inputs.nixpkgs.follows = "nixpkgs";
		};
	};
	
	outputs = { self, nixpkgs, flake-utils, ... } @ inputs: let
		makeCraneLib = pkgs: let
			fenix = inputs.fenix.packages.${pkgs.system};
			fenixToolchain = fenix.default.withComponents [
				"rustc"
				"cargo"
				"rust-std"
				"rust-docs"
				"clippy"
			];
		in (inputs.crane.mkLib pkgs).overrideToolchain fenixToolchain;
	in {
		overlays = {
			fye = final: prev: {
				fye = {
					server = prev.callPackage ./nix/server.nix {
						craneLib = makeCraneLib final;
					};
					
					client = prev.callPackage ./nix/client.nix {
						craneLib = makeCraneLib final;
					};
				};
			};
			
			default = self.overlays.fye;
		};
	} // flake-utils.lib.eachDefaultSystem (system:
		let
			pkgs = import nixpkgs {
				inherit system;
				overlays = [self.overlays.default];
			};
			craneLib = makeCraneLib pkgs;
		in {
			packages = {
				inherit (pkgs.fye) server client;
			};
			
			devShells.default = craneLib.devShell {
				packages = with pkgs; [
					rust-analyzer
					diesel-cli
					pkg-config
					openssl
					sqlite
				];
				
				LD_LIBRARY_PATH = with pkgs; lib.makeLibraryPath [
					openssl
					sqlite
				];
				
				DATABASE_URL = "dev_data/fye.db";
			};
		}
	);
}
