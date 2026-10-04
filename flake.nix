{
  description = "Arca, a fast and safe archive manager";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      # GPUI on macOS compiles its Metal shaders with Xcode, which the Nix
      # sandbox does not have, so the flake is Linux only.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        system: pkgs: {
          default = self.packages.${system}.arca;
          arca = pkgs.callPackage ./packaging/nix/package.nix { };
        }
      );

      apps = forAllSystems (
        system: pkgs: {
          default = {
            type = "app";
            program = "${self.packages.${system}.arca}/bin/arca-gui";
            meta.description = "Arca desktop window";
          };
          arca = {
            type = "app";
            program = "${self.packages.${system}.arca}/bin/arca";
            meta.description = "Arca command line";
          };
        }
      );

      devShells = forAllSystems (
        system: pkgs: {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.arca ];
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              # interop.sh calls 7z and python3; bench.sh calls 7zz.
              zip
              unzip
              p7zip
              _7zz
              python3
              zstd
              hyperfine
            ];
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath self.packages.${system}.arca.runtimeLibs;
          };
        }
      );

      formatter = forAllSystems (system: pkgs: pkgs.nixfmt);
    };
}
