#!/usr/bin/env python3
"""N2-D1b: strict, case-specific canonicalizer for repo-docker-java-parser's
raw Maven stdout.

D1b-authorized (2026-07-16, see capture-canonicalization-policy.json) after
real CI evidence (workflow run 29436883023, commit 216116af, artifacts
n2d1b-pilot-repo-docker-java-parser-capture-a/-b) showed capture-a and
capture-b's raw stdout differ in exactly five lines, all of them known,
inherently non-deterministic wall-clock/timestamp presentation fields Maven
core and its plugins print with no known config-level suppression, and
nowhere else:

  1. buildnumber-maven-plugin's "Storing buildNumber: null at timestamp: N"
  2. scala-maven-plugin's per-module "compile in N s" (can occur more than
     once per capture)
  3. surefire/JUnit's "Time elapsed: N s" inside the "Tests run: ..." summary
  4. Maven core's "Total time:  N s" build-completion line
  5. Maven core's "Finished at: <ISO-8601>" build-completion line

Deliberately NOT the general-purpose canary sanitizer (sanitizer.sanitize):
that module trims/dedupes/redacts broadly across every ecosystem's output.
This module does none of that -- it never reorders, drops, merges, or trims
a line, touches only the five lines above, and RAISES rather than silently
passing through any line that contains a rule's trigger substring but does
not conform to that rule's exact anchored grammar (a real future format
change in Maven or a plugin must be a loud failure here, never a silently
still-nondeterministic "canonical" byte). Applies only to case_id
"repo-docker-java-parser" -- see capture-canonicalization-policy.json's
applicable_case_ids for the single source of truth on scope.
"""
from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from typing import Callable

ESC = "\x1b"


class CanonicalizerError(Exception):
    pass


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


_LINE_ENDING_RE = re.compile(r"(\r\n|\r|\n)$")


def _split_line_ending(line: str) -> tuple[str, str]:
    m = _LINE_ENDING_RE.search(line)
    if m:
        return line[: m.start()], line[m.start():]
    return line, ""


@dataclass(frozen=True)
class Rule:
    name: str
    # Plain substring pre-check -- never itself the matching/replacement
    # logic. If present but `pattern` still doesn't match, that is an error,
    # not a silent skip.
    trigger: str
    # Fully anchored (^...$) against one line's content, WITHOUT its line
    # ending. Every alternative for a volatile group also accepts its own
    # placeholder text, so a second canonicalization pass matches (and is a
    # no-op) instead of erroring on already-canonicalized input.
    pattern: "re.Pattern[str]"
    placeholder: str
    apply: Callable[["re.Match[str]"], str]

    def to_policy_dict(self) -> dict:
        return {
            "rule_name": self.name,
            "trigger_substring": self.trigger,
            "anchored_regex": self.pattern.pattern,
            "placeholder": self.placeholder,
        }


# --- Rule 1: buildnumber-maven-plugin timestamp ----------------------------
_PREFIX_1 = f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: "
_PATTERN_1 = re.compile("^" + re.escape(_PREFIX_1) + r"(\d+|<TIMESTAMP>)$")
RULE_BUILDNUMBER_TIMESTAMP = Rule(
    name="buildnumber_timestamp",
    trigger="Storing buildNumber:",
    pattern=_PATTERN_1,
    placeholder="<TIMESTAMP>",
    apply=lambda m: _PREFIX_1 + "<TIMESTAMP>",
)

# --- Rule 2: scala-maven-plugin per-module compile duration ----------------
_PREFIX_2 = f"[{ESC}[1;34mINFO{ESC}[m] compile in "
_SUFFIX_2 = " s"
_PATTERN_2 = re.compile("^" + re.escape(_PREFIX_2) + r"(\d+\.\d+|<ELAPSED>)" + re.escape(_SUFFIX_2) + "$")
RULE_SCALA_COMPILE_DURATION = Rule(
    name="scala_compile_duration",
    trigger="compile in ",
    pattern=_PATTERN_2,
    placeholder="<ELAPSED>",
    apply=lambda m: _PREFIX_2 + "<ELAPSED>" + _SUFFIX_2,
)

