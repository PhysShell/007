#!/usr/bin/env python3
"""L2 Phase B — derive protected identifier spans for each frozen Q1R context.

The protection rule, stated once and applied uniformly:

    Every observation identity exposed by this context schema is
    machine-significant and is preserved uniformly.

That is deliberately a statement about the *schema*, not about the questions.
Spans come from the four structural identity positions —

    selected[].observation_id
    omitted[].observation_id
    relations[].from
    relations[].to

— and from nothing else. In particular they are NOT derived from
`required_observation_ids`, from the expected answers, or from which
identifiers happened to fail in L1: a protection set chosen by what the grader
wants would be a task oracle wearing a codec's clothes, and the measurement
would be of the oracle.

Occurrences are located by their key, so only true identity positions match; an
id merely *mentioned* inside a prose `statement` is not an identity position and
is left eligible for compression. Deduplication is by source range.

Output per task: a `qodec-protected-spans-v0` sidecar bound by `input_sha256`
to the exact canonical bytes that will be encoded.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from o7b1.canonical import canonical_bytes  # noqa: E402

RB = Path(__file__).resolve().parent.parent
Q2 = RB / "results" / "case-0002-q2-qodec-projection-arm-v1"
OUT = RB / "results" / "case-0002-l2-protected-representation-v1"

TASKS = [
    "case-0002-resume-product-integration",
    "case-0002-audit-r21-evidence-provenance",
    "case-0002-assess-change-impact-on-oracle-topology",
]

# The identity positions this context schema exposes. `observation_id` covers
# both `selected[]` and `omitted[]`; `from`/`to` cover `relations[]`.
IDENTITY_KEYS = ("observation_id", "from", "to")


def sha(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def identity_spans(payload: bytes):
    """Byte ranges of every identity *value* in `payload`.

    The range covers the value only, not its quotes or its key: the caller
    claims the identifier, and leaves the surrounding structure free to be
    compressed. Because the codec refuses to alias any phrase that merely
    *intersects* a claim, a phrase reaching across the opening quote into the
    value cannot be committed either, so the value stays whole without the
    quotes having to be claimed too.
    """
    spans = []
    for key in IDENTITY_KEYS:
        pattern = re.compile(b'"' + key.encode() + rb'":"((?:[^"\\]|\\.)*)"')
        for m in pattern.finditer(payload):
            spans.append((m.start(1), m.end(1)))
    # Dedup by source range, not by value: one range claimed twice is one claim.
    return sorted(set(spans))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    spans_dir = OUT / "protected-spans"
    spans_dir.mkdir(exist_ok=True)
    manifest = {
        "record": "o7.b1.l2-protected-spans-manifest/v0",
        "rule": ("every observation identity exposed by the context schema is machine-significant "
                 "and is preserved uniformly; derived from selected[].observation_id, "
                 "omitted[].observation_id, relations[].from, relations[].to -- never from "
                 "required_observation_ids, expected answers, or L1 failures"),
        "identity_keys": list(IDENTITY_KEYS),
        "tasks": {},
    }
    for tid in TASKS:
        ctx_path = Q2 / "qodec-project-arm" / "tasks" / tid / "context.json"
        # The exact bytes the frozen L1 Qodec arm encoded: canonical, no trailing LF.
        payload = canonical_bytes(json.loads(ctx_path.read_bytes()))
        spans = identity_spans(payload)
        values = []
        for s, e in spans:
            values.append(payload[s:e].decode("utf-8"))
        sidecar = {
            "schema": "qodec-protected-spans-v0",
            "input_sha256": sha(payload),
            "spans": [{"start": s, "end": e, "class": "opaque"} for s, e in spans],
        }
        side_bytes = (json.dumps(sidecar, indent=2, sort_keys=True) + "\n").encode()
        (spans_dir / f"{tid}.spans.json").write_bytes(side_bytes)
        (spans_dir / f"{tid}.payload.json").write_bytes(payload)
        manifest["tasks"][tid] = {
            "input_sha256": sha(payload),
            "input_bytes": len(payload),
            "protected_span_count": len(spans),
            "protected_source_bytes": sum(e - s for s, e in spans),
            "protected_unique_values": len(set(values)),
            "protected_manifest_sha256": sha(side_bytes),
            "protected_fraction_of_input": round(sum(e - s for s, e in spans) / len(payload), 4),
        }
        print(f"{tid.split('-', 2)[-1]:36} spans={len(spans):3} bytes={sum(e - s for s, e in spans):5} "
              f"unique={len(set(values)):3} of {len(payload)}b "
              f"({manifest['tasks'][tid]['protected_fraction_of_input'] * 100:.1f}%)")
    man_bytes = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    (OUT / "l2-protected-spans-manifest.json").write_bytes(man_bytes)
    print("manifest:", sha(man_bytes))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
