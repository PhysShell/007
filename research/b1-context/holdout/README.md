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
