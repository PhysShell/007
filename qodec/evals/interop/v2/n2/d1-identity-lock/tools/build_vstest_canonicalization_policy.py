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

Evidence run (v1, vstest_duration rule): workflow run 29466573023, artifact
n2d1b-pair-reproducibility-repo-kubeops-generator (artifact ID 8363205429) --
both underlying captures (pilot-repo-kubeops-generator-capture-a/-b)
independently passed content acceptance (real VSTest completion banner,
"Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61"), all
identity_mismatches were empty (source/toolchain/sandbox/argv identity agreed
exactly), but canonical_bytes_equal was false because the completion
banner's own wall-clock "Duration: N s" field differed between the two runs.

Evidence run (v2, msbuild_completion_pair_order structural rule): workflow
run 29469560893, same artifact family -- AFTER the vstest_duration rule
already resolved the Duration difference, a second, structurally distinct
raw difference remained: two of MSBuild's own project-completion lines
("KubeOps.Generator -> ...dll" and "KubeOps.Generator.Test.Entities ->
...dll") appeared in swapped position between capture-a and capture-b,
identical byte-for-byte content otherwise -- real evidence of
nondeterministic ordering from concurrent/parallel project compilation
within the same `dotnet test` invocation (confirmed intermittent by a later
run, 29470199739, where the pair happened to compile in the same order and
canonical_bytes_equal was true without this rule). This is a SEPARATE D1b
authorization (2026-07-16) from the v1 Duration rule -- see
APPROVING_DECISION_IDENTITY_V2 -- not a silent broadening of the original
one.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

import vstest_canonicalizer as vc

OUT_PATH = Path(__file__).resolve().parents[1] / "vstest-capture-canonicalization-policy.json"

APPROVING_DECISION_IDENTITY_V1 = "n2d1b-vstest-canonicalization-authorization-2026-07-16"
APPROVING_DECISION_IDENTITY_V2 = "n2d1b-vstest-canonicalization-authorization-2026-07-16-v2-msbuild-order"

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

STRUCTURAL_RULE_EVIDENCE_JUSTIFICATION = {
    "msbuild_completion_pair_order": (
        "Run 29469560893's raw stdout: capture-a printed \"KubeOps.Generator."
        "Test.Entities -> ...dll\" then \"KubeOps.Generator -> ...dll\"; "
        "capture-b printed them in the opposite order -- byte-identical "
        "content otherwise, and a third project's completion line "
        "(\"KubeOps.Generator.Test -> ...dll\") stayed in the same relative "
        "position in both. MSBuild builds independent projects concurrently "
        "by default and prints each project's own completion line as soon "
        "as that project finishes, so which of two independently-building "
        "projects finishes first is a genuine wall-clock race, not a content "
        "difference -- confirmed intermittent by a later run (29470199739) "
        "where the same two projects happened to complete in the same order "
        "and canonical_bytes_equal was true without this rule."
    ),
}

PROHIBITED_TRANSFORMATIONS = [
    "faking, freezing, or otherwise altering wall-clock time during the real test run",
    "altering the frozen/authorized dotnet test execution argv",
    "trimming leading/trailing whitespace beyond a rule's own anchored match",
    "deduplicating lines",
    "removing lines",
    "reordering any line other than the two exact, named MSBuild completion lines authorized below",
    "disabling MSBuild parallelism or otherwise making pair verification generally order-insensitive",
    "a generic 'number followed by seconds' replacement across arbitrary diagnostic lines",
    "replacing Failed/Passed/Skipped/Total counts, or the assembly/TFM tail",
    "applying either rule to a case_id outside applicable_case_ids",
    "reusing the general-purpose canary sanitizer (sanitizer.sanitize) as the canonical-input transformer",
    "adding either rule to maven_canonicalizer.py or broadening capture-canonicalization-policy.json's scope",
    "sorting every MSBuild '->' line or any project line outside the two authorized project names",
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


STRUCTURAL_EVIDENCE_BOUNDED_DIFF = (
    "--- capture-a\n"
    "+++ capture-b\n"
    "@@ -5,4 +5,4 @@\n"
    " /home/runner/.nuget/packages/microsoft.sourcelink.common/10.0.300/build/Microsoft.SourceLink.Common.targets(56,5): warning : Source control information is not available - the generated source link is empty. [.../KubeOps.Generator.csproj]\n"
    "-  KubeOps.Generator.Test.Entities -> .../KubeOps.Generator.Test.Entities.dll\n"
    "-  KubeOps.Generator -> .../KubeOps.Generator.dll\n"
    "+  KubeOps.Generator -> .../KubeOps.Generator.dll\n"
    "+  KubeOps.Generator.Test.Entities -> .../KubeOps.Generator.Test.Entities.dll\n"
    "   KubeOps.Generator.Test -> .../KubeOps.Generator.Test.dll\n"
)


def build_policy() -> dict:
    rules = []
    for rule in vc.RULES:
        d = rule.to_policy_dict()
        d["evidence_justification"] = RULE_EVIDENCE_JUSTIFICATION[rule.name]
        rules.append(d)

    structural_rules = []
    for structural_rule in vc.STRUCTURAL_RULES:
        d = dict(structural_rule)
        d["evidence_justification"] = STRUCTURAL_RULE_EVIDENCE_JUSTIFICATION[d["rule_name"]]
        structural_rules.append(d)

    body = {
        "policy_type": "n2d1b-capture-canonicalization-policy-v1",
        "policy_version": 2,
        "applicable_case_ids": ["repo-kubeops-generator"],
        "selected_source_stream": "stdout",
        "canonicalizer_module": "vstest_canonicalizer.py",
        "rules": rules,
        "structural_rules": structural_rules,
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
        "structural_evidence_run": {
            "workflow_run_id": 29469560893,
            "confirmed_intermittent_by_workflow_run_id": 29470199739,
        },
        "structural_evidence_bounded_diff": STRUCTURAL_EVIDENCE_BOUNDED_DIFF,
        "prohibited_transformations": PROHIBITED_TRANSFORMATIONS,
        "utf8_and_line_ending_policy": UTF8_AND_LINE_ENDING_POLICY,
        "approving_decision_identity": APPROVING_DECISION_IDENTITY_V2,
        "superseded_decision_identities": [APPROVING_DECISION_IDENTITY_V1],
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
