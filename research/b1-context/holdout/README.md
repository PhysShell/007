# Holdout cases

Empty by design, until the state-observables schema v0 is frozen.

Rules for anything that lands here:

1. The source conversations must NOT have been used while designing the
   schema or its questions (case-0001 is therefore permanently excluded).
2. Future questions are fixed and digest-bound BEFORE any compaction or
   projection runs.
3. Part of the history may be deliberately hidden from the system under
   test.
4. Only holdout results support generalization claims. A fixture answering
   questions written against itself is a well-marketed cheat sheet, not
   memory.

---

## Scope of the H1/H2/H3 batch (added with the freeze)

H1/H2/H3 are a **synthetic producer-holdout**, not the real-source generalization
holdout above. They are authored fixtures whose purpose is to test whether the
**frozen, CI-qualified producer** (Qodec `project` QA `6a5d4030` + adapter
`f8fa87e3`, neither tunable to these cases) generalizes across deliberately
different case *shapes* (relation-light / relation-heavy / budget-pressure).

Because the fixtures are authored (not captured conversations whose questions were
fixed before the schema existed), they **do NOT satisfy rule 1** above and therefore
**cannot support a generalization claim** on their own (`generalization_claim_allowed:
false` stands). What they *can* establish is a producer-robustness signal: does the
frozen `qodec project` still produce valid repaired projections, or does it break /
reveal a missing capability (e.g. budget-aware eviction), on shapes it was not built
against. A real-source holdout per rules 1–4 remains the only path to a generalization
claim.
