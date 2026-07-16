#!/usr/bin/env python3
"""N2-D1b: strict, case-specific canonicalizer for repo-kubeops-generator's
raw VSTest stdout.

D1b-authorized (2026-07-16, see vstest-capture-canonicalization-policy.json)
after real CI evidence (workflow run 29466573023, artifact
n2d1b-pair-reproducibility-repo-kubeops-generator) showed capture-a and
capture-b's raw stdout differ in exactly one line -- VSTest's own completion
banner's wall-clock test-run duration:

  Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 2 s - KubeOps.Generator.Test.dll (net10.0)
  Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 1 s - KubeOps.Generator.Test.dll (net10.0)

-- identical pass/fail/skipped/total counts and identical assembly/TFM tail,
differing only in the real wall-clock seconds VSTest measured for its own
run, with no known suppression flag.

Independent of maven_canonicalizer.py's own rule set -- this module never
touches Maven output and is never extended to a case_id outside its own
applicable_case_ids (see vstest-capture-canonicalization-policy.json's
applicable_case_ids for the single source of truth on scope). It reuses
maven_canonicalizer.py's generic primitives (Rule, CanonicalizerError,
PolicyIntegrityError, hashing/line-ending helpers) verbatim -- these are
canonicalization-engine plumbing, not Maven-specific logic -- but defines its
own RULES, its own canonicalize_stream, and its own load_and_verify_policy,
so a Maven-format change can never silently alter what this module accepts
and vice versa.
"""
from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from maven_canonicalizer import (  # noqa: F401 -- re-exported for callers
    CanonicalizerError,
    PolicyIntegrityError,
    Rule,
    _sha256,
    _split_line_ending,
)

# --- Rule: VSTest completion banner's wall-clock duration -------------------
_PREFIX = "Passed!  - Failed:     "
_MID_A = ", Passed:    "
_MID_B = ", Skipped:     "
_MID_C = ", Total:    "
_MID_D = ", Duration: "
_MID_E = " s - "
_PATTERN = re.compile(
    "^" + re.escape(_PREFIX) + r"(\d+)" + re.escape(_MID_A) + r"(\d+)" + re.escape(_MID_B)
    + r"(\d+)" + re.escape(_MID_C) + r"(\d+)" + re.escape(_MID_D)
    + r"(\d+(?:\.\d+)?|<ELAPSED>)" + re.escape(_MID_E) + r"(.+)$"
)


def _apply_rule(m: "re.Match[str]") -> str:
    failed, passed, skipped, total, _duration, suite_tail = m.groups()
    return (
        _PREFIX + failed + _MID_A + passed + _MID_B + skipped + _MID_C + total
        + _MID_D + "<ELAPSED>" + _MID_E + suite_tail
    )


RULE_VSTEST_DURATION = Rule(
    name="vstest_duration",
    trigger="Duration: ",
    pattern=_PATTERN,
    placeholder="<ELAPSED>",
    apply=_apply_rule,
)

# Only one rule -- a changed pass/fail/skipped/total count, or a malformed
# banner, must never be silently canonicalized away; the loop below fails
# closed on any line containing the trigger substring that doesn't match
# this exact anchored grammar.
RULES: list[Rule] = [RULE_VSTEST_DURATION]


def canonicalize_stream(raw_bytes: bytes) -> tuple[bytes, dict]:
    """Returns (canonicalized_bytes, report). Raises CanonicalizerError if
    `raw_bytes` is not valid UTF-8, or if any line contains the rule's
    trigger substring but does not conform to its anchored expected
    grammar."""
    try:
        text = raw_bytes.decode("utf-8", errors="strict")
    except UnicodeDecodeError as e:
        raise CanonicalizerError(f"input is not valid UTF-8: {e}") from e

    lines = text.splitlines(keepends=True)
    replacements: list[dict] = []
    rule_match_counts = {rule.name: 0 for rule in RULES}
    out_lines: list[str] = []

    for line_number, line in enumerate(lines, start=1):
        content, ending = _split_line_ending(line)
        current = content
        for rule in RULES:
            if rule.trigger not in current:
                continue
            m = rule.pattern.match(current)
            if not m:
                raise CanonicalizerError(
                    f"line {line_number}: contains trigger {rule.trigger!r} for rule "
                    f"{rule.name!r} but does not match its anchored expected grammar: {current!r}"
                )
            rule_match_counts[rule.name] += 1
            replaced = rule.apply(m)
            if replaced != current:
                replacements.append({
                    "rule_name": rule.name,
                    "line_number": line_number,
                    "before_line_sha256": _sha256(current.encode("utf-8")),
                    "after_line_sha256": _sha256(replaced.encode("utf-8")),
                })
            current = replaced
        out_lines.append(current + ending)

    canonical_text = "".join(out_lines)
    canonical_bytes = canonical_text.encode("utf-8")
    report = {
        "report_type": "n2d1b-vstest-canonicalization-report-v1",
        "line_count_in": len(lines),
        "line_count_out": len(out_lines),
        "trailing_newline_preserved": raw_bytes.endswith(b"\n") == canonical_bytes.endswith(b"\n"),
        "rule_match_counts": rule_match_counts,
        "replacement_count": len(replacements),
        "replacements": replacements,
    }
    return canonical_bytes, report


def load_and_verify_policy(policy_path: Path) -> dict:
    """Same integrity discipline as maven_canonicalizer.load_and_verify_policy,
    verified against THIS module's own RULES -- never merely trusts the
    policy file's embedded hash, and a documented rule set that has drifted
    from vstest_canonicalizer.RULES is a hard failure."""
    body = json.loads(policy_path.read_text())
    if "policy_sha256" not in body:
        raise PolicyIntegrityError(f"{policy_path}: missing policy_sha256 -- not a self-hash-locked policy record")
    recorded = body["policy_sha256"]
    without_hash = {k: v for k, v in body.items() if k != "policy_sha256"}
    canonical_text = json.dumps(without_hash, indent=2, sort_keys=True) + "\n"
    recomputed = hashlib.sha256(canonical_text.encode("utf-8")).hexdigest()
    if recomputed != recorded:
        raise PolicyIntegrityError(
            f"{policy_path}: policy_sha256 {recorded} does not match recomputed {recomputed} "
            "-- refusing to trust a tampered or corrupted canonicalization policy"
        )

    documented_rules = {r["rule_name"]: r for r in body.get("rules", [])}
    code_rule_names = {rule.name for rule in RULES}
    if set(documented_rules) != code_rule_names:
        raise PolicyIntegrityError(
            f"{policy_path}: documented rule set {sorted(documented_rules)} does not match "
            f"vstest_canonicalizer.RULES {sorted(code_rule_names)} -- policy and code have drifted"
        )
    for rule in RULES:
        documented = documented_rules[rule.name]
        if (
            documented["anchored_regex"] != rule.pattern.pattern
            or documented["trigger_substring"] != rule.trigger
            or documented["placeholder"] != rule.placeholder
        ):
            raise PolicyIntegrityError(
                f"{policy_path}: rule {rule.name!r} in the policy file does not match the "
                "actual vstest_canonicalizer.py rule it is supposed to document"
            )

    if not body.get("applicable_case_ids"):
        raise PolicyIntegrityError(f"{policy_path}: applicable_case_ids is empty or missing")

    return body
