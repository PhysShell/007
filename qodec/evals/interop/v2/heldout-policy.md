# Sealed held-out contract — Interop Benchmark v2

This document is the binding lifecycle for the v2 sealed held-out set. It is
part of the `interop-benchmark-v2` contract (`frozen-before-data`). It defines
*what may be tracked in Git*, *what must be frozen before a sealed run*, and
*what invalidates a promotion run*.

## Where the sealed material lives

The real sealed bundle is produced in a **later scope** by a local custodian and
stored **outside Git**:

```
qodec/evals/interop/v2/private/heldout-v2.tar.zst
```

`qodec/evals/interop/v2/private/` is git-ignored. Nothing under it is ever
committed. This design scope neither creates the bundle nor looks at any future
sealed evaluation result.

## What Git may track about the sealed set

After the bundle exists, the **only** tracked sealed artifact is
`sealed-manifest.json`, conforming to `schemas/sealed-manifest.schema.json`. It
may contain **only**:

- `contract_version`
- `bundle_sha256`
- `case_count`
- `question_count`
- `coverage_summary_sha256`
- `created_at`
- `custodian`

A tracked sealed manifest **must not** contain: payload, payload path, gold,
question text, case IDs that reveal content, or any tool-output fragment. The
same applies to sealed **cases** and **questions** in any coverage manifest:
sealed cases carry coverage metadata only (family, ecosystem, outcome, size,
hazards, split, origin.kind …) — never `payload`, `payload_path`, or `gold`, and
never question text. `validate_contract.py` enforces this (`sealed-leak`,
`sealed-gold`).

## Freeze list — frozen before any sealed run

The sealed runner receives the bundle **only after every one of these identities
is frozen and recorded**:

1. policy source SHA
2. qodec binary SHA
3. benchmark contract SHA (`benchmark-contract.json` / `coverage-matrix.json`)
4. scorer SHA
5. reader task schema SHA
6. notation brief SHA
7. model identities
8. tokenizer identities
9. runtime contracts
10. generation parameters

## Rules

1. The policy tuner sees **only** `public-development` and `public-validation`.
2. The sealed runner receives the bundle **only after** all identities above are
   frozen.
3. Any change to policy, scorer, or gate **after unseal** voids the promotion
   run.
4. If VG changes after studying sealed failures, that is a **new policy
   version** — not a patch to the accepted one.
5. A sealed set, once revealed, becomes a **public regression set**.
6. Each new promotion attempt requires a **new sealed set**.
7. A set that has been looked at may **never** be called held-out again.

## Promotion lifecycle (summary)

```
freeze identities ──► seal bundle (custodian, off-Git)
       │
       ▼
tuner works on public-development + public-validation only
       │
       ▼
sealed runner unseals AFTER freeze ──► sealed promotion gate scored ONCE
       │
       ├─ pass ─► verdict recorded under this contract_version
       │           sealed set retired to public regression set
       │
       └─ any post-unseal change to policy/scorer/gate ─► run VOID,
                                                          new sealed set required
```

Model answers from old runs are never reused in place of fresh answers. The
sealed promotion gate is scored exactly once per frozen configuration.
