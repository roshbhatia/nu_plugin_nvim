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
            cargoHash = "sha256-GKF8xyR0TPMH1T6UFr7PDScnJHTI0I/NwACzTxIGKRI=";
            NUVIM_NU_BIN = nixpkgs.lib.getExe pkgs.nushell;
            nativeCheckInputs = [
              pkgs.neovim
              pkgs.nushell
            ];
            cargoBuildFlags = [ "--workspace" ];
            cargoTestFlags = [ "--workspace" ];
            postInstall = ''
              mkdir -p "$out/share/nvim/site/lua"
              cp lua/nu.lua "$out/share/nvim/site/lua/nu.lua"
              nuvim_library="$(find target -type f \( -name libnvim_nu.dylib -o -name libnvim_nu.so \) -print -quit)"
              test -n "$nuvim_library"
              cp "$nuvim_library" "$out/share/nvim/site/lua/nvim_nu.so"
            '';
            meta = {
              description = "Treat Neovim as a Nushell structured-data source and sink";
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
          nvimPlugin = pkgs.runCommand "nvim-nu-0.1.0" { } ''
            mkdir -p "$out/lua"
            ln -s ${nuvim}/share/nvim/site/lua/nu.lua "$out/lua/nu.lua"
            ln -s ${nuvim}/share/nvim/site/lua/nvim_nu.so "$out/lua/nvim_nu.so"
          '';
        in
        {
          inherit nuvim;
          nu-plugin = nuPlugin;
          nvim-plugin = nvimPlugin;
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
