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

## Secrets: reference, not value (`o7-secret` + `o7-restic`)

`o7-secret` is a tiny `age`-encrypted store demonstrating the "give a
reference, not the value" discipline (the local twin of `bw get` / `op run`
/ `bws run`):

```
o7-secret init                     # create the age identity (local unlock key)
o7-secret set  NAME                # value read from your TTY (hidden), encrypted at rest
o7-secret run  NAME[,NAME2] -- cmd # inject as env VARS into a subprocess — never printed
o7-secret ls                       # names only, never values
```

`o7-restic` is `restic` with the R2 access keys pulled from that vault at
call time, so **no secret ever lives as plaintext** in `restic.env`, the
shell, history, or a chat:

```
o7-restic backup ~/.local/share/o7-research --tag o7-cas
o7-restic snapshots ; o7-restic check --read-data ; o7-restic restore <snap> --target <dir>
```

**Two roots of trust** (neither is committed, neither is in the cloud
backup — put both in your password manager): `~/.config/o7-research/age-identity.txt`
(decrypts the secret vault) and `~/.config/o7-research/restic-password`
(decrypts the R2 backup). The R2 backup holds only *age-encrypted* secrets —
useless without the identity, which lives only on your machine + your PM.
