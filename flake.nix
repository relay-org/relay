{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      forAllSystems = function:
        nixpkgs.lib.genAttrs [
          "aarch64-linux"
          "x86_64-linux"
          "aarch64-darwin"
          "x86_64-darwin"
        ] (system: function system);
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./toolchain.toml;
        in
        {
          default = pkgs.mkShell rec {
            buildInputs =
              [ toolchain pkgs.pkg-config pkgs.sqlite ]
              ++
              pkgs.lib.optionals
                (pkgs.stdenv.hostPlatform.system == "aarch64-darwin"
                  || pkgs.stdenv.hostPlatform.system == "x86_64-darwin")
                (with pkgs.darwin.apple_sdk.frameworks; [
                  Security SystemConfiguration
                ]);

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
          };
        }
      );
    };
}
