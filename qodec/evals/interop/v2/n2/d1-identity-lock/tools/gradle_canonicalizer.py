#!/usr/bin/env python3
"""N2-D1b: strict, case-specific canonicalizer for repo-moshi's raw Gradle
stdout.

D1b-authorized (2026-07-16, see gradle-capture-canonicalization-policy.json)
only AFTER the case's own deterministic scheduling profile
(org.gradle.parallel=false, org.gradle.workers.max=1, org.gradle.console=
plain/non-interactive -- see run_pilot_case.py) was confirmed to make every
task-execution line byte-identical and same-order between capture-a and
capture-b (real CI evidence, workflow run 29474204715): the ONLY remaining
raw difference was Gradle's own build-completion banner's wall-clock
duration --

  BUILD SUCCESSFUL in 1m 50s
  BUILD SUCCESSFUL in 1m 11s

-- identical "42 actionable tasks: 42 executed" summary and identical task
log otherwise, differing only in the real wall-clock seconds Gradle
measured for its own run, with no known suppression flag.

Independent of maven_canonicalizer.py's and vstest_canonicalizer.py's own
rule sets -- this module never touches Maven or VSTest output and is never
extended to a case_id outside its own applicable_case_ids (see
gradle-capture-canonicalization-policy.json's applicable_case_ids for the
single source of truth on scope). It reuses maven_canonicalizer.py's
generic primitives (Rule, CanonicalizerError, PolicyIntegrityError, hashing/
line-ending helpers) verbatim -- these are canonicalization-engine
plumbing, not Maven-specific logic -- but defines its own RULES, its own
canonicalize_stream, and its own load_and_verify_policy.
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

# --- Rule: Gradle build-completion banner's wall-clock duration -------------
_PREFIX = "BUILD SUCCESSFUL in "
# Gradle's documented duration format: an optional hours part, an optional
# minutes part, and a mandatory seconds part (e.g. "3s", "3m 12s",
# "1h 3m 12s") -- only the "Nm Ns" shape has been directly observed in real
# evidence so far; any other shape (including a future observed variant)
# must fail closed for separate review, never silently pass through.
_PATTERN = re.compile(
    "^" + re.escape(_PREFIX) + r"((?:\d+h )?(?:\d+m )?\d+s|<ELAPSED>)$"
)


def _apply_rule(m: "re.Match[str]") -> str:
    return _PREFIX + "<ELAPSED>"


RULE_GRADLE_BUILD_DURATION = Rule(
    name="gradle_build_duration",
    trigger=_PREFIX,
    pattern=_PATTERN,
    placeholder="<ELAPSED>",
    apply=_apply_rule,
)

# Only one rule -- a changed task count, a genuine BUILD FAILED, or a
# malformed banner, must never be silently canonicalized away; the loop
# below fails closed on any line containing the trigger substring that
# doesn't match this exact anchored grammar.
RULES: list[Rule] = [RULE_GRADLE_BUILD_DURATION]


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
        "report_type": "n2d1b-gradle-canonicalization-report-v1",
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
    from gradle_canonicalizer.RULES is a hard failure."""
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
            f"gradle_canonicalizer.RULES {sorted(code_rule_names)} -- policy and code have drifted"
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
                "actual gradle_canonicalizer.py rule it is supposed to document"
            )

    if not body.get("applicable_case_ids"):
        raise PolicyIntegrityError(f"{policy_path}: applicable_case_ids is empty or missing")

    return body
