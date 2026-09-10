{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs.lib)
        genAttrs
        importTOML
        licenses
        maintainers
        platforms
        sourceByRegex
        ;

      eachSystem =
        f:
        genAttrs [
          "aarch64-linux"
          "x86_64-linux"
        ] (system: f nixpkgs.legacyPackages.${system});
    in
    {
      formatter = eachSystem (pkgs: pkgs.nixfmt);

      packages = eachSystem (
        pkgs:
        let
          src = sourceByRegex self [
            "(src)(/.*)?"
            ''Cargo\.(toml|lock)''
          ];
        in
        {
          default = pkgs.callPackage (
            {
              lib,
              rustPlatform,
              makeWrapper,
              systemd,
            }:
            rustPlatform.buildRustPackage {
              pname = "sis";
              inherit ((importTOML (src + "/Cargo.toml")).package) version;

              inherit src;

              cargoLock = {
                lockFile = src + "/Cargo.lock";
              };

              nativeBuildInputs = [
                makeWrapper
              ];

              postInstall = ''
                # journalctl, coredumpctl, loginctl and friends are spawned as
                # subprocesses; make sure the systemd of this nixpkgs is found.
                wrapProgram $out/bin/sis \
                  --prefix PATH : ${lib.makeBinPath [ systemd ]}
              '';

              meta = {
                description = "k9s for systemd";
                mainProgram = "sis";
                license = licenses.mpl20;
                maintainers = with maintainers; [ kmein ];
                platforms = platforms.linux;
              };
            }
          ) { };
        }
      );

      devShells = eachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
          ];
        };
      });
    };
}
