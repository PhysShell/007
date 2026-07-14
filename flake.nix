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

    # RTK — pinned to an EXACT commit (never a branch/tag), source-only so
    # `packages.rtk-pinned` builds it from source instead of pulling a mutable
    # release binary. Interop Benchmark v2 RTK↔qodec comparison substrate.
    rtk-src = {
      url = "github:rtk-ai/rtk/5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay, codex-cli, rtk-src }:
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

        # ---- qodec: separate crate from ./qodec with its own Cargo identity ---- #
        qodecArgs = {
          src = craneLib.cleanCargoSource ./qodec;
          strictDeps = true;
          # onig (tokenizers `onig` feature) uses bindgen -> needs libclang;
          # ureq v2 uses rustls (ring) -> C compiler from stdenv, no openssl.
          nativeBuildInputs = [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];
          buildInputs = [ ];
        };
        qodecDeps = craneLib.buildDepsOnly (qodecArgs // { pname = "qodec-deps"; });
        qodec = craneLib.buildPackage (qodecArgs // {
          cargoArtifacts = qodecDeps;
          pname = "qodec";
          doCheck = false;
          meta = {
            description = "Q's codec lab — qodec (fold-grep-guarded is the VG policy)";
            mainProgram = "qodec";
          };
        });

        # ---- rtk-pinned: built from the pinned source, never a release binary --
        # RTK declares rust-version = "1.91"; nixpkgs-25.05's default rustPlatform
        # is Rust 1.86 and would fail Cargo's minimum-version check. Build it with
        # the rust-overlay stable toolchain (same one o7/qodec use) via
        # makeRustPlatform. `src = rtk-src` is UNFILTERED on purpose: build.rs
        # reads src/filters/*.toml, so a cargo-source clean would break it. RTK's
        # own complete Cargo.lock (203 packages) is vendored offline — no network,
        # no cargoHash guessing, no mutable release binary. ----
        rtkPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        rtk-pinned = rtkPlatform.buildRustPackage {
          pname = "rtk-pinned";
          version = "0.42.4";
          src = rtk-src;
          cargoLock.lockFile = "${rtk-src}/Cargo.lock";
          doCheck = false;
          meta = {
            description = "RTK reducer, pinned @ 5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2";
            mainProgram = "rtk";
          };
        };

        # ---- python for the model-free contract / smoke checks ---------------- #
        pyEnv = pkgs.python3.withPackages (ps: [ ps.pyyaml ]);

        # A store copy of just the files the contract tests need to read.
        v2Root = pkgs.runCommand "qodec-v2-root" { } ''
          mkdir -p $out/qodec/evals/interop $out/.github/workflows
          cp -r ${./qodec/evals/interop/v2} $out/qodec/evals/interop/v2
          cp ${./flake.nix} $out/flake.nix
          cp ${./flake.lock} $out/flake.lock
          cp ${./.github/workflows/qodec-v2.yml} $out/.github/workflows/qodec-v2.yml
          cp ${./.gitignore} $out/.gitignore
        '';

        # Reproducibility identity exported to the smoke runner. Everything here
        # is purely derivable from the pinned flake — no impure `nix --version`.
        identityExports = ''
          export LC_ALL=C.UTF-8
          export LANG=C.UTF-8
          export TZ=UTC
          export HOME=$(mktemp -d)
          export NIX_SYSTEM=${system}
          export NIX_VERSION=${pkgs.nix.version}
          export NIXPKGS_REV=${nixpkgs.rev or "unknown"}
          export REPO_COMMIT_SHA=${self.rev or self.dirtyRev or "uncommitted"}
          export FLAKE_LOCK_SHA256=${builtins.hashFile "sha256" ./flake.lock}
          export RUST_TOOLCHAIN_IDENTITY=${builtins.hashFile "sha256" ./rust-toolchain.toml}
          export RTK_SOURCE_SHA=5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2
        '';

        runContractTests = pkgs.writeShellScript "qodec-v2-contract-test" ''
          set -euo pipefail
          root=$(mktemp -d)
          cp -r ${v2Root}/. "$root"
          chmod -R u+w "$root"
          export V2_REPO_ROOT="$root"
          export PYTHONDONTWRITEBYTECODE=1
          cd "$root/qodec/evals/interop/v2"
          exec ${pyEnv}/bin/python -m unittest discover -s tests -v
        '';

        runSmoke = pkgs.writeShellScript "qodec-rtk-smoke" ''
          set -euo pipefail
          ${identityExports}
          out=''${SMOKE_OUT:-$(mktemp -d)/smoke}
          export PYTHONDONTWRITEBYTECODE=1
          exec ${pyEnv}/bin/python ${./qodec/evals/interop/v2/smoke}/run_smoke.py \
            --qodec ${qodec}/bin/qodec \
            --rtk ${rtk-pinned}/bin/rtk \
            --meter o200k \
            --rtk-source-sha 5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2 \
            --out "$out"
        '';

        # ---- Scope N0 corpus compiler (compiler-only; zero benchmark cases) --- #
        corpusRoot = pkgs.runCommand "qodec-v2-corpus-root" { } ''
          mkdir -p $out/qodec/evals/interop/v2
          cp -r ${./qodec/evals/interop/v2/corpus} $out/qodec/evals/interop/v2/corpus
          cp ${./flake.lock} $out/flake.lock
        '';

        # Prelude that stages a writable corpus copy and exports capture identity
        # + the pinned RTK binary. Reproducibility identity is derived purely.
        corpusPrelude = ''
          set -euo pipefail
          ${identityExports}
          export RTK_BIN=${rtk-pinned}/bin/rtk
          export PYTHONDONTWRITEBYTECODE=1
          root=$(mktemp -d); cp -r ${corpusRoot}/. "$root"; chmod -R u+w "$root"
          cd "$root/qodec/evals/interop/v2/corpus"
          export PYTHONPATH="$PWD/tools:$PWD/tests"
        '';

        runCorpusCmd = name: sub: pkgs.writeShellScript name ''
          ${corpusPrelude}
          exec ${pyEnv}/bin/python tools/corpus_tool.py ${sub}
        '';

        corpusValidateApp = runCorpusCmd "qodec-v2-corpus-validate" "validate";
        corpusRegenApp = runCorpusCmd "qodec-v2-demo-regenerate" "regenerate --case deterministic-log-demo";
        corpusVerifyApp = runCorpusCmd "qodec-v2-demo-verify" "verify";
        corpusListApp = runCorpusCmd "qodec-v2-corpus-list" "list";
      in
      {
        packages = {
          default = o7;
          o7 = o7;
          qodec = qodec;
          rtk-pinned = rtk-pinned;
        };

        apps = {
          default = flake-utils.lib.mkApp { drv = o7; };
          qodec-v2-contract-test = {
            type = "app";
            program = "${runContractTests}";
          };
          qodec-rtk-smoke = {
            type = "app";
            program = "${runSmoke}";
          };
          qodec-v2-corpus-validate = { type = "app"; program = "${corpusValidateApp}"; };
          qodec-v2-demo-regenerate = { type = "app"; program = "${corpusRegenApp}"; };
          qodec-v2-demo-verify = { type = "app"; program = "${corpusVerifyApp}"; };
          qodec-v2-corpus-list = { type = "app"; program = "${corpusListApp}"; };
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

          # Interop Benchmark v2 bench shell: pinned tools, no mutable PATH RTK.
          qodec-bench = pkgs.mkShell {
            packages = [
              qodec
              rtk-pinned
              pyEnv
            ] ++ (with pkgs; [
              git
              ripgrep
              gnugrep
              jq
              hyperfine
              actionlint
            ]);
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

          # ---- Interop Benchmark v2 substrate checks (no model/tokenizer net) -- #
          qodec-build = qodec;
          rtk-pinned-build = rtk-pinned;

          qodec-v2-contract = pkgs.runCommand "check-qodec-v2-contract"
            { nativeBuildInputs = [ pyEnv ]; } ''
            ${runContractTests}
            touch $out
          '';

          qodec-rtk-smoke = pkgs.runCommand "check-qodec-rtk-smoke"
            { nativeBuildInputs = [ pyEnv qodec rtk-pinned ]; } ''
            set -euo pipefail
            ${identityExports}
            export SMOKE_OUT=$out/smoke
            mkdir -p $out
            # 1) real-RTK smoke: runs a real `rtk pipe` subcommand per fixture.
            ${pyEnv}/bin/python ${./qodec/evals/interop/v2/smoke}/run_smoke.py \
              --qodec ${qodec}/bin/qodec --rtk ${rtk-pinned}/bin/rtk \
              --meter o200k --out "$SMOKE_OUT"
            # 2) real RTK integration unittests (execute the pinned RTK binary).
            root=$(mktemp -d); cp -r ${v2Root}/. "$root"; chmod -R u+w "$root"
            export V2_REPO_ROOT="$root" QODEC_BIN=${qodec}/bin/qodec RTK_BIN=${rtk-pinned}/bin/rtk
            # test_rtk_comparison.py lives under tests/ — run from that directory
            # so the module resolves (a bare module name from v2/ would not).
            cd "$root/qodec/evals/interop/v2/tests"
            ${pyEnv}/bin/python -m unittest test_rtk_comparison.TestRealRtkIntegration -v
          '';

          github-actions-lint = pkgs.runCommand "check-github-actions-lint"
            { nativeBuildInputs = [ pkgs.actionlint ]; } ''
            actionlint -color ${./.github/workflows}/qodec-v2.yml ${./.github/workflows}/qodec-v2-corpus.yml
            touch $out
          '';

          # ---- Scope N0 corpus compiler checks (model-free) ------------------ #
          qodec-v2-corpus-schemas = pkgs.runCommand "check-qodec-v2-corpus-schemas"
            { nativeBuildInputs = [ pyEnv rtk-pinned ]; } ''
            ${corpusPrelude}
            ${pyEnv}/bin/python -m unittest test_schemas -v
            touch $out
          '';

          qodec-v2-corpus-unit = pkgs.runCommand "check-qodec-v2-corpus-unit"
            { nativeBuildInputs = [ pyEnv rtk-pinned ]; } ''
            ${corpusPrelude}
            # unit surfaces that need no tool execution (RTK present but unused here)
            ${pyEnv}/bin/python -m unittest test_manifest test_snapshots test_receipts test_security -v
            touch $out
          '';

          # capture twice in independent temp dirs; compare raw + rtk snapshots and
          # semantic receipt fields (capture_timestamp/wall_time_s ignored).
          qodec-v2-demo-reproducible = pkgs.runCommand "check-qodec-v2-demo-reproducible"
            { nativeBuildInputs = [ pyEnv rtk-pinned ]; } ''
            ${corpusPrelude}
            ${pyEnv}/bin/python tools/check_reproducible.py deterministic-log-demo
            touch $out
          '';

          qodec-v2-demo-snapshots = pkgs.runCommand "check-qodec-v2-demo-snapshots"
            { nativeBuildInputs = [ pyEnv rtk-pinned ]; } ''
            ${corpusPrelude}
            # committed-bundle integrity + schemas, no tool execution
            ${pyEnv}/bin/python tools/corpus_tool.py verify
            touch $out
          '';

          qodec-v2-no-benchmark-data = pkgs.runCommand "check-qodec-v2-no-benchmark-data"
            { nativeBuildInputs = [ pyEnv ]; } ''
            ${corpusPrelude}
            ${pyEnv}/bin/python tools/check_no_benchmark_data.py
            touch $out
          '';
        };
      });
}
