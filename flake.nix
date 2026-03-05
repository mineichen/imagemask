{
  description = "Deterministic Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ { flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];

      perSystem = { pkgs, system, ... }: let
        # Stable toolchain for regular tests
        stableToolchain = with inputs.fenix.packages.${system}; combine [
          stable.toolchain
        ];
        
        # Nightly toolchain for Miri
        nightlyToolchain = inputs.fenix.packages.${system}.latest.withComponents [
          "cargo"
          "clippy"
	  "rustc"
          "rustfmt"
          "miri"
        ];
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            stableToolchain
          ];


          shellHook = ''
            echo "===================================="
            echo " Welcome to the deterministic dev shell! "
            echo "===================================="
            echo "Rust toolchain:"
            rustc --version
            echo "Cargo version:"
            cargo --version
            echo "Ready to develop! 🦀"
          '';
        };

        # Miri devShell with nightly
        devShells.miri = pkgs.mkShell {
          buildInputs = [
            nightlyToolchain
          ];

          shellHook = ''
            echo "Miri dev shell (nightly)"
            rustc --version
          '';
        };
      };
    };
}
