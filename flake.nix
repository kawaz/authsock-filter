{
  description = "SSH agent proxy with filtering and logging";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      version = "0.1.38";

      # バイナリ配布があるプラットフォームのみ
      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" ];

      # プラットフォームごとのアーカイブ情報
      archives = {
        x86_64-linux = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-x86_64-unknown-linux-gnu.tar.gz";
          sha256 = "sha256-7HB90YLfOEaWVZfT9AemMo/MjgITVPSTjoCicoc0J/M=";
        };
        x86_64-darwin = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-x86_64-apple-darwin.tar.gz";
          sha256 = "sha256-O7xO18p1TZ0CfbH+SgE2PdezPrR0hWKS9b2ctGG49Kk=";
        };
        aarch64-darwin = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-aarch64-apple-darwin.tar.gz";
          sha256 = "sha256-s/7gXChMweYRzH1aFPaYOEIpQvRBmsPLB5G+wehrZiI=";
        };
      };

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          archive = archives.${system};
        in
        {
          default = pkgs.stdenv.mkDerivation {
            pname = "authsock-filter";
            inherit version;

            src = pkgs.fetchurl {
              url = archive.url;
              sha256 = archive.sha256;
            };

            sourceRoot = ".";

            nativeBuildInputs = [ pkgs.gzip ];

            installPhase = ''
              mkdir -p $out/bin
              cp authsock-filter $out/bin/
              chmod +x $out/bin/authsock-filter
            '';

            meta = with pkgs.lib; {
              description = "SSH agent proxy with filtering and logging";
              homepage = "https://github.com/kawaz/authsock-filter";
              license = licenses.mit;
              maintainers = [ ];
              mainProgram = "authsock-filter";
              platforms = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" ];
            };
          };
        });

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              rust-analyzer
              clippy
              rustfmt
            ];
          };
        });
    };
}
