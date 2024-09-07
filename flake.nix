{
	inputs = {
		nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
		flake-utils.url = "github:numtide/flake-utils";
		
		crane = {
			url = "github:ipetkov/crane";
			inputs.nixpkgs.follows = "nixpkgs";
		};
		
		fenix = {
			url = "github:nix-community/fenix";
			inputs.nixpkgs.follows = "nixpkgs";
		};
	};
	
	outputs = { self, nixpkgs, flake-utils, ... } @ inputs: let
		makeCraneLib = pkgs: let
			fenix = inputs.fenix.packages.${pkgs.system};
			fenixToolchain = fenix.stable.defaultToolchain;
		in (inputs.crane.mkLib pkgs).overrideToolchain fenixToolchain;
	in {
		overlays = {
			fye = final: prev: {
				fye = {
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
				inherit (pkgs.fye) client;
			};
			
			devShells.default = craneLib.devShell {
				packages = with pkgs; [
					rust-analyzer
				];
			};
		}
	);
}
