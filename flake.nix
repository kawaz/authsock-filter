{
  description = "SSH agent proxy with filtering and logging";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      version = "0.1.39";

      # バイナリ配布があるプラットフォーム
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      # プラットフォームごとのアーカイブ情報
      archives = {
        x86_64-linux = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-x86_64-unknown-linux-gnu.tar.gz";
          sha256 = "sha256-TSanwYHaLV5cpfpJq5JilfxMpaVj/QA54NvFFaBkjbs=";
        };
        aarch64-linux = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-aarch64-unknown-linux-gnu.tar.gz";
          sha256 = "sha256-DeQKxpiZGxupAsE+Y0wNBE7RTT4C03SEaucqhz0BL6Q=";
        };
        x86_64-darwin = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-x86_64-apple-darwin.tar.gz";
          sha256 = "sha256-+fbYZMJdnQPmRPNkzgO4e2e94MUkUhBg5viiWY08GD0=";
        };
        aarch64-darwin = {
          url = "https://github.com/kawaz/authsock-filter/releases/download/v${version}/authsock-filter-aarch64-apple-darwin.tar.gz";
          sha256 = "sha256-GmPiKCMz8MRVo9JO8p/Mu/UPQVKVBx2TXcPKAe4TFi0=";
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
              platforms = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
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
