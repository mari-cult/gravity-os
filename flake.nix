{
  description = "Gravity OS development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "riscv64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          useLld =
            stdenv:
            stdenv.override (prev: {
              allowedRequisites = null;
              cc = prev.cc.override {
                bintools = prev.cc.bintools.override {
                  extraBuildCommands = ''
                    ln -fs ${pkgs.buildPackages.lld}/bin/* "$out/bin"
                  '';
                };
              };
            });

          nativeCraneLib = crane.mkLib pkgs;

          pkgsCross = import nixpkgs {
            inherit system;
            crossSystem = {
              config = "aarch64-unknown-none-elf";
              useLLVM = true;
              linker = "lld";
              libc = null;
              rust.rustcTarget = "aarch64-unknown-none-softfloat";
            };
          };
          crossCraneLib = crane.mkLib pkgsCross;

          commonArgs = {
            src = self;
            strictDeps = true;
            doCheck = false;
          };

          callPackage = pkgs.lib.callPackageWith (
            pkgs
            // {
              commonArgs = commonArgs;
            }
          );
        in
        {
          kernel = callPackage ./src/kernel/package.nix {
            craneLib = crossCraneLib;
            stdenv = useLld pkgsCross.stdenv;
          };

          dyld = callPackage ./src/lib/dyld/package.nix {
            craneLib = crossCraneLib;
            stdenv = useLld pkgsCross.stdenv;
          };

          ipsw = callPackage ./src/tools/ipsw/package.nix {
            craneLib = nativeCraneLib;
          };

          gravity-setup = callPackage ./src/tools/gravity-setup/package.nix {
            craneLib = nativeCraneLib;
          };

          linker-wrapper = callPackage ./src/tools/linker-wrapper/package.nix {
            craneLib = nativeCraneLib;
          };

          vfdecrypt = callPackage ./src/lib/vfdecrypt/package.nix {
            craneLib = nativeCraneLib;
          };

          ipsw-downloader = callPackage ./src/lib/ipsw-downloader/package.nix {
            craneLib = nativeCraneLib;
          };

          hfsplus = callPackage ./src/lib/hfsplus/package.nix {
            craneLib = nativeCraneLib;
          };

          apple-dmg = callPackage ./src/lib/apple-dmg/package.nix {
            craneLib = nativeCraneLib;
          };

          default = self.packages.${system}.kernel;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        with pkgs;
        {
          default = mkShell {
            nativeBuildInputs = [
              rustc
              cargo
              cargo-edit
              rustfmt
              rust-analyzer
              clippy
              qemu
              mtools
              pkg-config
              nixfmt
              pkg-config
              nix-output-monitor
              rust-bindgen
              watchman
              jujutsu
              gitMinimal
            ]
            ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.SystemConfiguration
            ];

            buildInputs = [
              aws-lc
            ];

            OPENSSL_DIR = aws-lc.dev;
            OPENSSL_LIB_DIR = "${aws-lc}/lib";
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          useLld =
            stdenv:
            stdenv.override (prev: {
              allowedRequisites = null;
              cc = prev.cc.override {
                bintools = prev.cc.bintools.override {
                  extraBuildCommands = ''
                    ln -fs ${pkgs.buildPackages.lld}/bin/* "$out/bin"
                  '';
                };
              };
            });
          nativeCraneLib = crane.mkLib pkgs;
          pkgsCross = import nixpkgs {
            inherit system;
            crossSystem = {
              config = "aarch64-unknown-none-elf";
              useLLVM = true;
              linker = "lld";
              libc = null;
              rust.rustcTarget = "aarch64-unknown-none-softfloat";
            };
          };
          crossCraneLib = crane.mkLib pkgsCross;
          commonArgs = {
            src = self;
            strictDeps = true;
            doCheck = false;
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.rust-bindgen
              pkgs.buildPackages.clang
            ];
            buildInputs = [
              pkgs.aws-lc
            ];
            OPENSSL_DIR = pkgs.aws-lc.dev;
            OPENSSL_LIB_DIR = "${pkgs.aws-lc}/lib";
          };

          nativeArtifacts = nativeCraneLib.buildDepsOnly (
            commonArgs
            // {
              cargoExtraArgs = "--workspace --exclude kernel --exclude dyld";
            }
          );

          crossArtifacts = crossCraneLib.buildDepsOnly (
            commonArgs
            // {
              cargoExtraArgs = "--package kernel --package dyld";
            }
          );
        in
        {
          clippy = nativeCraneLib.cargoClippy (
            commonArgs
            // {
              cargoArtifacts = nativeArtifacts;
              cargoClippyExtraArgs = "--workspace --exclude kernel --exclude dyld --all-targets -- -D warnings";
            }
          );

          clippy-kernel = crossCraneLib.cargoClippy (
            commonArgs
            // {
              cargoArtifacts = crossArtifacts;
              cargoClippyExtraArgs = "--package kernel --package dyld --all-targets -- -D warnings";
            }
          );
        }
      );
    };
}
