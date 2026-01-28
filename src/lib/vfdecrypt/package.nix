{ craneLib, commonArgs, ... }:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "vfdecrypt";
    version = "0.1.0";
    cargoExtraArgs = "-p vfdecrypt";
    doCheck = false;
  }
)
