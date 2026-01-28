{ craneLib, commonArgs, ... }:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "hfsplus";
    version = "0.1.0";
    cargoExtraArgs = "-p hfsplus";
    doCheck = false;
  }
)
