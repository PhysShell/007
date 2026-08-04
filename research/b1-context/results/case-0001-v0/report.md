# B1 development result — case-0001 (v0)

> Golden **development** fixture. One fixture can show the pipeline runs and detects known losses; it cannot prove the pipeline works on new cases.

## Verdict

| field | value |
|---|---|
| development_result | **PASS** |
| generalization | NOT_EVALUATED |
| source_set_complete | false |
| holdout_evaluated | false |
| authoritative_for_a_series | false |

## What worked

- All 6 fixture input blobs verified by digest and size (fail-closed check).
- Both extractors ran deterministically over the local CAS and produced 195 derived transcript records (1561141 bytes), user-visible only, no chain-of-thought.
- The compiled projection (arm C) carried **100.0%** of required observations with **100.0%** question support coverage, at **0.9%** of the full-derived byte size (reduction ratio 0.0085).
- Projection made **0** unsupported claims, **0** contradictions, **0** supersession errors; provenance coverage 100.0%.

## What did not work / limits of this run

- Arm A (full derived transcripts) carries the raw bytes but **zero** typed, provenance-bound observations (required-observation recall 6.2%): a dump does not answer typed questions without the compilation step. Its raw material directly evidences only 18.8% of required observations (evidence_source_presence).
- This fixture is a development fixture: its questions were shaped on its own material, so arm C's high recall debugs the representation and does **not** demonstrate generalization.
- A ChatGPT account export is still pending; the source set remains INTERIM_SOURCE_SET_CAPTURED (not complete).

## Where the negative control lost state

The sealed negative-control reconstruction (arm B) does not merely omit — it **contradicts** the captured topology on **2** required observation(s):
- `obs-bd-one-conversation`
- `obs-session-e-separate`

Confirmation is digest-bound, not hardcoded:
- `obs-nc-claim-bd-separate`: asserted_by_negative_control=true — manifest.known_divergence + NC does not reference corrective capture capture-main-dialog native id 6a6b6230-cb8c-83eb-ad3d-456c0d0c8ce3
- `obs-nc-claim-no-session-e`: asserted_by_negative_control=true — manifest.known_divergence + NC does not reference corrective capture capture-compaction native id 6a6f73b9-d1b0-83eb-a6cb-99064d7cc6cf

## Context reduction

| arm | input bytes | records | required recall | reduction vs full |
|---|---|---|---|---|
| full_derived | 1561141 | 195 | 6.2% | 1.0000 |
| negative_control | 39496 | 48 | 0.0% | 0.0253 |
| projection | 13293 | 16 | 100.0% | 0.0085 |

## What still cannot be claimed

- B1 is **not** proven; context preservation is **not** solved.
- The schema is **not** universal and this is **not** production ready.
- Nothing here **generalizes**; the source set is **not** SOURCE_SET_COMPLETE.
- B1 remains read-only and non-authoritative over the A-series.

## Reproduce

```
python3 research/b1-context/tools/run_case.py --fixture case-0001 --data-root "$HOME/.local/share/o7-research" --out /tmp/o7-b1-case-0001
```