# --- Rule 3: surefire/JUnit elapsed field inside the test summary line -----
_PREFIX_3 = f"[{ESC}[1;34mINFO{ESC}[m] {ESC}[1;32mTests run: {ESC}[0;1;32m"
_MID_3A = f"{ESC}[m, Failures: "
_MID_3B = ", Errors: "
_MID_3C = ", Skipped: "
_MID_3D = ", Time elapsed: "
_MID_3E = " s - in "
_PATTERN_3 = re.compile(
    "^" + re.escape(_PREFIX_3) + r"(\d+)" + re.escape(_MID_3A) + r"(\d+)" + re.escape(_MID_3B)
    + r"(\d+)" + re.escape(_MID_3C) + r"(\d+)" + re.escape(_MID_3D)
    + r"(\d+\.\d+|<ELAPSED>)" + re.escape(_MID_3E) + r"(.+)$"
)


def _apply_rule_3(m: "re.Match[str]") -> str:
    tests_run, failures, errors, skipped, _elapsed, suite_tail = m.groups()
    return (
        _PREFIX_3 + tests_run + _MID_3A + failures + _MID_3B + errors + _MID_3C + skipped
        + _MID_3D + "<ELAPSED>" + _MID_3E + suite_tail
    )


RULE_SUREFIRE_TIME_ELAPSED = Rule(
    name="surefire_time_elapsed",
    trigger="Time elapsed:",
    pattern=_PATTERN_3,
    placeholder="<ELAPSED>",
    apply=_apply_rule_3,
)

# --- Rule 4: Maven core total build duration -------------------------------
_PREFIX_4 = f"[{ESC}[1;34mINFO{ESC}[m] Total time:  "
_PATTERN_4 = re.compile("^" + re.escape(_PREFIX_4) + r"(\d+\.\d+ s|<ELAPSED>)$")
RULE_MAVEN_TOTAL_TIME = Rule(
    name="maven_total_time",
    trigger="Total time:",
    pattern=_PATTERN_4,
    placeholder="<ELAPSED>",
    apply=lambda m: _PREFIX_4 + "<ELAPSED>",
)

# --- Rule 5: Maven core completion timestamp -------------------------------
_PREFIX_5 = f"[{ESC}[1;34mINFO{ESC}[m] Finished at: "
_PATTERN_5 = re.compile(
    "^" + re.escape(_PREFIX_5) + r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z|<TIMESTAMP>)$"
)
RULE_MAVEN_FINISHED_AT = Rule(
    name="maven_finished_at",
    trigger="Finished at:",
    pattern=_PATTERN_5,
    placeholder="<TIMESTAMP>",
    apply=lambda m: _PREFIX_5 + "<TIMESTAMP>",
)

# Ordered -- processed top to bottom against every line. In real evidence
# each line matches at most one rule, but the loop does not assume that.
RULES: list[Rule] = [
    RULE_BUILDNUMBER_TIMESTAMP,
    RULE_SCALA_COMPILE_DURATION,
    RULE_SUREFIRE_TIME_ELAPSED,
    RULE_MAVEN_TOTAL_TIME,
    RULE_MAVEN_FINISHED_AT,
]


def canonicalize_stream(raw_bytes: bytes) -> tuple[bytes, dict]:
    """Returns (canonicalized_bytes, report). Raises CanonicalizerError if
    `raw_bytes` is not valid UTF-8, or if any line contains a rule's trigger
    substring but does not conform to that rule's anchored expected
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
        "report_type": "n2d1b-maven-canonicalization-report-v1",
        "line_count_in": len(lines),
        "line_count_out": len(out_lines),
        "trailing_newline_preserved": raw_bytes.endswith(b"\n") == canonical_bytes.endswith(b"\n"),
        "rule_match_counts": rule_match_counts,
        "replacement_count": len(replacements),
        "replacements": replacements,
    }
    return canonical_bytes, report
