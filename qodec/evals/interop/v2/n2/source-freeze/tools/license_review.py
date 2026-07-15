#!/usr/bin/env python3
"""N2-C license and redistribution review (section 11).

Builds a license-record.json for a primary/alternate case from the
candidate's registry entry, validates it against the schema, and applies the
section-11 hard-reject rules. This is deliberately independent of
eligibility.py's license check (which only looks at status/spdx) — this
module is the authoritative, fuller record with attribution/modification/
redistribution-basis text, meant for human review evidence, not just a
boolean gate.
"""
from __future__ import annotations

import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SOURCE_FREEZE_DIR.parent.parent / "corpus" / "tools"))
import jsonschema_mini  # noqa: E402

SCHEMA_PATH = SOURCE_FREEZE_DIR / "schemas" / "license-record.schema.json"

HARD_REJECT_CONDITIONS = [
    "missing_license",
    "unclear_redistribution_basis",
    "non_commercial_restriction",
    "no_derivatives_restriction",
    "private_or_confidential_source",
    "personal_data_heavy_dataset",
    "credentials_or_secrets_present",
]

_NON_COMMERCIAL_MARKERS = ("noncommercial", "non-commercial", "nc-", "-nc", "cc-by-nc")
_NO_DERIVATIVES_MARKERS = ("noderivatives", "no-derivatives", "-nd", "nd-", "cc-by-nd")


def build_license_record(candidate: dict, review_evidence: list[str]) -> dict:
    license_ = candidate.get("license", {})
    return {
        "candidate_id": candidate["candidate_id"],
        "license_status": license_.get("status", "missing"),
        "spdx": license_.get("spdx"),
        "publisher_stated_license": license_.get("publisher_stated_license", license_.get("spdx")),
        "license_source_url": license_.get("license_source_url", candidate.get("public_canonical_url", "")),
        "license_text_sha256": license_.get("license_text_sha256"),
        "source_code_license": license_.get("source_code_license", license_.get("spdx")),
        "log_or_data_redistribution_basis": license_.get("log_or_data_redistribution_basis"),
        "redistribution_basis": license_.get("redistribution_basis", ""),
        "attribution_requirements": license_.get("attribution_requirements", ""),
        "modification_requirements": license_.get("modification_requirements", ""),
        "redistribution_allowed": license_.get("redistribution_allowed", "unclear"),
        "review_evidence": review_evidence,
    }


def _load_schema() -> dict:
    import json
    return json.loads(SCHEMA_PATH.read_text())


def validate_license_record(record: dict) -> list[str]:
    return jsonschema_mini.validate(record, _load_schema())


def hard_reject_reasons(record: dict) -> list[str]:
    """Returns the list of section-11 hard-reject conditions this record
    triggers (empty list means the record passes review)."""
    reasons = []
    if record.get("license_status") == "missing" or not record.get("spdx"):
        reasons.append("missing_license")
    if record.get("license_status") == "ambiguous" or record.get("redistribution_allowed") == "unclear":
        reasons.append("unclear_redistribution_basis")
    spdx_low = (record.get("spdx") or "").lower()
    if any(marker in spdx_low for marker in _NON_COMMERCIAL_MARKERS):
        reasons.append("non_commercial_restriction")
    if any(marker in spdx_low for marker in _NO_DERIVATIVES_MARKERS):
        reasons.append("no_derivatives_restriction")
    if record.get("redistribution_allowed") is False:
        reasons.append("unclear_redistribution_basis")
    return reasons


if __name__ == "__main__":
    import json

    example = {
        "license": {
            "status": "clear", "spdx": "MIT",
            "redistribution_allowed": True,
            "redistribution_basis": "MIT license grants redistribution",
            "attribution_requirements": "Retain copyright notice",
            "modification_requirements": "None beyond attribution",
        },
        "candidate_id": "example-candidate",
        "public_canonical_url": "https://github.com/example/example",
    }
    record = build_license_record(example, ["https://github.com/example/example/blob/main/LICENSE"])
    print(json.dumps(record, indent=2))
    print("errors:", validate_license_record(record), file=sys.stderr)
    print("hard_reject:", hard_reject_reasons(record), file=sys.stderr)
