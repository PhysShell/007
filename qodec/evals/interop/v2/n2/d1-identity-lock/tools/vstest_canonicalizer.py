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

Policy v2 (D1b-authorized 2026-07-16) added a second, structurally distinct
rule after further real evidence (CI run 29469560893) showed a second,
independent nondeterminism: two of MSBuild's own project-completion lines
("<Project> -> <path>") appear in swapped order between capture-a/capture-b,
from concurrent/parallel project compilation within the same `dotnet test`
invocation (confirmed intermittent -- not every pair collides). This rule
(msbuild_completion_pair_order) is a bounded PERMUTATION of exactly two
named project lines into a fixed declared order, never a general sort of
arbitrary MSBuild output -- see _reorder_msbuild_completion_pair.

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

# Line-substitution rules -- a changed pass/fail/skipped/total count, or a
# malformed banner, must never be silently canonicalized away; the loop
# below fails closed on any line containing the trigger substring that
# doesn't match its exact anchored grammar.
RULES: list[Rule] = [RULE_VSTEST_DURATION]

# --- Structural rule: MSBuild project-completion pair ordering -------------
# D1b authorization (2026-07-16, policy v2): repo-kubeops-generator's raw
# stdout showed two MSBuild project-completion lines ("<Project> -> <path>")
# in swapped position between capture-a/capture-b -- real evidence of
# nondeterministic ordering from concurrent/parallel project compilation
# within the same `dotnet test` invocation (confirmed intermittent: not
# every pair collides). Bounded to EXACTLY these two named projects, in
# this fixed declared order -- never a general "sort every MSBuild -> line"
# transform, and never applied unless every hard precondition below holds.
MSBUILD_LINE_PREFIX = "  "
MSBUILD_ARROW = " -> "
MSBUILD_PROJECT_ORDER = ["KubeOps.Generator", "KubeOps.Generator.Test.Entities"]

_MSBUILD_PATTERNS = {
    name: re.compile("^" + re.escape(MSBUILD_LINE_PREFIX + name + MSBUILD_ARROW) + r"(.+)$")
    for name in MSBUILD_PROJECT_ORDER
}

STRUCTURAL_RULE_MSBUILD_COMPLETION_PAIR_ORDER = {
    "rule_name": "msbuild_completion_pair_order",
    "operation_type": "bounded_permutation",
    "authorized_project_order": list(MSBUILD_PROJECT_ORDER),
    "line_grammar": "  <ProjectName> -> <path>",
}
STRUCTURAL_RULES: list[dict] = [STRUCTURAL_RULE_MSBUILD_COMPLETION_PAIR_ORDER]


def _reorder_msbuild_completion_pair(full_lines: list[str]) -> tuple[list[str], dict | None]:
    """Returns (possibly-reordered full_lines, structural_operation_report or
    None if the rule is not applicable -- neither authorized line is
    present). Raises CanonicalizerError (fail closed) if:

      - either authorized project's completion line occurs more than once;
      - only ONE of the two authorized lines is present (not both, not
        neither);
      - any non-blank content (a warning, a third project's completion
        line, test output, anything) appears between the two lines --
        they must form one contiguous block, blank lines aside.

    Never touches any OTHER "<Project> -> <path>" line -- e.g.
    "KubeOps.Generator.Test -> ..." is untouched even when immediately
    adjacent to the reordered pair.
    """
    positions: dict[str, int] = {}
    for name, pattern in _MSBUILD_PATTERNS.items():
        matches = [
            i for i, full_line in enumerate(full_lines)
            if pattern.match(_split_line_ending(full_line)[0])
        ]
        if len(matches) > 1:
            raise CanonicalizerError(
                f"msbuild_completion_pair_order: project {name!r} completion line occurs "
                f"{len(matches)} times, expected at most 1"
            )
        if matches:
            positions[name] = matches[0]

    present = set(positions)
    authorized = set(MSBUILD_PROJECT_ORDER)
    if not present:
        return full_lines, None
    if present != authorized:
        raise CanonicalizerError(
            f"msbuild_completion_pair_order: expected both {sorted(authorized)} or neither "
            f"present, found only {sorted(present)}"
        )

    i, j = positions[MSBUILD_PROJECT_ORDER[0]], positions[MSBUILD_PROJECT_ORDER[1]]
    lo, hi = min(i, j), max(i, j)
    between = full_lines[lo + 1:hi]
    if any(_split_line_ending(full_line)[0].strip() != "" for full_line in between):
        raise CanonicalizerError(
            "msbuild_completion_pair_order: the two authorized project-completion lines "
            "are not contiguous (non-blank content found between them)"
        )

    before_block = "".join(full_lines[lo:hi + 1])
    if i < j:
        # Already in canonical order -- no-op, idempotent by construction.
        return full_lines, {
            "rule_name": "msbuild_completion_pair_order",
            "already_canonical": True,
            "before_block_sha256": _sha256(before_block.encode("utf-8")),
            "after_block_sha256": _sha256(before_block.encode("utf-8")),
        }
    new_lines = list(full_lines)
    new_lines[lo], new_lines[hi] = new_lines[hi], new_lines[lo]
    after_block = "".join(new_lines[lo:hi + 1])
    return new_lines, {
        "rule_name": "msbuild_completion_pair_order",
        "already_canonical": False,
        "before_block_sha256": _sha256(before_block.encode("utf-8")),
        "after_block_sha256": _sha256(after_block.encode("utf-8")),
    }


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

    # Structural pass (after the per-line Duration substitution): bounded,
    # narrowly-scoped reordering of the two authorized MSBuild
    # project-completion lines -- see _reorder_msbuild_completion_pair's own
    # docstring for the exact fail-closed preconditions.
    out_lines, structural_report = _reorder_msbuild_completion_pair(out_lines)

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
        "structural_operations": [structural_report] if structural_report is not None else [],
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

    documented_structural_rules = {r["rule_name"]: r for r in body.get("structural_rules", [])}
    code_structural_rule_names = {r["rule_name"] for r in STRUCTURAL_RULES}
    if set(documented_structural_rules) != code_structural_rule_names:
        raise PolicyIntegrityError(
            f"{policy_path}: documented structural rule set {sorted(documented_structural_rules)} "
            f"does not match vstest_canonicalizer.STRUCTURAL_RULES {sorted(code_structural_rule_names)}"
            " -- policy and code have drifted"
        )
    for structural_rule in STRUCTURAL_RULES:
        documented = documented_structural_rules[structural_rule["rule_name"]]
        if (
            documented["operation_type"] != structural_rule["operation_type"]
            or documented["authorized_project_order"] != structural_rule["authorized_project_order"]
            or documented["line_grammar"] != structural_rule["line_grammar"]
        ):
            raise PolicyIntegrityError(
                f"{policy_path}: structural rule {structural_rule['rule_name']!r} in the policy "
                "file does not match the actual vstest_canonicalizer.py rule it is supposed to "
                "document"
            )

    if not body.get("applicable_case_ids"):
        raise PolicyIntegrityError(f"{policy_path}: applicable_case_ids is empty or missing")

    return body
