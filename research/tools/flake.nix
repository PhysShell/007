{
  description = "o7-research tooling: restic (-> Cloudflare R2), rclone, awscli, age, and the o7-cas / o7-secret / o7-restic helpers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";   # matches the repo's root flake pin
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        # the committed scripts, wrapped as proper bins (no dependence on ~/.local/bin)
        o7-cas    = pkgs.writeShellScriptBin "o7-cas"    (builtins.readFile ./o7-cas);
        o7-secret = pkgs.writeShellScriptBin "o7-secret" (builtins.readFile ./o7-secret);
        o7-restic = pkgs.writeShellScriptBin "o7-restic" (builtins.readFile ./o7-restic);
      in {
        packages = { inherit o7-cas o7-secret o7-restic; };
        # `nix develop ./research/tools`
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            restic rclone awscli2 age coreutils gawk findutils
            o7-cas o7-secret o7-restic
          ];
          shellHook = ''
            envf="$HOME/.config/o7-research/restic.env"
            if [ -f "$envf" ]; then set -a; . "$envf"; set +a; fi
            echo "[o7-research] tools: o7-cas o7-secret o7-restic | restic rclone aws age"
            echo "[o7-research] secrets: R2 keys live ENCRYPTED in the o7-secret vault,"
            echo "                       injected at call time by o7-restic (never plaintext in env/history/chat)."
            echo "[o7-research] roots (KEEP, ideally in a password manager): ~/.config/o7-research/{age-identity.txt,restic-password}"
            echo "[o7-research] set a secret (hidden input): o7-secret set NAME"
            echo "[o7-research] backup: o7-restic backup ~/.local/share/o7-research --tag o7-cas   |   verify: o7-restic check --read-data"
          '';
        };
      });
}
