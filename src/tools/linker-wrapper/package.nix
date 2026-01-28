{ craneLib, commonArgs, ... }:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "linker-wrapper";
    version = "0.1.0";
    cargoExtraArgs = "-p linker-wrapper";
    doCheck = false;
  }
)
