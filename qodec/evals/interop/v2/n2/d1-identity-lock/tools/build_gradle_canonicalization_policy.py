#!/usr/bin/env python3
"""Builds the immutable, self-hash-locked
gradle-capture-canonicalization-policy.json.

D1b decision (2026-07-16): Stage 1 acceptance for "repo-moshi" no longer
requires exact raw capture-a/capture-b byte equality. Instead, the canonical
benchmark input is a deterministic derivation of the raw, selected stream
through this narrowly scoped Gradle canonicalization profile (implemented
in gradle_canonicalizer.py, the single source of truth for the actual
regex/replacement logic -- this builder only packages that same logic plus
the evidence and policy metadata into one durable, self-hash-locked record;
it never hand-duplicates the regex text). Independent of
capture-canonicalization-policy.json (Maven) and
vstest-capture-canonicalization-policy.json (VSTest) -- this profile is
never merged into either and never extended to a case_id outside its own
applicable_case_ids.

Authorized only AFTER repo-moshi's own deterministic scheduling profile
(org.gradle.parallel=false, org.gradle.workers.max=1, plain non-interactive
console -- see run_pilot_case.py) was confirmed by real CI evidence
(workflow run 29474204715) to make every task-execution line byte-identical
and same-order between capture-a and capture-b -- the ONLY remaining raw
difference was the build-completion banner's own wall-clock duration.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import gradle_canonicalizer as gcz

OUT_PATH = Path(__file__).resolve().parents[1] / "gradle-capture-canonicalization-policy.json"

APPROVING_DECISION_IDENTITY = "n2d1b-gradle-canonicalization-authorization-2026-07-16"

# The real, bounded, line-level diff (raw capture-a vs raw capture-b,
# repo-moshi, run 29474204715, AFTER the deterministic scheduling profile
# was already applied) that justified the rule below -- reproduced
# verbatim, not summarized, so the authorization is checkable against the
# actual evidence rather than a paraphrase of it.
EVIDENCE_BOUNDED_DIFF = (
    "--- capture-a\n"
    "+++ capture-b\n"
    "@@ -215,3 +215,3 @@\n"
    "\n"
    "-BUILD SUCCESSFUL in 1m 50s\n"
    "+BUILD SUCCESSFUL in 1m 11s\n"
    " 42 actionable tasks: 42 executed\n"
)

RULE_EVIDENCE_JUSTIFICATION = {
    "gradle_build_duration": (
        "capture-a's raw stdout line 216 vs capture-b's: Gradle's own "
        "build-completion banner prints the real wall-clock duration of "
        "its own run (\"BUILD SUCCESSFUL in N\"), with no known "
        "suppression flag; the identical \"42 actionable tasks: 42 "
        "executed\" summary line, and every task-execution line above it "
        "being byte-identical and same-order (confirmed only after the "
        "deterministic scheduling profile was applied), prove both runs "
        "performed the same real work and differ only in this "
        "presentation field."
    ),
}

PROHIBITED_TRANSFORMATIONS = [
    "faking, freezing, or otherwise altering wall-clock time during the real build",
    "altering the frozen/authorized ./gradlew test execution argv",
    "trimming leading/trailing whitespace beyond the rule's own anchored match",
    "deduplicating lines",
    "removing lines",
    "reordering lines",
    "a generic 'number followed by seconds/minutes' replacement across arbitrary diagnostic lines",
    "replacing the actionable-tasks count, task names, or any other build output",
    "applying this rule to a case_id outside applicable_case_ids",
    "reusing the general-purpose canary sanitizer (sanitizer.sanitize) as the canonical-input transformer",
    "adding this rule to maven_canonicalizer.py/vstest_canonicalizer.py or broadening their own scopes",
    "using this rule as a substitute for the deterministic scheduling profile -- it does not mask task-log ordering",
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
    for rule in gcz.RULES:
        d = rule.to_policy_dict()
        d["evidence_justification"] = RULE_EVIDENCE_JUSTIFICATION[rule.name]
        rules.append(d)

    body = {
        "policy_type": "n2d1b-capture-canonicalization-policy-v1",
        "policy_version": 1,
        "applicable_case_ids": ["repo-moshi"],
        "selected_source_stream": "stdout",
        "canonicalizer_module": "gradle_canonicalizer.py",
        "rules": rules,
        "requires_deterministic_scheduling_profile": True,
        "evidence_run": {
            "workflow_run_id": 29474204715,
            "pair_reproducibility_artifact": {
                "name": "n2d1b-pair-reproducibility-repo-moshi",
            },
            "both_captures_independently_content_accepted": True,
            "identity_mismatches_were_empty": True,
            "applied_after_deterministic_scheduling_profile_confirmed_sole_remaining_diff": True,
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
            "with capture-canonicalization-policy.json (Maven) or "
            "vstest-capture-canonicalization-policy.json (VSTest)."
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
