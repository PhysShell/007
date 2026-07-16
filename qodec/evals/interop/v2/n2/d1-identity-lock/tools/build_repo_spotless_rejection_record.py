#!/usr/bin/env python3
"""Builds the immutable, self-hash-locked repo-spotless-rejection-record.json.

D1b decision (2026-07-16): repo-spotless is REJECTED_ACQUISITION_MODEL_INCOMPATIBLE,
not a sandbox/tooling defect. Evidence (CI runs 29466993434, 29467180079,
29467952842 -- the last AFTER the equo/p2 ~/.m2 fs_ro fix and the root
"build" writable-dir fix had already resolved two earlier, genuinely
sandbox-side gaps): the confined capture's frozen `./gradlew spotlessCheck`
still fails with "Cannot find git repository in any parent directory"
because repo-spotless's own upstream build.gradle unconditionally applies
`ratchetFrom 'origin/main'` (gradle/spotless.gradle) to every non-root
subproject's spotlessJava task -- a hard, unconditional dependency on real
git history reachable from an origin/main ref, which the frozen
tarball-snapshot acquisition model (source.tar with no .git directory)
structurally cannot provide, and which upstream exposes no property/CLI
override for (its own only demonstrated disable mechanism is a source-level
`ratchetFrom null` edit, e.g. gradle/spotless-freshmark.gradle:21).

repo-spotless is replaced by the already-frozen repo-moshi case (also an
N2-C primary case, same jvm-gradle ecosystem) in the five-ecosystem pilot.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

OUT_PATH = Path(__file__).resolve().parents[1] / "repo-spotless-rejection-record.json"

APPROVING_DECISION_IDENTITY = "n2d1b-repo-spotless-rejection-2026-07-16"

OBSERVED_FAILURE_RAW_STDERR = (
    "sandboy: warning: Landlock only PARTIALLY enforced (kernel too old for some access rights)\n"
    "[Resource-Usage] Failed to initialize JNA. Instantiated JNA is version '5.16.0'. "
    "Please update to 5.15.0 or later.\n"
    "\n"
    "FAILURE: Build failed with an exception.\n"
    "\n"
    "* What went wrong:\n"
    "Could not determine the dependencies of task ':lib:spotlessJavaCheck'.\n"
    "> Could not create task ':lib:spotlessJava'.\n"
    "   > Cannot find git repository in any parent directory\n"
    "\n"
    "* Try:\n"
    "> Run with --stacktrace option to get the stack trace.\n"
    "> Run with --info or --debug option to get more log output.\n"
    "> Get more help at https://help.gradle.org.\n"
    "\n"
    "BUILD FAILED in 11s\n"
    "Command exited with non-zero status 1\n"
)

PROHIBITED_WORKAROUNDS = [
    "patching upstream build scripts (build.gradle, gradle/spotless.gradle, or any other upstream file)",
    "adding ratchetFrom null or any other upstream source-level override",
    "synthesizing a .git directory, commit, or ref in source_root",
    "pointing an origin/main ref at HEAD or any other fabricated git history",
    "cloning or fetching the upstream repository during a D1b confined capture",
    "loosening content_acceptance.py's semantic-marker requirement for this or any other case",
    "classifying this failure as a Sandboy/sandbox-policy defect",
]

REPLACEMENT_SELECTION_PROCEDURE = (
    "For the five-ecosystem pilot: repo-moshi (already a frozen N2-C primary "
    "case, same jvm-gradle ecosystem, candidate_id 'repo-moshi' in "
    "candidate-registry.json, commit 889013ec2edb8d8034902662a1dc8c4f3b3f8111) "
    "takes repo-spotless's pilot slot -- selected because it was already "
    "frozen and eligible, not hand-picked for this substitution. For the "
    "final nine-case repository-miner set: use the pre-existing "
    "deterministic candidate-selection policy (N2-B's eligibility.py/"
    "scorer.py/quota_planner.py, applied to candidate-registry.json's "
    "jvm-gradle-ecosystem candidates) to select the next eligible Gradle "
    "alternate -- never a hand-picked or QODEC/RTK/token-count/output-size "
    "re-ranked substitute."
)


def build_record() -> dict:
    body = {
        "record_type": "n2d1b-repository-case-rejection-record-v1",
        "record_version": 1,
        "case_id": "repo-spotless",
        "classification": "REJECTED_ACQUISITION_MODEL_INCOMPATIBLE",
        "approving_decision_identity": APPROVING_DECISION_IDENTITY,
        "source_identity": {
            "canonical_url": "https://github.com/diffplug/spotless",
            "commit_sha": "03d43ba2cdc81050e07b62646c08b22e39505368",
            "source_manifest": (
                "qodec/evals/interop/v2/n2/source-freeze/source-manifests/primary/repo-spotless.json"
            ),
        },
        "frozen_execution_argv": ["./gradlew", "spotlessCheck"],
        "acquisition_artifact_identity": {
            "acquisition_model": "durable N2-C release-asset tarball (source.tar), no .git metadata",
            "note": (
                "Per-CI-run durable identity (archive_sha256, actual_head_sha) is populated at capture "
                "time from the N2-D0 durable release asset following the existing acquisition-receipt.json "
                "pattern shared by every repository-miner case -- the STATIC, frozen identity anchor is "
                "source_identity.commit_sha above, which is what this rejection is scoped against."
            ),
        },
        "upstream_evidence": {
            "root_build_gradle": {
                "path": "build.gradle",
                "lines": "21-24",
                "excerpt": (
                    "allprojects {\n"
                    "\tapply from: rootProject.file('gradle/error-prone.gradle')\n"
                    "\tapply from: rootProject.file('gradle/spotless.gradle')\n"
                    "}"
                ),
            },
            "spotless_convention_script": {
                "path": "gradle/spotless.gradle",
                "line": 6,
                "excerpt": "ratchetFrom 'origin/main'",
            },
            "only_known_disable_mechanism": {
                "path": "gradle/spotless-freshmark.gradle",
                "line": 21,
                "excerpt": "ratchetFrom null",
                "note": "A source-level edit disabling ratchet for one specific format target only -- not a property or CLI override.",
            },
        },
        "observed_failure": {
            "workflow_run_ids": [29466993434, 29467180079, 29467952842],
            "raw_stderr": OBSERVED_FAILURE_RAW_STDERR,
            "raw_stderr_sha256": hashlib.sha256(OBSERVED_FAILURE_RAW_STDERR.encode()).hexdigest(),
        },
        "rationale": (
            "repo-spotless's own upstream build unconditionally requires real git history reachable "
            "from an origin/main ref (via ratchetFrom) for every non-root subproject's spotlessJava "
            "task, with no config/property/CLI override. The frozen acquisition model for this D1b "
            "effort is a content-addressed tarball snapshot with no .git directory -- these two things "
            "are structurally incompatible, independent of network policy, plugin caching, Gradle "
            "daemon behavior, or filesystem confinement (all of which were separately diagnosed and "
            "fixed in this same investigation, and are NOT the cause of this failure)."
        ),
        "prohibited_workarounds": PROHIBITED_WORKAROUNDS,
        "replacement_selection_procedure": REPLACEMENT_SELECTION_PROCEDURE,
        "replacement_case_id": "repo-moshi",
    }
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    body["record_sha256"] = hashlib.sha256(text.encode()).hexdigest()
    return body


def main() -> int:
    body = build_record()
    without_hash = {k: v for k, v in body.items() if k != "record_sha256"}
    recomputed = hashlib.sha256((json.dumps(without_hash, indent=2, sort_keys=True) + "\n").encode()).hexdigest()
    assert recomputed == body["record_sha256"], "self-hash did not verify stable"
    OUT_PATH.write_text(json.dumps(body, indent=2, sort_keys=True) + "\n")
    print(f"wrote {OUT_PATH} (record_sha256={body['record_sha256']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
