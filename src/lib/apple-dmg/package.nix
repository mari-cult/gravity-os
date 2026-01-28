{ craneLib, commonArgs, ... }:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "apple-dmg";
    version = "0.1.0";
    cargoExtraArgs = "-p apple-dmg";
    doCheck = false;
  }
)
