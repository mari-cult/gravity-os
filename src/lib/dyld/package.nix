{
  craneLib,
  commonArgs,
  stdenv,
  ...
}:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "dyld";
    version = "0.1.0";
    cargoExtraArgs = "-p dyld";
    doCheck = false;

    nativeBuildInputs = (commonArgs.nativeBuildInputs or [ ]) ++ [
      stdenv.cc
    ];

    RUSTFLAGS = "-Clinker=clang -Clink-arg=--ld-path=ld.lld -Clink-arg=-nostdlib -Clink-arg=--target=aarch64-unknown-none-elf";
  }
)
