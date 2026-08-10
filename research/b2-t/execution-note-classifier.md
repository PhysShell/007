# B2-T — execution note: classifier eligibility and packet construction

```text
STATUS:  EXECUTION NOTE — NOT PART OF THE FROZEN PREREGISTRATION
         Operationalises §4 of preregistration.md; adds no norm to §3.
         Written BEFORE any classification output exists.
```

The preregistration was frozen at `C = 153684fd27e7d55992015cc33cb03ec23df03c1d`
by `F = 68c3f1d80eb3daf2b8c3e91fa4ba876956e078d7`. This note does not modify it.
It records two operational decisions that §4 requires but does not spell out, and
it is written down now so that neither is settled after the numbers exist.

## 1. Classifier eligibility

**Decision.** "Participant in the discussion" means an *execution context /
agent instance* that had access to the discussion — not the underlying model.

> A fresh instance of the same base model is eligible as classifier, provided it
> cannot obtain the prior context and receives only the frozen classification
> contract and the blinded corpus packet.

Disqualifying the model weights would require asserting some contamination of a
model by a conversation that is not in those weights. That is not the experiment.

**A classifier is eligible if:**

- it is a fresh execution context;
- it has not received and cannot retrieve the B2-T discussion;
- it receives only (a) the frozen §3 contract and (b) the 41 verbatim items in
  randomised order;
- round labels, reviewer provenance, chronology and the hypotheses are absent;
- no repository, search, or memory retrieval is available to it during the pass.

**"A new chat" is not the criterion; actual isolation is.** An instance in an
environment that injects project history, memory, or repository context, or that
can reach GitHub on its own, does **not** qualify. The classifier must be
effectively stateless with respect to 007.

Using the same base model family does not by itself constitute prior
participation.

**Explicitly ineligible:** the assistant instance that participated in forming
the taxonomy (this one), and any instance carrying this conversation.

## 2. Packet construction

The builder role for the calibration corpus is unrestricted — §2 fixes the
corpus, the unit and N=41, so there is no selection decision left to bias. (This
is *not* true of the §5 corpus, where both discussion participants are barred as
builders because its eligibility rule was written after seeing the material.)

**Source.** `docs/q-deck/a1-authority-contracts.md` §9, extracted mechanically.

**Extraction rule.** A finding is its numbered line plus every following line
that is blank or indented, terminating at the first non-indented non-blank line
(round-level prose), the next numbered item, or the next `###` round heading;
fenced blocks are not treated as terminators. This matters: a naive extractor
absorbed R5.2's closing paragraph into finding #4. Verified: 41 findings,
distributed R1 6 / R2 6 / R3 7 / R4 7 / R5 6 / R5.1 5 / R5.2 4.

**Anonymisation.** Each finding receives an uninformative stable id
`item_001` … `item_041`. The packet carries no round labels, numbers, dates or
ordering signal.

**Shuffle.** `random.Random(20260810).shuffle` over the source order. The seed is
the freeze date — fixed and recorded, not chosen for effect. Recording it is not
about reproducing randomness but about removing any later occasion to guess by
hand which item was which finding.

**Artefacts.**

```text
research/b2-t/packet/instructions.md   §3 verbatim + task + output schema
research/b2-t/packet/items.md          the 41 items
research/b2-t/packet-key.json          UNBLINDING KEY — deliberately NOT in packet/
```

`packet-key.json` carries the `item_id → round/number` mapping, the seed, and two
digests. It must not reach the classifier and is opened only after the
classification output is recorded. Its separate location is deliberate: the key
being one directory away from the packet is the difference between a mistake and
a habit.

### Digests, with their preimages stated

"A digest over the item list" is not an identification — a reader cannot tell
whether the preimage is the file's bytes, the concatenated bodies, a JSON array,
or any of those after newline normalisation. Both are therefore written out:

```text
packet_items_sha256
  6ec7b050e62ca32fb90608f624285565d795cfefa69759dd5233188acd2f37c6
  preimage: UTF-8 bytes of json.dumps(items, ensure_ascii=False,
            sort_keys=True), items being the ordered list of
            {"item_id", "text"} objects item_001..item_041, text the
            verbatim body with no trailing newline.
            NOT the bytes of items.md.

packet_items_md_sha256
  9e367e84b3ea26f093733fa7973b7113fcdbd7996735d8b2fa34aaf17b2f923f
  preimage: raw blob bytes of research/b2-t/packet/items.md as committed,
            no decoding, no newline normalisation.
            Verify: sha256sum research/b2-t/packet/items.md

packet_items_md_git_blob
  91ec34bb8102347f4bc60fef6b8be2083c72635d
  Verify: git hash-object research/b2-t/packet/items.md
```

The first is the content digest and is invariant under changes to the file's
framing — it was computed before the packet title was neutralised and still
matches, which is exactly the property wanted from it. The second and third
identify the artefact as shipped and are checkable with one command each.

Both were recomputed from the committed `items.md` and agree; the 41 items
reparsed out of the file are identical to the ones the builder hashed.

## 3. What the classifier is told

`instructions.md` contains §3 verbatim, the task sentence, the output schema, and
nothing else. No statement of purpose, no mention of what is being measured, no
worked example. Even "we are checking how often identity conflation occurs" would
re-supply the hypothesis that §4 exists to withhold.

**Residual leak 1 — §3's own wording.** §3 is reproduced verbatim as required,
and it contains "The headline rates use primary cause only", which tells the
classifier that rates will be computed. The alternative was to edit the frozen
contract on its way to the classifier, which is worse.

**Residual leak 2 — round references inside the verbatim findings.** Five of the
41 items carry six cross-references the original reviewer wrote into the finding
text itself:

```text
item_016   "…and R5 picks one"
item_034   "R5 fixed enum framing but left…"  /  "the divergence R5 removed"
item_038   "the same choice R1 §11.6 made for the same condition"
item_040   "a leftover from R4, contradicting §3.9 and §5.3"
item_041   "R3 removed the…"
```

They partially reveal chronology — an item citing R5 in the past tense is from
R5.1 or R5.2.

**This was left as-is, deliberately.** §4 requires the findings *verbatim*,
emphasising that even paraphrase can help the hypothesis unnoticed, and requires
"no round labels" — which on the plain reading means no labels *attached to* the
items, not redaction of the source prose. Redacting would break the stronger and
first-stated requirement to satisfy the weaker one. The exposure is bounded:
these references reveal ordering, not the hypothesis, and the classifier produces
no stratification — the internal/external split is computed at unblind from
`packet-key.json`, not by the classifier.

Both leaks are recorded here so they are known properties of the run rather than
later discoveries. Only the packet **title** was neutralised (it named the study,
and was not source text).

## 4. Run discipline

One pass. No consultation during it, no "item_017 looks odd", no second opinion
folded in afterwards. A blinded study conducted as a group exercise is neither.

Output is recorded verbatim before `packet-key.json` is opened.
