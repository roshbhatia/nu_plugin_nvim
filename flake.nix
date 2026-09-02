{
  description = "Structured data bridge between Nushell and Neovim";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      ...
    }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);
    in
    {
      formatter = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        pkgs.writeShellApplication {
          name = "nuvim-format";
          runtimeInputs = [
            pkgs.cargo
            pkgs.findutils
            pkgs.nixfmt
            pkgs.rustfmt
          ];
          text = ''
            cargo fmt --all
            find . -type f -name '*.nix' -not -path './.git/*' -print0 \
              | xargs -0 --no-run-if-empty nixfmt
          '';
        }
      );

      packages = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          nuvim = pkgs.rustPlatform.buildRustPackage {
            pname = "nuvim";
            version = "0.1.0";
            src = self;
            cargoHash = "sha256-X06BZn3nG4GA5fV46hARcoNfMSC/yxMztYG0j3D0ZnQ=";
            nativeCheckInputs = [
              pkgs.neovim
              pkgs.nushell
              pkgs.rustfmt
            ];
            cargoBuildFlags = [ "--workspace" ];
            cargoTestFlags = [ "--workspace" ];
            postCheck = ''
              cargo run -p nuvim-codegen -- --check
            '';
            meta = {
              description = "Treat Neovim as a Nushell structured-data source and sink over RPC";
              homepage = "https://github.com/roshbhatia/nu_plugin_nvim";
              license = pkgs.lib.licenses.mit;
              mainProgram = "nu_plugin_nuvim";
              platforms = pkgs.lib.platforms.unix;
            };
          };
          nuPlugin =
            pkgs.runCommand "nu-plugin-nuvim-0.1.0"
              {
                meta.mainProgram = "nu_plugin_nuvim";
              }
              ''
                mkdir -p "$out/bin"
                ln -s ${nuvim}/bin/nu_plugin_nuvim "$out/bin/nu_plugin_nuvim"
              '';
        in
        {
          inherit nuvim;
          nu-plugin = nuPlugin;
          default = nuvim;
        }
      );

      apps = eachSystem (system: {
        default = {
          type = "app";
          program = "${nixpkgs.lib.getExe self.packages.${system}.default}";
        };
      });

      checks = eachSystem (system: {
        default = self.packages.${system}.default;
      });

      devShells = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShellNoCC {
            packages = [
              pkgs.cargo
              pkgs.clippy
              pkgs.charm-freeze
              pkgs.neovim
              pkgs.nixfmt
              pkgs.nushell
              pkgs.rust-analyzer
              pkgs.rustc
              pkgs.rustfmt
              pkgs.vhs
            ];
          };
        }
      );

      overlays.default = final: _previous: {
        nuvim = self.packages.${final.stdenv.hostPlatform.system}.default;
      };
    };
}
