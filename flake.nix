{
  description = "007 (o7) — private agent harness dev environment";

  inputs = {
    # Matches the pin used across the other Rust projects here.
    # crane warns it wants nixpkgs >= 25.11; bump when the store cache is cold.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # codex — OpenAI's Rust CLI, the `--provider codex` backend for `o7 judge`.
    # Deliberately NOT `follows`-ing our nixpkgs: codex is built/tested against its
    # own nixpkgs-unstable pin; forcing it onto 25.05 risks a broken rebuild.
    codex-cli.url = "github:PhysShell/codex-cli-nix";
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, codex-cli }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          # o7 is pure Rust (clap / serde / toml / anyhow) — no native or -sys deps.
          buildInputs = [ ];
          nativeBuildInputs = [ ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        o7 = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "o7";
          doCheck = false;
          meta = {
            description = "007 — private agent harness";
            mainProgram = "o7";
          };
        });
      in
      {
        packages = {
          default = o7;
          o7 = o7;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = o7; };
        };

        devShells = {
          default = pkgs.mkShell {
            packages = (with pkgs; [
              rustToolchain
              cargo-deny
              cargo-audit
              git
              jq
            ]) ++ [
              # Native `bin/codex` from github:PhysShell/codex-cli-nix.
              codex-cli.packages.${system}.default
            ];
            # `claude` is external (npm + Claude Max). `codex` is provided above but
            # still needs `codex login` once (ChatGPT subscription, no API key).
          };
        };

        checks = {
          inherit o7;

          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          fmt = craneLib.cargoFmt {
            src = commonArgs.src;
          };
        };
      });
}
