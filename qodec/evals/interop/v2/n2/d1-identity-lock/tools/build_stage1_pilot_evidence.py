#!/usr/bin/env python3
"""N2-D1b: builds the immutable Stage-1 (five-ecosystem pilot) acceptance
evidence record, so the accepted, fully-green pilot run is preserved as its
own durable artifact rather than getting buried under Stage 2's CI history.

Every field here is real data already retrieved from the GitHub API for the
accepted run (workflow run 29418422603, commit a68176bd1725ab46b9ff14c9694d4a622c95fe4d)
plus this session's own local test-suite run at that same commit -- nothing
here is synthesized or estimated.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

OUT_PATH = Path(__file__).resolve().parents[1] / "stage1-pilot-evidence.json"


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


JOBS = [
    {"name": "pilot-rust-capture-a", "job_id": 87362359407, "conclusion": "success"},
    {"name": "pilot-rust-capture-b", "job_id": 87362359096, "conclusion": "success"},
    {"name": "pilot-jvm-maven-capture-a", "job_id": 87362359132, "conclusion": "success"},
    {"name": "pilot-jvm-maven-capture-b", "job_id": 87362359321, "conclusion": "success"},
    {"name": "pilot-jvm-gradle-capture-a", "job_id": 87362359171, "conclusion": "success"},
    {"name": "pilot-jvm-gradle-capture-b", "job_id": 87362359117, "conclusion": "success"},
    {"name": "pilot-dotnet-capture-a", "job_id": 87362359122, "conclusion": "success"},
    {"name": "pilot-dotnet-capture-b", "job_id": 87362359159, "conclusion": "success"},
    {"name": "pilot-python-capture-a", "job_id": 87362359163, "conclusion": "success"},
    {"name": "pilot-python-capture-b", "job_id": 87362359156, "conclusion": "success"},
]

ARTIFACTS = [
    {"name": "n2d1b-pilot-repo-hyperfine-capture-a", "artifact_id": 8344135596,
     "digest_sha256": "7fe73f96947cfc7f2309a4583c00c6bea983003c37fc6979a713e833a96b5dd9"},
    {"name": "n2d1b-pilot-repo-hyperfine-capture-b", "artifact_id": 8344140102,
     "digest_sha256": "ef332eff924e6c3a36f9faf16cffe4c98a9aba5c6603a9aa90915f6fc93f815f"},
    {"name": "n2d1b-pilot-repo-docker-java-parser-capture-a", "artifact_id": 8344177926,
     "digest_sha256": "c84264321ef2082edb3d05681975fa7995c7187bb297c52dd25cab74060f047e"},
    {"name": "n2d1b-pilot-repo-docker-java-parser-capture-b", "artifact_id": 8344178714,
     "digest_sha256": "6defb6c73e2896e6ac1d620616db6e6e47018b60a8a70945a50d3710fc67d982"},
    {"name": "n2d1b-pilot-repo-spotless-capture-a", "artifact_id": 8344149634,
     "digest_sha256": "c1ed5fd0a93687ed5a20c4b9b869cee1fd1f8a123b1c07e41863de09b897e196"},
    {"name": "n2d1b-pilot-repo-spotless-capture-b", "artifact_id": 8344147629,
     "digest_sha256": "5c3f2fc05cb362c9d3e02b56fcd9f22f6b68409b24dcd4576d44ede5b032dda4"},
    {"name": "n2d1b-pilot-repo-kubeops-generator-capture-a", "artifact_id": 8344207914,
     "digest_sha256": "bcfaf556cf3319c35bbf8358ec356ac855ea240877fb60e2e961405fecf29cbe"},
    {"name": "n2d1b-pilot-repo-kubeops-generator-capture-b", "artifact_id": 8344202389,
     "digest_sha256": "81e82f43032998db3e576aca27083943e9b730c59627f3d2a6a0a498124d5f82"},
    {"name": "n2d1b-pilot-repo-pyflakes-capture-a", "artifact_id": 8344141430,
     "digest_sha256": "9020fbaf375ecea6fa33a980f85d995c05adfe0bd264753ee253d9b936c257e9"},
    {"name": "n2d1b-pilot-repo-pyflakes-capture-b", "artifact_id": 8344136971,
     "digest_sha256": "896e83eafaff27f95b7565c51e0c7c3b3ad4fdedf4a553b324c357377a6c0946"},
]


def build_record() -> dict:
    body = {
        "record_type": "n2d1b-stage1-pilot-evidence-v1",
        "status": "STAGE_1_ACCEPTED_COMPLETE",
        "pull_request": {
            "repo": "PhysShell/007",
            "number": 56,
            "state_at_acceptance": "draft, open, unmerged",
        },
        "workflow": {
            "name": "qodec-n2d1b-miner-pilot",
            "run_id": 29418422603,
            "run_number": 4,
            "run_html_url": "https://github.com/PhysShell/007/actions/runs/29418422603",
            "event": "push",
            "conclusion": "success",
        },
        "tested_head_sha": "a68176bd1725ab46b9ff14c9694d4a622c95fe4d",
        "job_count": len(JOBS),
        "jobs": sorted(JOBS, key=lambda j: j["name"]),
        "all_job_conclusions_success": all(j["conclusion"] == "success" for j in JOBS),
        "artifact_count": len(ARTIFACTS),
        "artifacts": sorted(ARTIFACTS, key=lambda a: a["name"]),
        "ecosystem_lanes_statement": (
            "All five ecosystem lanes (rust, jvm-maven, jvm-gradle, dotnet, python) "
            "passed both capture-a and capture-b in this run -- the D1b generic "
            "capture engine's core design (toolchain identity capture, generalized "
            "Sandboy policy rendering, confined execution, receipt assembly) is "
            "proven across all five ecosystems it targets."
        ),
        "local_test_suite_at_tested_head_sha": {
            "command": 'python3 -m unittest discover -s tests -p "test_*.py"',
            "working_directory": "qodec/evals/interop/v2/n2/d1-identity-lock",
            "test_count": 67,
            "result": "OK",
        },
        "bug_fix_history_leading_to_this_run": [
            {"ci_run_id": 29417042823, "found": "double case_id path nesting; source.tar never extracted before trusted setup"},
            {"ci_run_id": 29417313334, "found": "repo-pyflakes erratum-ordering stale-erratum refusal; missing dotnet ECOSYSTEM_POLICY_HINTS entry; gradle toolchain probe ran with no cwd; ANSI-colored 'mvn --version' output broke version regex"},
            {"ci_run_id": 29418081991, "found": "Gradle 9.x replaced '^JVM:' with 'Launcher JVM:'/'Daemon JVM:', breaking runtime_identifier parsing; sanitizer.sanitize's rules_applied (list of dicts) crashed set()-based dedup with TypeError"},
            {"ci_run_id": 29418422603, "found": "none -- first fully green 10/10 stage-1 run"},
        ],
        "not_yet_authorized": [
            "N2-D2", "N2-D3", "token aggregation", "leaderboard calculations",
            "case substitution", "further argv errata", "modifications to frozen N2-C evidence",
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
