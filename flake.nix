{
  description = "Generates random text based on the statistical properties of a given source text. It implements an n-gram Markov Language Model using n-uplets to predict word sequences";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
  };

  outputs =
    {
      self,
      nixpkgs,
      systems,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      eachSystem = lib.genAttrs (import systems);

      pkgsFor = eachSystem (
        system:
        import nixpkgs {
          localSystem = system;
        }
      );
    in
    {
      packages = eachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = self.packages.${system}.mlm;

          mlm = pkgs.callPackage ./nix/package.nix {
            version = self.rev or self.dirtyRev or "dirty";
          };
        }
      );

      defaultPackage = eachSystem (system: self.packages.${system}.default);

      devShells = eachSystem (system: {
        default =
          pkgsFor.${system}.mkShell.override
            {
              inherit (self.packages.${system}.default) stdenv;
            }
            {
              env = {
                # Required by rust-analyzer
                RUST_SRC_PATH = "${pkgsFor.${system}.rustPlatform.rustLibSrc}";
              };

              nativeBuildInputs = with pkgsFor.${system}; [
                cargo
                rustc
                rust-analyzer
                rustfmt
                clippy

                rustPlatform.bindgenHook
              ];

              buildInputs = [ ];
            };
      });
    };
}
