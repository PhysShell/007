#!/usr/bin/env python3
"""Builds the immutable, self-hash-locked capture-canonicalization-policy.json.

D1b decision (2026-07-16): Stage 1 acceptance for "repo-docker-java-parser"
no longer requires exact raw capture-a/capture-b byte equality. Instead, the
canonical benchmark input is a deterministic derivation of the raw, selected
stream through this narrowly scoped Maven canonicalization profile
(implemented in maven_canonicalizer.py, the single source of truth for the
actual regex/replacement logic -- this builder only packages that same logic
plus the evidence and policy metadata into one durable, self-hash-locked
record; it never hand-duplicates the regex text).

Evidence run: workflow run 29436883023 (commit 216116af7de9f56caa6dd44228715e798fe172e7),
artifacts n2d1b-pilot-repo-docker-java-parser-capture-a (digest
sha256:64bf92980b12746bdea5e60379689dac48d2905564c1272ce22aba2fb39bd31d) and
n2d1b-pilot-repo-docker-java-parser-capture-b (digest
sha256:203d7b979043e11af350bee7335ff63906ede2873cc399ec1fec54f73272baa7) --
both captures independently passed content acceptance (real BUILD SUCCESS,
real "Tests run: 3, Failures: 0", exit_code 0) but their raw stdout differed
byte-for-byte in exactly the five lines the bounded diff below shows.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import maven_canonicalizer as mc

OUT_PATH = Path(__file__).resolve().parents[1] / "capture-canonicalization-policy.json"

APPROVING_DECISION_IDENTITY = "n2d1b-maven-canonicalization-authorization-2026-07-16"

# The real, bounded, line-level diff (raw capture-a vs raw capture-b,
# repo-docker-java-parser, run 29436883023) that justified every rule below
# -- reproduced verbatim, not summarized, so the authorization is checkable
# against the actual evidence rather than a paraphrase of it.
EVIDENCE_BOUNDED_DIFF = (
    "24c24\n"
    "< [\\x1b[1;34mINFO\\x1b[m] Storing buildNumber: null at timestamp: 1784136905453\n"
    "---\n"
    "> [\\x1b[1;34mINFO\\x1b[m] Storing buildNumber: null at timestamp: 1784136890043\n"
    "59c59\n"
    "< [\\x1b[1;34mINFO\\x1b[m] compile in 11.4 s\n"
    "---\n"
    "> [\\x1b[1;34mINFO\\x1b[m] compile in 10.5 s\n"
    "80c80\n"
    "< [\\x1b[1;34mINFO\\x1b[m] compile in 3.3 s\n"
    "---\n"
    "> [\\x1b[1;34mINFO\\x1b[m] compile in 3.1 s\n"
    "94c94\n"
    "< [\\x1b[1;34mINFO\\x1b[m] [\\x1b[1;32mTests run: [\\x1b[0;1;32m3[\\x1b[m, Failures: 0, Errors: 0, "
    "Skipped: 0, Time elapsed: 0.14 s - in com.github.thstock.djp.[\\x1b[1mScalaParserTest[\\x1b[m\n"
    "---\n"
    "> [\\x1b[1;34mINFO\\x1b[m] [\\x1b[1;32mTests run: [\\x1b[0;1;32m3[\\x1b[m, Failures: 0, Errors: 0, "
    "Skipped: 0, Time elapsed: 0.134 s - in com.github.thstock.djp.[\\x1b[1mScalaParserTest[\\x1b[m\n"
    "103,104c103,104\n"
    "< [\\x1b[1;34mINFO\\x1b[m] Total time:  18.865 s\n"
    "< [\\x1b[1;34mINFO\\x1b[m] Finished at: 2026-07-15T17:35:22Z\n"
    "---\n"
    "> [\\x1b[1;34mINFO\\x1b[m] Total time:  17.597 s\n"
    "> [\\x1b[1;34mINFO\\x1b[m] Finished at: 2026-07-15T17:35:06Z\n"
)

RULE_EVIDENCE_JUSTIFICATION = {
    "buildnumber_timestamp": (
        "Diff hunk 24c24: buildnumber-maven-plugin (bound to the `validate` "
        "phase) prints its own wall-clock read with no known suppress/"
        "override property in this plugin version."
    ),
    "scala_compile_duration": (
        "Diff hunks 59c59 and 80c80: scala-maven-plugin prints a per-module "
        "'compile in N s' line after each of this project's two Scala "
        "compilation units; both are real wall-clock durations of the live "
        "compiler invocation, not a config-controllable value."
    ),
    "surefire_time_elapsed": (
        "Diff hunk 94c94: surefire's own JUnit runner prints the real "
        "wall-clock elapsed time for the test class inside its "
        "'Tests run: ...' summary line -- test counts/failures/errors/"
        "skipped/suite name are stable and must never be touched by this "
        "rule."
    ),
    "maven_total_time": (
        "Diff hunk 103,104c103,104 (first line): Maven core's own "
        "ExecutionEventLogger prints the real wall-clock total build "
        "duration; no suppress flag exists that keeps the BUILD SUCCESS/"
        "Tests-run banner while hiding only this line (`-q` would suppress "
        "both)."
    ),
    "maven_finished_at": (
        "Diff hunk 103,104c103,104 (second line): Maven core's own "
        "ExecutionEventLogger prints the real wall-clock build-completion "
        "timestamp; same suppression constraint as maven_total_time."
    ),
}

PROHIBITED_TRANSFORMATIONS = [
    "faking, freezing, or otherwise altering wall-clock time during the real build",
    "altering the frozen Maven execution argv",
    "trimming leading/trailing whitespace beyond a rule's own anchored match",
    "deduplicating lines",
    "removing lines",
    "reordering lines",
    "a generic 'number followed by seconds' replacement across arbitrary diagnostic lines",
    "replacing test counts, failure counts, error counts, skipped counts, or suite names",
    "applying any rule to a case_id outside applicable_case_ids",
    "reusing the general-purpose canary sanitizer (sanitizer.sanitize) as the canonical-input transformer",
]

UTF8_AND_LINE_ENDING_POLICY = (
    "Input must be valid UTF-8 (strict decode); invalid UTF-8 is a hard "
    "failure, never a lossy replace. Line order, line count, and each "
    "line's own original line-ending sequence (or absence of one, for a "
    "final line with no trailing newline) are preserved exactly -- the "
    "canonicalizer only ever substitutes a rule-matched value within a "
    "line's own content, never its terminator."
)


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


def build_policy() -> dict:
    rules = []
    for rule in mc.RULES:
        d = rule.to_policy_dict()
        d["evidence_justification"] = RULE_EVIDENCE_JUSTIFICATION[rule.name]
        rules.append(d)

    body = {
        "policy_type": "n2d1b-capture-canonicalization-policy-v1",
        "policy_version": 1,
        "applicable_case_ids": ["repo-docker-java-parser"],
        "selected_source_stream": "stdout",
        "canonicalizer_module": "maven_canonicalizer.py",
        "rules": rules,
        "evidence_run": {
            "workflow_run_id": 29436883023,
            "head_sha": "216116af7de9f56caa6dd44228715e798fe172e7",
            "capture_a_artifact": {
                "name": "n2d1b-pilot-repo-docker-java-parser-capture-a",
                "raw_stdout_sha256": "64bf92980b12746bdea5e60379689dac48d2905564c1272ce22aba2fb39bd31d",
            },
            "capture_b_artifact": {
                "name": "n2d1b-pilot-repo-docker-java-parser-capture-b",
                "raw_stdout_sha256": "203d7b979043e11af350bee7335ff63906ede2873cc399ec1fec54f73272baa7",
            },
            "both_captures_independently_content_accepted": True,
        },
        "evidence_bounded_diff": EVIDENCE_BOUNDED_DIFF,
        "prohibited_transformations": PROHIBITED_TRANSFORMATIONS,
        "utf8_and_line_ending_policy": UTF8_AND_LINE_ENDING_POLICY,
        "approving_decision_identity": APPROVING_DECISION_IDENTITY,
        "scope_statement": (
            "This policy applies ONLY to the case_id(s) listed in "
            "applicable_case_ids. Any other case found to need additional "
            "canonicalization rules must stop for separate D1b review -- "
            "this policy is not extended silently."
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
