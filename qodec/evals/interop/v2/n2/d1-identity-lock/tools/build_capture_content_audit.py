#!/usr/bin/env python3
"""N2-D1b: builds the immutable content-audit record for CI run #6
(29419856899, commit 9fdd704) -- the run previously treated as Stage-2
acceptance evidence.

Every entry here was produced by actually downloading all 18 capture
artifacts, extracting receipt.json/raw.stdout/raw.stderr/canonical-raw-
input.bin, and verifying the receipt's own recorded hashes against the real
file bytes (all matched). Real inspection of the captured content showed
every one of the 18 captures is content-invalid: infrastructure/sandbox
failures, not real workload output. This record preserves that finding --
it does NOT delete or alter the run #6 artifacts themselves.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

OUT_PATH = Path(__file__).resolve().parents[1] / "capture-content-audit-run6.json"


def canonicalize_and_hash(body: dict) -> tuple[str, str]:
    text = json.dumps(body, indent=2, sort_keys=True) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


# Every field below was computed by downloading all 18 real artifacts from
# run #6 and reading receipt.json/raw.stdout/raw.stderr/canonical-raw-
# input.bin directly -- see audit_method in build_record() for exactly how.
CAPTURE_ENTRIES = [
    {"case_id": "repo-docker-java-parser", "capture_id": "capture-a", "artifact_id": 8344776881,
     "artifact_digest_sha256": "221b3d0318719aa5218202a69b0fbd2796ee3e75636b2a67f7e7dc31931d3e0e",
     "receipt_sha256": "b13a17582bcb5492240fdf2cde9a55ece7a5ca0bb43f07841329f0d1c5c8e700",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "6a51d9be23768c212d955453b95cdebd5e1a9b925ca668402ee2ffa937cfeb75", "raw_stderr_byte_size": 439,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "Caused by: java.lang.ClassNotFoundException: org.codehaus.plexus.classworlds.launcher.Launcher",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
    {"case_id": "repo-docker-java-parser", "capture_id": "capture-b", "artifact_id": 8344776352,
     "artifact_digest_sha256": "4e165428ea1176a5bee697e0505d7010ece97cd637079358a65e14bdfc6d8ea4",
     "receipt_sha256": "25075d0f82e0cdcb9b73ee8fdfbf4730776caf13eeb1c72a66105a670afdb4a7",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "6a51d9be23768c212d955453b95cdebd5e1a9b925ca668402ee2ffa937cfeb75", "raw_stderr_byte_size": 439,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "Caused by: java.lang.ClassNotFoundException: org.codehaus.plexus.classworlds.launcher.Launcher",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
    {"case_id": "repo-dockerfile-parser-rs", "capture_id": "capture-a", "artifact_id": 8344736321,
     "artifact_digest_sha256": "7ad700e850182bbc0ad11e007f6aa8a0cff58ae2c5389869ffd759678827eced",
     "receipt_sha256": "4992e17af252d601387ef34b8cdfe46a1d920f65eaffe29e608cbf203a9064ff",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-dockerfile-parser-rs", "capture_id": "capture-b", "artifact_id": 8344741291,
     "artifact_digest_sha256": "e2e97a95c05bc8f8746cb44e329ab085c1fe3a5796e9cbc637b65e364f2269f1",
     "receipt_sha256": "7dfd3ab4b6bf9c864b437510f49bd981d091f49751580ac48f81a7e8e44880e0",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-hyperfine", "capture_id": "capture-a", "artifact_id": 8344734536,
     "artifact_digest_sha256": "fcff8f2ee8f69b1c97aa176b4b048878e6792ded9c5fbc28592ad28f959a1673",
     "receipt_sha256": "d9f22e520c9cbdb39e3eaea0327897a0aa8a63ec3dfb5d41b7591825ed30d127",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-hyperfine", "capture_id": "capture-b", "artifact_id": 8344738978,
     "artifact_digest_sha256": "9deef45760539911e790e47e77e6891f6811b638b4b29fd35f81af8541949ad6",
     "receipt_sha256": "9ad2cc965f100daba1f023a25c84a6300dbb283b3c8a8e2d4f382162ed3831ef",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-kubeops-generator", "capture_id": "capture-a", "artifact_id": 8344807368,
     "artifact_digest_sha256": "fa49a5b501b06015e1c283c36e4b7e7ff8088bad8acf8e89bf1abc7c021fa139",
     "receipt_sha256": "0271f2b24313a33de9dbe0bcaf648494bab6b1494a8a8e9231df17d7834b877a",
     "raw_stdout_sha256": "621a430147666f9147f1bc1b2729b9ba5e44fb21aa2769a73c15c4400bdc74ea", "raw_stdout_byte_size": 27438,
     "raw_stderr_sha256": "dbfbc12985b0c3b931e343ba91f38965dc6a61dfdbc7dc27c932716f7049d135", "raw_stderr_byte_size": 227,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "621a430147666f9147f1bc1b2729b9ba5e44fb21aa2769a73c15c4400bdc74ea", "canonical_input_byte_size": 27438,
     "detected_infrastructure_failure": "An issue was encountered verifying workloads. For more information, run \"dotnet workload update\".",
     "content_validity": "INVALID_NON_WORKLOAD_OUTPUT", "root_cause_category": "dotnet-trusted-restore-missing-nuget-restore-attempted-under-network-denial"},
    {"case_id": "repo-kubeops-generator", "capture_id": "capture-b", "artifact_id": 8344810468,
     "artifact_digest_sha256": "3d32280637ac99e02323e8911d23cb74b848b08f3f5a1ee500da729a8b195e7d",
     "receipt_sha256": "bc8be8a759958b04a133847e290e6dd71bc12298ae91ec0c8d85c4d3f46cdf8d",
     "raw_stdout_sha256": "bc15e94a6a5f6d373a0d43204c5d584cceeb94c19363b48a58a1bf5d93bc1b13", "raw_stdout_byte_size": 27443,
     "raw_stderr_sha256": "dbfbc12985b0c3b931e343ba91f38965dc6a61dfdbc7dc27c932716f7049d135", "raw_stderr_byte_size": 227,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "bc15e94a6a5f6d373a0d43204c5d584cceeb94c19363b48a58a1bf5d93bc1b13", "canonical_input_byte_size": 27443,
     "detected_infrastructure_failure": "An issue was encountered verifying workloads. For more information, run \"dotnet workload update\".",
     "content_validity": "INVALID_NON_WORKLOAD_OUTPUT", "root_cause_category": "dotnet-trusted-restore-missing-nuget-restore-attempted-under-network-denial"},
    {"case_id": "repo-moshi", "capture_id": "capture-a", "artifact_id": 8344770952,
     "artifact_digest_sha256": "9ea248c47058f5c5692e448410ab701b862383f1d3ad7fb00dbe2819b8263e07",
     "receipt_sha256": "a0a31d0dc71d531c6c4871f7567478aa30588dff837c9515e8569cd7462804cf",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "317c3000f3052d0c02996a1175c396c4cf7e9c43b0a33c2de8d52b20263c035b", "raw_stderr_byte_size": 187,
     "recorded_exit_code": 2, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "./gradlew: 89: cannot create /dev/null: Permission denied",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
    {"case_id": "repo-moshi", "capture_id": "capture-b", "artifact_id": 8344747055,
     "artifact_digest_sha256": "7e26f66e6cf1c9948e7cdb34eae5cd7a5a906ec453acf8fa0f226329b559a0f2",
     "receipt_sha256": "c0fc003cda6de4eeb5ccde49fc65cce904bd286f943e43eb25a4041dfb4ffbec",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "317c3000f3052d0c02996a1175c396c4cf7e9c43b0a33c2de8d52b20263c035b", "raw_stderr_byte_size": 187,
     "recorded_exit_code": 2, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "./gradlew: 89: cannot create /dev/null: Permission denied",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
    {"case_id": "repo-pyflakes", "capture_id": "capture-a", "artifact_id": 8344742377,
     "artifact_digest_sha256": "91396af6c99f7e8ec1e0f5837f33c378c70f388d1aad73405e22471beb0dba22",
     "receipt_sha256": "2da104f7f00a282e8cfc3076676bfef0d10d696e3f5b4077918ab1be1f06379a",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "289db6e03a3c4353894650d12a2a192b29085e37a349e84663fd6a5955a1c695", "raw_stderr_byte_size": 783,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "PermissionError: [Errno 13] Permission denied: '/home/runner/work/_temp/venv-repo-pyflakes/pyvenv.cfg'",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "python-venv-root-not-in-sandbox-policy"},
    {"case_id": "repo-pyflakes", "capture_id": "capture-b", "artifact_id": 8344738344,
     "artifact_digest_sha256": "84caab227b6be76f81f3e54ebb1b397076c3b333b140c8030ecfe95544315851",
     "receipt_sha256": "72f2cb72cc0c64a46726a23e8af8bacfef4a063dcd2d313fc7a4c6e284dff8a9",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "289db6e03a3c4353894650d12a2a192b29085e37a349e84663fd6a5955a1c695", "raw_stderr_byte_size": 783,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "PermissionError: [Errno 13] Permission denied: '/home/runner/work/_temp/venv-repo-pyflakes/pyvenv.cfg'",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "python-venv-root-not-in-sandbox-policy"},
    {"case_id": "repo-requests", "capture_id": "capture-a", "artifact_id": 8344741403,
     "artifact_digest_sha256": "bc9be7715461b7aafbae6d3864d423f7dff538754a045200d0d06ebd49013b41",
     "receipt_sha256": "c9307c2d9e7f5cde103a494eeb56728c4fca1987030ef1e67857d0d5b9009f13",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "46cb70f779479fdc80e699c0d2c775be611b81b867f5f1e3c0f621452efeecdc", "raw_stderr_byte_size": 783,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "PermissionError: [Errno 13] Permission denied: '/home/runner/work/_temp/venv-repo-requests/pyvenv.cfg'",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "python-venv-root-not-in-sandbox-policy"},
    {"case_id": "repo-requests", "capture_id": "capture-b", "artifact_id": 8344743765,
     "artifact_digest_sha256": "6b4b4269df72130a1e74151dda911c05089188d439838f3241d3d35bf7b0939a",
     "receipt_sha256": "cdfd456db5a0c2e43029ffee7c4450bc68450da4e6151f2d693c91ee1b8c5473",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "46cb70f779479fdc80e699c0d2c775be611b81b867f5f1e3c0f621452efeecdc", "raw_stderr_byte_size": 783,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "PermissionError: [Errno 13] Permission denied: '/home/runner/work/_temp/venv-repo-requests/pyvenv.cfg'",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "python-venv-root-not-in-sandbox-policy"},
    {"case_id": "repo-rustlings", "capture_id": "capture-a", "artifact_id": 8344739595,
     "artifact_digest_sha256": "35a8d3d8af7e621fbcb44e7de2125c83c6cdaecbd4d96e2b6e00a72ce564685b",
     "receipt_sha256": "a176d1ac7dac4a60a4d7475407e942e80716b3789e237d69ea7d73289ce85437",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-rustlings", "capture_id": "capture-b", "artifact_id": 8344740069,
     "artifact_digest_sha256": "a09374f2d04cd227ff8580b38130083825e3f947aa1c93c99462cbbaa4c4f4a5",
     "receipt_sha256": "cacc889ddd8622e5694021ac669a29c3ad947f5c9fe83fe2da7ba1bfd59394a6",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "abf1c42d17889a4d7f2c6a78f6edbd39ea01bae1fe901eaf0a006e21b3e4e995", "raw_stderr_byte_size": 376,
     "recorded_exit_code": 1, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "rustup-default-toolchain-unresolved-in-sandbox"},
    {"case_id": "repo-spotless", "capture_id": "capture-a", "artifact_id": 8344747518,
     "artifact_digest_sha256": "b4c60676835769f118c7337251a7469ae68083926050179d5e1451393aaa57a2",
     "receipt_sha256": "ce0d71632f749f421916e96529f307b20cdddd9fef48b1ac14dc90c992554094",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "317c3000f3052d0c02996a1175c396c4cf7e9c43b0a33c2de8d52b20263c035b", "raw_stderr_byte_size": 187,
     "recorded_exit_code": 2, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "./gradlew: 89: cannot create /dev/null: Permission denied",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
    {"case_id": "repo-spotless", "capture_id": "capture-b", "artifact_id": 8344742112,
     "artifact_digest_sha256": "d280186fd0732f7059db8f7fd73fa43fa67a7aefa73d3b6d4a3021a2a7c29410",
     "receipt_sha256": "2d2274e1f32da8ec87aa1c5d6adbfce36cb5936a6db4319c34de60c3e8d1e22a",
     "raw_stdout_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "raw_stdout_byte_size": 0,
     "raw_stderr_sha256": "317c3000f3052d0c02996a1175c396c4cf7e9c43b0a33c2de8d52b20263c035b", "raw_stderr_byte_size": 187,
     "recorded_exit_code": 2, "selected_canonical_stream": "stdout",
     "canonical_input_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "canonical_input_byte_size": 0,
     "detected_infrastructure_failure": "./gradlew: 89: cannot create /dev/null: Permission denied",
     "content_validity": "INVALID_EMPTY_INFRASTRUCTURE_FAILURE", "root_cause_category": "dev-null-missing-from-sandbox-policy"},
]


def load_capture_entries() -> list[dict]:
    return list(CAPTURE_ENTRIES)


def build_record() -> dict:
    entries = load_capture_entries()
    body = {
        "record_type": "n2d1b-capture-content-audit-v1",
        "audited_workflow": {
            "name": "qodec-n2d1b-miner-pilot",
            "run_id": 29419856899,
            "run_number": 6,
            "head_sha": "9fdd70456b4910836b21a2df644ea8f76cd890d0",
            "run_html_url": "https://github.com/PhysShell/007/actions/runs/29419856899",
        },
        "audit_method": (
            "Downloaded all 18 capture-a/capture-b artifacts for run #6 via the "
            "GitHub Actions API's own artifact digest + presigned download URL, "
            "extracted receipt.json/raw.stdout/raw.stderr/canonical-raw-input.bin "
            "from each, and recomputed stdout/stderr/canonical-input SHA256 "
            "directly from the real bytes -- every recomputed hash matched the "
            "receipt's own recorded value (the receipts are NOT lying about their "
            "own bytes). The finding is that the captured BYTES THEMSELVES are "
            "invalid benchmark content, not that the receipts are internally "
            "inconsistent."
        ),
        "capture_count": len(entries),
        "captures": entries,
        "content_valid_count": sum(1 for e in entries if e["content_validity"] == "VALID"),
        "content_invalid_count": sum(1 for e in entries if e["content_validity"] != "VALID"),
        "root_cause_summary": {
            "rustup-default-toolchain-unresolved-in-sandbox": sorted({
                e["case_id"] for e in entries if e["root_cause_category"] == "rustup-default-toolchain-unresolved-in-sandbox"
            }),
            "dev-null-missing-from-sandbox-policy": sorted({
                e["case_id"] for e in entries if e["root_cause_category"] == "dev-null-missing-from-sandbox-policy"
            }),
            "python-venv-root-not-in-sandbox-policy": sorted({
                e["case_id"] for e in entries if e["root_cause_category"] == "python-venv-root-not-in-sandbox-policy"
            }),
            "dotnet-trusted-restore-missing-nuget-restore-attempted-under-network-denial": sorted({
                e["case_id"] for e in entries
                if e["root_cause_category"] == "dotnet-trusted-restore-missing-nuget-restore-attempted-under-network-denial"
            }),
        },
        "conclusion": (
            "All 18 captures in run #6 are content-invalid. Every prior CI 'success' "
            "conclusion (runs #1-#6) validated workflow plumbing, receipt schema "
            "compliance, and artifact upload integrity only -- none of it validated "
            "that the captured bytes were genuine workload output. No case's raw "
            "input from these runs is eligible for RTK probing or benchmark use."
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
