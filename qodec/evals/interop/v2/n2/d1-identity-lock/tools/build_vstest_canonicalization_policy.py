#!/usr/bin/env python3
"""Builds the immutable, self-hash-locked
vstest-capture-canonicalization-policy.json.

D1b decision (2026-07-16): Stage 1 acceptance for "repo-kubeops-generator"
no longer requires exact raw capture-a/capture-b byte equality. Instead, the
canonical benchmark input is a deterministic derivation of the raw, selected
stream through this narrowly scoped VSTest canonicalization profile
(implemented in vstest_canonicalizer.py, the single source of truth for the
actual regex/replacement logic -- this builder only packages that same logic
plus the evidence and policy metadata into one durable, self-hash-locked
record; it never hand-duplicates the regex text). Independent of
capture-canonicalization-policy.json (Maven) -- this profile is never merged
into it and never extended to a case_id outside its own applicable_case_ids.

Evidence run: workflow run 29466573023, artifact
n2d1b-pair-reproducibility-repo-kubeops-generator (artifact ID 8363205429) --
both underlying captures (pilot-repo-kubeops-generator-capture-a/-b)
independently passed content acceptance (real VSTest completion banner,
"Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61"), all
identity_mismatches were empty (source/toolchain/sandbox/argv identity agreed
exactly), but canonical_bytes_equal was false because the completion
banner's own wall-clock "Duration: N s" field differed between the two runs.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import vstest_canonicalizer as vc

OUT_PATH = Path(__file__).resolve().parents[1] / "vstest-capture-canonicalization-policy.json"

APPROVING_DECISION_IDENTITY = "n2d1b-vstest-canonicalization-authorization-2026-07-16"

# The real, bounded, line-level diff (raw capture-a vs raw capture-b,
# repo-kubeops-generator, run 29466573023) that justified the rule below --
# reproduced verbatim, not summarized, so the authorization is checkable
# against the actual evidence rather than a paraphrase of it.
EVIDENCE_BOUNDED_DIFF = (
    "--- capture-a\n"
    "+++ capture-b\n"
    "@@ -11,2 +11,2 @@\n"
    "\n"
    "-Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 2 s - KubeOps.Generator.Test.dll (net10.0)\n"
    "+Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 1 s - KubeOps.Generator.Test.dll (net10.0)\n"
)

RULE_EVIDENCE_JUSTIFICATION = {
    "vstest_duration": (
        "capture-a's raw stdout line 12 vs capture-b's: VSTest's own "
        "completion-banner logger prints the real wall-clock duration of "
        "its own test run (\"Duration: N s\"), with no known suppression "
        "flag; the identical Failed/Passed/Skipped/Total counts and the "
        "identical assembly/TFM tail (\"KubeOps.Generator.Test.dll "
        "(net10.0)\") prove both runs performed the same real work and "
        "differ only in this presentation field."
    ),
}

PROHIBITED_TRANSFORMATIONS = [
    "faking, freezing, or otherwise altering wall-clock time during the real test run",
    "altering the frozen/authorized dotnet test execution argv",
    "trimming leading/trailing whitespace beyond the rule's own anchored match",
    "deduplicating lines",
    "removing lines",
    "reordering lines",
    "a generic 'number followed by seconds' replacement across arbitrary diagnostic lines",
    "replacing Failed/Passed/Skipped/Total counts, or the assembly/TFM tail",
    "applying this rule to a case_id outside applicable_case_ids",
    "reusing the general-purpose canary sanitizer (sanitizer.sanitize) as the canonical-input transformer",
    "adding this rule to maven_canonicalizer.py or broadening capture-canonicalization-policy.json's scope",
]

UTF8_AND_LINE_ENDING_POLICY = (
    "Input must be valid UTF-8 (strict decode); invalid UTF-8 is a hard "
    "failure, never a lossy replace. Line order, line count, and each "
    "line's own original line-ending sequence (or absence of one, for a "
    "final line with no trailing newline) are preserved exactly -- the "
    "canonicalizer only ever substitutes the rule-matched duration value "
    "within a line's own content, never its terminator."
)


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


def build_policy() -> dict:
    rules = []
    for rule in vc.RULES:
        d = rule.to_policy_dict()
        d["evidence_justification"] = RULE_EVIDENCE_JUSTIFICATION[rule.name]
        rules.append(d)

    body = {
        "policy_type": "n2d1b-capture-canonicalization-policy-v1",
        "policy_version": 1,
        "applicable_case_ids": ["repo-kubeops-generator"],
        "selected_source_stream": "stdout",
        "canonicalizer_module": "vstest_canonicalizer.py",
        "rules": rules,
        "evidence_run": {
            "workflow_run_id": 29466573023,
            "pair_reproducibility_artifact": {
                "name": "n2d1b-pair-reproducibility-repo-kubeops-generator",
                "artifact_id": 8363205429,
            },
            "both_captures_independently_content_accepted": True,
            "identity_mismatches_were_empty": True,
        },
        "evidence_bounded_diff": EVIDENCE_BOUNDED_DIFF,
        "prohibited_transformations": PROHIBITED_TRANSFORMATIONS,
        "utf8_and_line_ending_policy": UTF8_AND_LINE_ENDING_POLICY,
        "approving_decision_identity": APPROVING_DECISION_IDENTITY,
        "scope_statement": (
            "This policy applies ONLY to the case_id(s) listed in "
            "applicable_case_ids. Any other case found to need additional "
            "canonicalization rules must stop for separate D1b review -- "
            "this policy is not extended silently, and is never merged "
            "with capture-canonicalization-policy.json (Maven)."
        ),
    }
    _, digest = canonicalize_and_hash(body)
    body["policy_sha256"] = digest
    return body


def main() -> int:
    body = build_policy()
    without_hash = {k: v for k, v in body.items() if k != "policy_sha256"}
    _, recomputed = canonicalize_and_hash(without_hash)
    assert recomputed == body["policy_sha256"], "self-hash did not verify stable"
    OUT_PATH.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n")
    print(f"wrote {OUT_PATH} (policy_sha256={body['policy_sha256']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
