{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
  };

  outputs =
    {
      nixpkgs,
      ...
    }:
    let
      eachSystem = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "riscv64-linux"
      ];
    in
    {
      devShells = eachSystem (
        system: with nixpkgs.legacyPackages.${system}; {
          default = buildFHSEnv rec {
            name = "dev";
            nativeBuildInputs = with buildPackages; [
              dotslash
              rust-cbindgen
              rust-bindgen
              rustup
              nixfmt
              nil
              ty
              python3
              llvmPackages.lld
              llvmPackages.clangUseLLVM
            ];

            buildInputs = [
            ];

            LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
          };
        }
      );
    };
}
