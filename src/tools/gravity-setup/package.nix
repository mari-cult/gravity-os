{
  craneLib,
  commonArgs,
  pkgs,
  ...
}:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "gravity-setup";
    version = "0.1.0";

    cargoExtraArgs = "-p gravity-setup";

    nativeBuildInputs = (commonArgs.nativeBuildInputs or [ ]) ++ [
      pkgs.pkg-config
      pkgs.rust-bindgen
    ];

    buildInputs =
      (commonArgs.buildInputs or [ ])
      ++ [
        pkgs.openssl
      ]
      ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
        pkgs.darwin.apple_sdk.frameworks.Security
        pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
      ];

    doCheck = false;
  }
)
