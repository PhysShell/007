# Scope N2-D0 — durable evidence rescue and benchmark-input lock

N2-C's real acquisition bytes (37 candidates: 17 primary + 20 alternate) and
N2-A.1's real canary-capture bytes exist only as GitHub Actions workflow-run
artifacts, which expire ~90 days after creation. Neither scope ever wrote
durable copies into the repository. N2-D0 rescues that evidence into a
GitHub Release (`n2d0-durable-evidence-v1`) before it expires, verifies it
byte-for-byte against the already-accepted evidence trail, and locks a
canonical, SHA256-hashed benchmark-input manifest — **without** modifying
N2-C (frozen at `acb57379e2d0b9ed6fe79fd45e7540d7d00d7490`, PR #54) or
executing QODEC/RTK/any model.

See [`n2d0-contract.json`](n2d0-contract.json) for the full, machine-readable
acceptance contract. See [`durable-input-manifest.json`](durable-input-manifest.json)
(written after the real rescue workflow run) for the final, hash-locked
inventory of every rescued case.

## Layout

- `n2d0-contract.json` — the acceptance contract (this scope's spec, made concrete)
- `tools/zip_fetch.py` — real, authenticated same-repo artifact retrieval (runs only in CI)
- `tools/verify_n2c_evidence.py` — cross-verifies rescued N2-C bytes against the accepted artifact index, folded identities, and final selection report
- `tools/verify_n2a_evidence.py` — cross-verifies rescued N2-A.1 bytes against the accepted run identity and capture-a/capture-b agreement
- `tools/build_durable_manifest.py` — assembles the final, canonicalized, SHA256-locked manifest
- `tests/` — unit tests for the above (synthetic fixtures, no network)
- `durable-input-manifest.json` — the final artifact (written after real CI verification + release publication)
- `reports/` — small, committed evidence reports from the real rescue run

## What N2-D0 does not do

Execute QODEC or RTK, compute token counts, instantiate a tokenizer, inspect
comparative arm outputs, call a model, modify candidate selection or
fallback ordering, reacquire upstream source content, or declare/imply a
benchmark winner. See `n2d0-contract.json`'s `scope_boundary_prohibited_actions`.
