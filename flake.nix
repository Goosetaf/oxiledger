{
  description = "Folks - Dioxus Project";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        libraries = with pkgs; [
          openssl_3
        ];

        wasmBindgenCli = pkgs.stdenv.mkDerivation rec {
          pname = "wasm-bindgen-cli";
          version = "0.2.114";

          src = pkgs.fetchurl {
            url = "https://github.com/rustwasm/wasm-bindgen/releases/download/${version}/wasm-bindgen-${version}-x86_64-unknown-linux-musl.tar.gz";
            sha256 = "sha256-ziG+ACvbwi7eKlzyUMuAHSEGlW/DxSnHXZRScp2zSQw=";
          };

          sourceRoot = "wasm-bindgen-${version}-x86_64-unknown-linux-musl";

          installPhase = ''
            mkdir -p $out/bin
            cp wasm-bindgen wasm-bindgen-test-runner wasm2es6js $out/bin/
            chmod +x $out/bin/*
          '';
        };

        packages = with pkgs; [
          rustToolchain
          pkg-config
          dioxus-cli
          curl
          wget
          wasmBindgenCli
          tailwindcss
          sqlx-cli
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = packages ++ libraries;
        };
      }
    );
}
