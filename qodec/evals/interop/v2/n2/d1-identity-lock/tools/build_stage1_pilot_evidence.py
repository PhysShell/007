#!/usr/bin/env python3
"""N2-D1b: builds the immutable Stage-1 (five-ecosystem pilot) acceptance
evidence record, so the accepted, fully-green pilot run is preserved as its
own durable artifact rather than getting buried under Stage 2's CI history.

Rebuilt (2026-07-16) following the user's formal Stage-1 sign-off, on the
new accepted implementation head after repo-spotless's rejection and
repo-moshi's substitution, and after the full chain of real-evidence fixes
this required (network exception, deterministic scheduling, canonicalization
policies). Every field here is real data retrieved from the GitHub API for
the accepted run (workflow run 29474805883, commit
c51eacca7edd9b73f58c740f5de31998304cf85c) plus this session's own local
test-suite run at that same commit -- nothing here is synthesized or
estimated. Supersedes the prior record (run 29418422603, commit a68176bd...,
which included the since-rejected repo-spotless case).
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

OUT_PATH = Path(__file__).resolve().parents[1] / "stage1-pilot-evidence.json"

APPROVING_DECISION_IDENTITY = "n2d1b-stage1-acceptance-formal-signoff-2026-07-16"
SUPERSEDED_RECORD_WORKFLOW_RUN_ID = 29418422603
SUPERSEDED_RECORD_TESTED_HEAD_SHA = "a68176bd1725ab46b9ff14c9694d4a622c95fe4d"


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


PILOT_JOBS = [
    {"name": "pilot-repo-hyperfine-capture-a", "job_id": 87545435781, "conclusion": "success"},
    {"name": "pilot-repo-hyperfine-capture-b", "job_id": 87545435767, "conclusion": "success"},
    {"name": "pilot-repo-docker-java-parser-capture-a", "job_id": 87545435746, "conclusion": "success"},
    {"name": "pilot-repo-docker-java-parser-capture-b", "job_id": 87545435787, "conclusion": "success"},
    {"name": "pilot-repo-kubeops-generator-capture-a", "job_id": 87545435758, "conclusion": "success"},
    {"name": "pilot-repo-kubeops-generator-capture-b", "job_id": 87545435766, "conclusion": "success"},
    {"name": "pilot-repo-pyflakes-capture-a", "job_id": 87545435783, "conclusion": "success"},
    {"name": "pilot-repo-pyflakes-capture-b", "job_id": 87545435812, "conclusion": "success"},
    {"name": "pilot-repo-moshi-capture-a", "job_id": 87545435807, "conclusion": "success"},
    {"name": "pilot-repo-moshi-capture-b", "job_id": 87545435810, "conclusion": "success"},
]

PAIR_VERIFY_JOBS = [
    {"name": "pair-verify-repo-hyperfine", "job_id": 87546321351, "conclusion": "success"},
    {"name": "pair-verify-repo-docker-java-parser", "job_id": 87546321377, "conclusion": "success"},
    {"name": "pair-verify-repo-kubeops-generator", "job_id": 87546321346, "conclusion": "success"},
    {"name": "pair-verify-repo-pyflakes", "job_id": 87546321315, "conclusion": "success"},
    {"name": "pair-verify-repo-moshi", "job_id": 87546321342, "conclusion": "success"},
]

ARTIFACTS = [
    {"name": "n2d1b-pilot-repo-hyperfine-capture-a", "artifact_id": 8366139532,
     "digest_sha256": "514ee058130fb418fef0650c2c3bf1f053d06550098effaddb11f2898b7422e8"},
    {"name": "n2d1b-pilot-repo-hyperfine-capture-b", "artifact_id": 8366141731,
     "digest_sha256": "e31980cfef4b50abc026ab97ae2778513c2f3812d339e20e35b130c9476108bd"},
    {"name": "n2d1b-pilot-repo-docker-java-parser-capture-a", "artifact_id": 8366180501,
     "digest_sha256": "cb739a8667118f847fa2af715f9fb3429ca7c9460457e278e4f47ba97dc4dbdc"},
    {"name": "n2d1b-pilot-repo-docker-java-parser-capture-b", "artifact_id": 8366168264,
     "digest_sha256": "e0fa530fc93fc8a3e29c72979e33b93fdb57434d73272e4c5e11ca6472568cc5"},
    {"name": "n2d1b-pilot-repo-kubeops-generator-capture-a", "artifact_id": 8366144829,
     "digest_sha256": "b2f1e08a6e68bbb4d3b2f4af3c90c1975a760b64dd678ffa89e74bb869f602f6"},
    {"name": "n2d1b-pilot-repo-kubeops-generator-capture-b", "artifact_id": 8366144792,
     "digest_sha256": "e754eeb9c5eedf24030d8bc1af0019df289420f3b88c93364dc0a80dcc376a31"},
    {"name": "n2d1b-pilot-repo-pyflakes-capture-a", "artifact_id": 8366142354,
     "digest_sha256": "2e5093a0b319b15546e09aa39ec54c122846a8622bd1b705b61959b85c397e9c"},
    {"name": "n2d1b-pilot-repo-pyflakes-capture-b", "artifact_id": 8366141344,
     "digest_sha256": "cc5071b4e5286beefca89251ca89bbb19d6b1d241419dd62c1945959eca3ef62"},
    {"name": "n2d1b-pilot-repo-moshi-capture-a", "artifact_id": 8366228037,
     "digest_sha256": "29b7d7b0cef908e8f799b9c1b5100890b565d1747ecb316add22d3be0fb0bf77"},
    {"name": "n2d1b-pilot-repo-moshi-capture-b", "artifact_id": 8366234779,
     "digest_sha256": "9bd12e04a08edf00a1772579ee577ab82dc29e7073b63aa933d862ff0762278c"},
]

PAIR_REPORT_ARTIFACTS = [
    {"name": "n2d1b-pair-reproducibility-repo-hyperfine", "artifact_id": 8366237288,
     "digest_sha256": "d9153a4d949a73f61d63007245dd0ddfc6abd487f5a533d69db0c3e2d6cd8f66"},
    {"name": "n2d1b-pair-reproducibility-repo-docker-java-parser", "artifact_id": 8366237868,
     "digest_sha256": "4d80fae459a0d124ed0a2df3e53b03aa66e76db2c348303482a27301da22a030"},
    {"name": "n2d1b-pair-reproducibility-repo-kubeops-generator", "artifact_id": 8366237711,
     "digest_sha256": "32c3bb895bee68d7a49dea7b6018e450afadce220b93715cfbe27cca76fe6238"},
    {"name": "n2d1b-pair-reproducibility-repo-pyflakes", "artifact_id": 8366237450,
     "digest_sha256": "a297cde6737ac72005eff5b734ea37129f9774de20adeb227d2e757c012bc8d5"},
    {"name": "n2d1b-pair-reproducibility-repo-moshi", "artifact_id": 8366238367,
     "digest_sha256": "f94772da4bdabd764200ee93bc9971a0721fc3b7ed6e9e7a691a72f3e0c8b4ae"},
]

# Real values read directly from the committed, self-hash-locked policy/
# record files at this same commit -- never hand-copied from memory.
CANONICALIZATION_POLICIES = {
    "maven": {
        "file": "capture-canonicalization-policy.json",
        "policy_sha256": "d633497d6b2e5575bdaeb183ad10b140e2900df98d8299caa57772fd4d8b495c",
        "policy_version": 1,
        "applicable_case_ids": ["repo-docker-java-parser"],
        "canonicalizer_module": "maven_canonicalizer.py",
    },
    "vstest": {
        "file": "vstest-capture-canonicalization-policy.json",
        "policy_sha256": "c6728ad1447dc9ab328bee526f60fb33b29d3346f0db8d3b617ef4352db7df59",
        "policy_version": 2,
        "applicable_case_ids": ["repo-kubeops-generator"],
        "canonicalizer_module": "vstest_canonicalizer.py",
        "rules": ["vstest_duration"],
        "structural_rules": ["msbuild_completion_pair_order (bounded permutation of the two "
                              "named MSBuild project-completion lines only)"],
    },
    "gradle": {
        "file": "gradle-capture-canonicalization-policy.json",
        "policy_sha256": "c968245e3837e2155873a8c8a3623bad9b2522ef163ee79cfbf2461eb8ef3b7c",
        "policy_version": 1,
        "applicable_case_ids": ["repo-moshi"],
        "canonicalizer_module": "gradle_canonicalizer.py",
        "rules": ["gradle_build_duration"],
    },
}

REPO_SPOTLESS_REJECTION = {
    "file": "repo-spotless-rejection-record.json",
    "record_sha256": "f6849f50d97566346d0f1eca55a6efd91ab0d7362964f9d8b4ba00ad4ccb288c",
    "classification": "REJECTED_ACQUISITION_MODEL_INCOMPATIBLE",
    "excluded_from_pilot_numerator_and_denominator": True,
}

NETWORK_ENFORCEMENT_AUTHORIZED_CASES = {
    "repo-kubeops-generator": "outer-netns-loopback-only",
    "repo-moshi": "outer-netns-loopback-only",
}

# Real value read from repo-moshi's own committed receipt.json
# (gradle_scheduling_profile.profile_sha256) at this commit -- the profile
# text itself (org.gradle.daemon=false, org.gradle.parallel=false,
# org.gradle.workers.max=1, org.gradle.console=plain,
# org.gradle.console.interactive=false) is what this hash locks.
MOSHI_DETERMINISTIC_SCHEDULING_PROFILE_SHA256 = "68c4b4cccc5bb7c7d8862cf09195538a1d9d62e9b0e6229cad2e8b69d8d81aa2"


def build_record() -> dict:
    body = {
        "record_type": "n2d1b-stage1-pilot-evidence-v1",
        "record_version": 2,
        "status": "STAGE_1_ACCEPTED_COMPLETE",
        "approving_decision_identity": APPROVING_DECISION_IDENTITY,
        "superseded_record": {
            "workflow_run_id": SUPERSEDED_RECORD_WORKFLOW_RUN_ID,
            "tested_head_sha": SUPERSEDED_RECORD_TESTED_HEAD_SHA,
            "reason": (
                "Included repo-spotless, since rejected as "
                "REJECTED_ACQUISITION_MODEL_INCOMPATIBLE and replaced by repo-moshi; "
                "predates the network-exception, deterministic-scheduling, and "
                "canonicalization-policy work this record now reflects."
            ),
        },
        "pull_request": {
            "repo": "PhysShell/007",
            "number": 56,
            "state_at_acceptance": "draft, open, unmerged",
        },
        "workflow": {
            "name": "qodec-n2d1b-miner-pilot",
            "run_id": 29474805883,
            "run_number": 25,
            "run_html_url": "https://github.com/PhysShell/007/actions/runs/29474805883",
            "event": "push",
            "conclusion": "success",
        },
        "tested_head_sha": "c51eacca7edd9b73f58c740f5de31998304cf85c",
        "accepted_pilot_case_ids": [
            "repo-hyperfine", "repo-docker-java-parser", "repo-kubeops-generator",
            "repo-pyflakes", "repo-moshi",
        ],
        "job_count": len(PILOT_JOBS),
        "jobs": sorted(PILOT_JOBS, key=lambda j: j["name"]),
        "all_job_conclusions_success": all(j["conclusion"] == "success" for j in PILOT_JOBS),
        "pair_verify_job_count": len(PAIR_VERIFY_JOBS),
        "pair_verify_jobs": sorted(PAIR_VERIFY_JOBS, key=lambda j: j["name"]),
        "all_pair_verify_job_conclusions_success": all(j["conclusion"] == "success" for j in PAIR_VERIFY_JOBS),
        "artifact_count": len(ARTIFACTS),
        "artifacts": sorted(ARTIFACTS, key=lambda a: a["name"]),
        "pair_report_artifact_count": len(PAIR_REPORT_ARTIFACTS),
        "pair_report_artifacts": sorted(PAIR_REPORT_ARTIFACTS, key=lambda a: a["name"]),
        "repo_spotless_rejection": REPO_SPOTLESS_REJECTION,
        "canonicalization_policies": CANONICALIZATION_POLICIES,
        "network_enforcement_authorized_cases": NETWORK_ENFORCEMENT_AUTHORIZED_CASES,
        "moshi_deterministic_scheduling_profile_sha256": MOSHI_DETERMINISTIC_SCHEDULING_PROFILE_SHA256,
        "ecosystem_lanes_statement": (
            "All five ecosystem lanes (rust, jvm-maven, dotnet, python, jvm-gradle) "
            "passed both capture-a and capture-b in this run -- the D1b generic "
            "capture engine's core design (toolchain identity capture, generalized "
            "Sandboy policy rendering, confined execution, receipt assembly, and, "
            "for two cases, case-scoped network-enforcement exceptions and "
            "canonicalization profiles) is proven across all five ecosystems it "
            "targets, using genuine end-to-end workload completion, not merely "
            "receipt schema validity or job conclusion."
        ),
        "local_test_suite_at_tested_head_sha": {
            "command": 'python3 -m unittest discover -s tests -p "test_*.py"',
            "working_directory": "qodec/evals/interop/v2/n2/d1-identity-lock",
            "test_count": 302,
            "result": "OK",
        },
        "bug_fix_history_leading_to_this_run": [
            {"ci_run_id": 29417042823, "found": "double case_id path nesting; source.tar never extracted before trusted setup"},
            {"ci_run_id": 29417313334, "found": "repo-pyflakes erratum-ordering stale-erratum refusal; missing dotnet ECOSYSTEM_POLICY_HINTS entry; gradle toolchain probe ran with no cwd; ANSI-colored 'mvn --version' output broke version regex"},
            {"ci_run_id": 29418081991, "found": "Gradle 9.x replaced '^JVM:' with 'Launcher JVM:'/'Daemon JVM:', breaking runtime_identifier parsing; sanitizer.sanitize's rules_applied (list of dicts) crashed set()-based dedup with TypeError"},
            {"ci_run_id": 29418422603, "found": "none -- first fully green 10/10 stage-1 run (repo-spotless included, since rejected)"},
            {"ci_run_id": "run6 (revoked)", "found": "all 18 captures were infrastructure/sandbox failures, not genuine workload output, on real byte-level inspection -- fail-closed content-acceptance gate added"},
            {"ci_run_id": 29465040390, "found": "KubeOps VSTest test-host hit loopback-bind Permission denied identical to Gradle's daemon finding"},
            {"ci_run_id": 29466573023, "found": "repo-spotless Gradle plugin-portal network gap fixed at the sandbox level, but the case itself later found to require real git history (ratchetFrom) upstream, unconditionally"},
            {"ci_run_id": 29467180079, "found": "repo-spotless equo/p2 ~/.m2 permission gap and configuration-cache-report root-build gap fixed; git-ratchet requirement confirmed persistent -- case rejected (REJECTED_ACQUISITION_MODEL_INCOMPATIBLE), replaced by repo-moshi"},
            {"ci_run_id": 29469116485, "found": "repo-moshi hit the identical Gradle-daemon loopback-bind class as repo-spotless -- separately authorized (repo-spotless's own entry revoked, not inherited)"},
            {"ci_run_id": 29469560893, "found": "repo-moshi com.vanniktech.maven.publish plugin-resolution gap (fixed via trusted priming); KubeOps MSBuild project-completion-line ordering nondeterminism found (confirmed intermittent)"},
            {"ci_run_id": 29470199739, "found": "repo-moshi Kotlin compiler daemon /tmp lock-file and .kotlin/sessions/ gaps fixed; genuine BUILD SUCCESSFUL pair still failed on Gradle's own parallel task-log interleaving"},
            {"ci_run_id": 29474204715, "found": "repo-moshi deterministic scheduling profile (org.gradle.parallel=false, workers.max=1) made every task line byte-identical and same-order; sole remaining diff was the build-completion wall-clock duration"},
            {"ci_run_id": 29474805883, "found": "none -- fully green 10/10 captures + 5/5 pair-verify Stage-1 run with repo-moshi in the jvm-gradle slot"},
        ],
        "not_yet_authorized": [
            "official Stage-2 leaderboard calculations pending full Stage-2 corpus, RTK identity, and Nix identity",
            "further argv errata beyond what is already authorized",
            "modifications to frozen N2-C evidence",
        ],
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
