{
  description = "o7-research: S3/CAS backup tooling — restic (-> Cloudflare R2), rclone, awscli";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";   # matches the repo's root flake pin
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { inherit system; };
      in {
        # `nix develop ./research/tools` — S3/backup tooling for the B1 source-set.
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ restic rclone awscli2 ];
          shellHook = ''
            export PATH="$HOME/.local/bin:$PATH"   # o7-cas helper (also at research/tools/o7-cas)
            envf="$HOME/.config/o7-research/restic.env"
            if [ -f "$envf" ]; then
              set -a; . "$envf"; set +a
              echo "[o7-research] restic env loaded — repo: $RESTIC_REPOSITORY"
            else
              echo "[o7-research] NOTE: $envf not found. Secrets (RESTIC_REPOSITORY, RESTIC_PASSWORD_FILE,"
              echo "               AWS_* for R2) live OUTSIDE git at ~/.config/o7-research/*, mode 0600."
            fi
            echo "[o7-research] tools: restic rclone aws | helper: o7-cas"
            echo "[o7-research] backup:  restic backup ~/.local/share/o7-research --tag o7-cas"
            echo "[o7-research] verify:  restic check --read-data   |   restore: restic restore <snap> --target <dir>"
          '';
        };
      });
}
