# research/tools

Non-authoritative research tooling for the B1 context/memory corpus work
(GitHub issues #91/#94/#95 program). Nothing here is imported by any crate
in the Cargo workspace, and none of it carries production or admission
authority.

## o7-cas

A minimal content-addressed store (sha256) for the B1 source-set: RAW
platform exports and their deterministic derivatives live as read-only
blobs in a **local** CAS outside git; only digests + manifests belong in
this repo. `o7-cas` is the put/get/verify helper for that local store.

```
o7-cas put <file>          # ingest; prints  cas:sha256:<digest>
o7-cas has <digest>        # exit 0 if present
o7-cas path <digest>       # on-disk path
o7-cas get <digest> [dest] # copy out (or cat to stdout)
o7-cas verify <digest>     # OK / CORRUPT / MISSING
o7-cas verify --all        # recompute every object's digest vs its address
o7-cas ls                  # <digest> <bytes>
```

Store root: `$CAS_ROOT` (default `~/.local/share/o7-research/cas`), objects
sharded at `sha256/<aa>/<rest>`, stored read-only (`0444`).

**Guarantee boundary:** CAS proves *"these bytes match this address,"* not
*"these bytes are the object you intended."* A truncated/partial input is
still stored honestly under *its own* (different) address — always compare
the resulting digest against the *expected* one. Raw account exports and
session transcripts are **never** committed here — only their digests and
metadata.

## S3 / encrypted offsite copy (restic -> Cloudflare R2)

`flake.nix` here provides a `nix develop ./research/tools` shell with
`restic`, `rclone`, and `aws` (awscli2). Copy 2 of the source-set is a
**client-side-encrypted** restic repository on Cloudflare R2 (the provider
only ever sees ciphertext).

Secrets never live in git. They sit at `~/.config/o7-research/` (`0600`):
- `restic.env` — `RESTIC_REPOSITORY` (`s3:https://<acct>.r2.cloudflarestorage.com/o7-cas/restic`), `RESTIC_PASSWORD_FILE`, `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` (R2), `AWS_DEFAULT_REGION=auto`, `TMPDIR` (roomy — restic packs must not land on a small tmpfs).
- `restic-password` — the **only** key that decrypts the backup. Keep a copy in a password manager; losing it = the R2 ciphertext is unrecoverable.

Usage inside the dev shell:
```
restic backup ~/.local/share/o7-research --tag o7-cas    # incremental, dedup'd, encrypted
restic snapshots                                         # list
restic check --read-data                                 # full integrity (downloads + verifies)
restic restore <snapshot-id> --target /some/dir          # recover
```
