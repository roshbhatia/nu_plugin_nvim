{
  description = "Structured data bridge between Nushell and Neovim";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      ...
    }:
    let
      supportedSystems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      eachSystem = nixpkgs.lib.genAttrs supportedSystems;
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
          runtimeSource = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.lock
              ./Cargo.toml
              ./crates
            ];
          };
          common = {
            version = "0.1.0";
            src = runtimeSource;
            cargoHash = "sha256-ul7ZtwnG50W3C0++MOgA/Rxp2Pl4cagoMIx1YDpYszM=";
          };
          runtime = pkgs.rustPlatform.buildRustPackage (
            common
            // {
              pname = "nu-plugin-nuvim";
              cargoBuildFlags = [
                "-p"
                "nu-plugin-nuvim"
                "--bin"
                "nu_plugin_nuvim"
              ];
              doCheck = false;
              meta = {
                description = "Control Neovim as native Nushell data over MessagePack-RPC";
                homepage = "https://github.com/roshbhatia/nu_plugin_nvim";
                license = pkgs.lib.licenses.mit;
                mainProgram = "nu_plugin_nuvim";
                platforms = pkgs.lib.platforms.unix;
              };
            }
          );
          codegen = pkgs.rustPlatform.buildRustPackage (
            common
            // {
              pname = "nuvim-codegen";
              cargoBuildFlags = [
                "-p"
                "nuvim-codegen"
                "--bin"
                "nuvim-codegen"
              ];
              doCheck = false;
              meta = {
                description = "Generate Nuvim Rust RPC methods from Neovim API metadata";
                homepage = "https://github.com/roshbhatia/nu_plugin_nvim";
                license = pkgs.lib.licenses.mit;
                mainProgram = "nuvim-codegen";
                platforms = pkgs.lib.platforms.unix;
              };
            }
          );
        in
        {
          inherit codegen runtime;
          nu-plugin = runtime;
          default = runtime;
        }
      );

      checks = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          testedRuntime = pkgs.rustPlatform.buildRustPackage {
            pname = "nu-plugin-nuvim-checks";
            version = "0.1.0";
            src = self;
            cargoHash = "sha256-ul7ZtwnG50W3C0++MOgA/Rxp2Pl4cagoMIx1YDpYszM=";
            cargoBuildFlags = [
              "-p"
              "nu-plugin-nuvim"
              "--bin"
              "nu_plugin_nuvim"
            ];
            cargoTestFlags = [ "--workspace" ];
            nativeCheckInputs = [
              pkgs.expect
              pkgs.neovim
              pkgs.nushell
              pkgs.ripgrep
              pkgs.rustfmt
            ];
            postCheck = ''
              cargo run -p nuvim-codegen -- --check
              export NUVIM_TEST_PLUGIN
              NUVIM_TEST_PLUGIN="$(${pkgs.findutils}/bin/find target -type f -name nu_plugin_nuvim -perm -111 | ${pkgs.coreutils}/bin/head -n 1)"
              ./hack/test-agent-control.sh
              ./hack/test-session-discovery.sh
              ./hack/check-reverse-bridge.sh
            '';
          };
        in
        {
          runtime = testedRuntime;
          codegen = self.packages.${system}.codegen;
          media = pkgs.runCommand "nuvim-media-check" { nativeBuildInputs = [ pkgs.imagemagick ]; } ''
            cp -R ${self} source
            chmod -R u+w source
            cd source
            ./hack/check-media.sh
            touch "$out"
          '';
          repository =
            pkgs.runCommand "nuvim-repository-check"
              {
                nativeBuildInputs = [
                  pkgs.actionlint
                  pkgs.shellcheck
                  pkgs.shfmt
                ];
              }
              ''
                cp -R ${self} source
                chmod -R u+w source
                cd source
                actionlint .github/workflows/*.yml
                shellcheck hack/*.sh
                shfmt -d -i 2 -ci -sr -s hack/*.sh
                touch "$out"
              '';
        }
      );

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
              pkgs.actionlint
              pkgs.charm-freeze
              pkgs.expect
              pkgs.imagemagick
              pkgs.jq
              pkgs.neovim
              pkgs.nixfmt
              pkgs.nushell
              pkgs.ripgrep
              pkgs.rust-analyzer
              pkgs.rustc
              pkgs.rustfmt
              pkgs.shellcheck
              pkgs.shfmt
              pkgs.vhs
            ];
          };
        }
      );

      overlays.default = final: _previous: {
        nuvim = self.packages.${final.stdenv.hostPlatform.system}.runtime;
      };
    };
}
