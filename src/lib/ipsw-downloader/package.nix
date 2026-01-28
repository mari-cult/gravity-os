{
  craneLib,
  commonArgs,
  pkgs,
  ...
}:

craneLib.buildPackage (
  commonArgs
  // {
    pname = "ipsw-downloader";
    version = "0.1.0";
    cargoExtraArgs = "-p ipsw-downloader";

    nativeBuildInputs = (commonArgs.nativeBuildInputs or [ ]) ++ [
      pkgs.pkg-config
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
