#!/usr/bin/env python3
"""N2-D1b: builds the immutable revocation record for the Stage-1 pilot
acceptance claim. Real capture-content inspection (see
build_capture_content_audit.py / capture-content-audit-run6.json) showed
every one of the 18 captures behind the Stage-1/Stage-2 "accepted" claims
is content-invalid -- infrastructure/sandbox failures, not genuine
workload output. stage1-pilot-evidence.json is NOT deleted or edited; this
record supersedes its ACCEPTANCE STATUS only, and states plainly what it
still is valid for.

This record is itself a frozen historical snapshot: `revoked_records`
below pins the exact self-hash of the specific stage1-pilot-evidence.json
version that was revoked at the time (run 29418422603, repo-spotless-era),
by literal constant -- not by re-reading whatever the file says today.
stage1-pilot-evidence.json has since been legitimately rebuilt from
scratch and formally re-accepted by the user (2026-07-16 sign-off) after
satisfying every criterion this record's own `path_to_re_acceptance`
demanded (fail-closed content-acceptance gate; from-scratch re-run;
byte-level content validation). `superseded_by` records that
re-acceptance for Stage 1 ONLY -- Stage 2's revocation remains in effect
until the full nine-case matrix is itself re-run and re-accepted.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

OUT_PATH = Path(__file__).resolve().parents[1] / "stage1-and-stage2-acceptance-revocation.json"

# Frozen historical identity of the specific stage1-pilot-evidence.json
# version this record revoked -- literal, not re-derived, so this record's
# meaning does not silently drift if the live file is later rebuilt.
STAGE1_REVOKED_VERSION_RECORD_SHA256 = "eb1b1e385b53bc6714ad3bd45eda5da1037b4b6a9bd18a9c4ec9ccc797653261"
STAGE1_REVOKED_VERSION_STATUS = "STAGE_1_ACCEPTED_COMPLETE"

# Frozen identity of the record that formally re-accepted Stage 1, lifting
# this revocation for Stage 1 only.
STAGE1_REACCEPTANCE_DECISION_IDENTITY = "n2d1b-stage1-acceptance-formal-signoff-2026-07-16"
STAGE1_REACCEPTANCE_WORKFLOW_RUN_ID = 29474805883
STAGE1_REACCEPTANCE_TESTED_HEAD_SHA = "c51eacca7edd9b73f58c740f5de31998304cf85c"
STAGE1_REACCEPTANCE_RECORD_SHA256 = "ad71afd35e1af0668277e494c6594040fef21f44b55dc11564450437c72c345e"


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


def build_record() -> dict:
    body = {
        "record_type": "n2d1b-acceptance-revocation-v1",
        "revoked_records": [
            {
                "path": "qodec/evals/interop/v2/n2/d1-identity-lock/stage1-pilot-evidence.json",
                "record_sha256": STAGE1_REVOKED_VERSION_RECORD_SHA256,
                "previously_claimed_status": STAGE1_REVOKED_VERSION_STATUS,
            },
        ],
        "superseded_by": {
            "scope": "stage_1_five_ecosystem_pilot only -- stage_2_full_nine_case_matrix revocation remains in effect",
            "approving_decision_identity": STAGE1_REACCEPTANCE_DECISION_IDENTITY,
            "workflow_run_id": STAGE1_REACCEPTANCE_WORKFLOW_RUN_ID,
            "tested_head_sha": STAGE1_REACCEPTANCE_TESTED_HEAD_SHA,
            "new_stage1_evidence_record_sha256": STAGE1_REACCEPTANCE_RECORD_SHA256,
            "re_acceptance_criteria_satisfied": (
                "Fail-closed content-acceptance gate added to generic_capture.py; "
                "five-ecosystem pilot re-run from scratch with no reuse of runs "
                "#4-#6 raw input; every capture inspected and confirmed content-"
                "valid at the actual byte level; independent pair-reproducibility "
                "verification for all five cases; case-scoped network-enforcement "
                "and canonicalization exceptions each separately authorized and "
                "evidence-gated."
            ),
        },
        "revocation_reason": (
            "Real inspection of the actual captured bytes (see "
            "capture-content-audit-run6.json, built by downloading and reading "
            "all 18 capture-a/capture-b artifacts from run #6, the run treated "
            "as Stage-2 -- and by extension Stage-1 -- acceptance evidence) "
            "showed every capture is content-invalid: rustup default-toolchain "
            "resolution failures (rust), /dev/null permission denials (jvm-maven, "
            "jvm-gradle), Python venv pyvenv.cfg permission denials (python), and "
            "NuGet restore failures under network denial with no prior trusted "
            "restore step (dotnet). Every prior CI 'success' conclusion (runs "
            "#1 through #6) validated workflow plumbing, receipt schema "
            "compliance, and artifact upload integrity ONLY -- none of it "
            "validated that the captured content was genuine workload output."
        ),
        "effective_status_changes": {
            "stage_1_five_ecosystem_pilot": "NOT ACCEPTED (previously: STAGE_1_ACCEPTED_COMPLETE -- revoked)",
            "stage_2_full_nine_case_matrix": "NOT ACCEPTED (was never formally recorded as an acceptance artifact; this makes that explicit)",
        },
        "what_remains_valid": (
            "stage1-pilot-evidence.json and CI runs #1-#6 remain valid evidence "
            "of CI-PLUMBING correctness only: workflow YAML syntax and matrix "
            "wiring, receipt JSON-schema compliance, self-hash-locked artifact "
            "identity, and artifact upload/download integrity (every recomputed "
            "hash in capture-content-audit-run6.json matched the receipts' own "
            "recorded hashes). None of the 18 underlying raw captures from any "
            "of runs #1-#6 are valid benchmark input, and none are eligible for "
            "RTK probing or any other benchmark use."
        ),
        "not_yet_authorized_pending_re_acceptance": [
            "N2-D1b.3 (RTK filter applicability inventory + determinism probes)",
            "N2-D1b.4 (canonical Nix build identity)",
            "N2-D2", "N2-D3", "token aggregation", "leaderboard calculations",
        ],
        "path_to_re_acceptance": (
            "A fail-closed content-acceptance gate must be added to the capture "
            "engine (generic_capture.py) that rejects known infrastructure-"
            "failure signatures and empty/invalid canonical streams before a "
            "capture job may report success. The five-ecosystem pilot must then "
            "be re-run from scratch (no reuse of runs #4-#6 raw input) and pass "
            "content-level acceptance, inspected at the actual byte level -- not "
            "only receipt schema and CI job conclusion -- before a new Stage-1 "
            "acceptance record may be built. The full 9-case matrix must then "
            "pass twice before a new Stage-2 acceptance record may be built."
        ),
    }
    _, digest = canonicalize_and_hash(body)
    body["record_sha256"] = digest
    return body


def main() -> int:
    body = build_record()
    without_hash = {k: v for k, v in body.items() if k != "record_sha256"}
    _, recomputed = canonicalize_and_hash(without_hash)
    assert recomputed == body["record_sha256"], "self-hash did not verify stable"
    OUT_PATH.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n")
    print(f"wrote {OUT_PATH} (record_sha256={body['record_sha256']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
