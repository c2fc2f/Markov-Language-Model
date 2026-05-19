{
  version,
  lib,
  installShellFiles,
  rustPlatform,
  buildFeatures ? [ ],
}:

rustPlatform.buildRustPackage {
  pname = "mlm";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../src
      ../Cargo.lock
      ../Cargo.toml
    ];
  };

  inherit buildFeatures;
  inherit version;

  # inject version from nix into the build
  env.NIX_RELEASE_VERSION = version;

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    installShellFiles

    rustPlatform.bindgenHook
  ];

  buildInputs = [ ];

  meta = with lib; {
    description = "Generates random text based on the statistical properties of a given source text. It implements an n-gram Markov Language Model using n-uplets to predict word sequences";
    mainProgram = "mlm";
    homepage = "https://github.com/c2fc2f/Markov-Language-Model";
    license = licenses.mit;
    maintainers = [ maintainers.c2fc2f ];
  };
}
