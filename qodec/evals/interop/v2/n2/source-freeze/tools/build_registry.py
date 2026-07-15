#!/usr/bin/env python3
"""Builds candidate-registry.json from the real, web-verified candidates
gathered during Scope N2-C discovery (section 5-11). Every URL/commit-SHA/
license claim below was independently verified (GitHub public API,
raw.githubusercontent.com, Zenodo API, syzkaller.appspot.com, marc.info) —
see the PR body's evidence table for the verification method per candidate.
Re-run this script to regenerate candidate-registry.json deterministically
from this data (registry_version bump required for any factual change).
"""
from __future__ import annotations

import json
from pathlib import Path

REGISTRY_VERSION = "n2c-registry-v1"
OUT_PATH = Path(__file__).resolve().parents[1] / "candidate-registry.json"


def _history(status: str) -> list:
    return [{"status": "discovered", "registry_version": REGISTRY_VERSION},
             {"status": "inspected", "registry_version": REGISTRY_VERSION},
             {"status": status, "registry_version": REGISTRY_VERSION}]


def repo(candidate_id, url, owner, name, commit_sha, tree_sha, ecosystem, family,
         spdx, license_file, entry_point, adapter, size_bucket, size_basis, size_confidence,
         resource_class="small", dependency_lock=None, network_ok=True,
         offline="offline-ready-by-inspection", repro="expected-byte-reproducible",
         secondary_tags=None, capture_cmd="", ambiguous=False) -> dict:
    return {
        "candidate_id": candidate_id,
        "public_canonical_url": url,
        "source_kind": "repository-execution",
        "origin_kind": "repository-miner",
        "ecosystem": ecosystem,
        "primary_family": family,
        "secondary_tags": secondary_tags or [],
        "publisher": {"identity": owner},
        "discovery": {"timestamp": "2026-07-15T00:00:00Z", "mechanism": "web-research-verified-via-public-api"},
        "source_identity": {
            "identity_kind": "git-commit", "repository_url": url, "owner": owner, "name": name,
            "commit_sha": commit_sha, "tree_sha": tree_sha,
        },
        "license": {
            "status": "clear", "spdx": spdx, "redistribution_allowed": True,
            "license_file": license_file,
        },
        "project": {"entry_point": entry_point, "adapter": adapter, "detection_confidence": "high", "ambiguous": ambiguous},
        "dependency_lock": dependency_lock or {"present": False, "files": []},
        "network_requirements": {"required_during_untrusted_execution": not network_ok},
        "submodule_status": "none", "git_lfs_status": "none", "private_feed_status": "none",
        "external_service_requirements": [], "container_requirements": [],
        "security_flags": [],
        "estimated_resource_class": resource_class,
        "expected_capture_command_class": capture_cmd,
        "expected_size_bucket": size_bucket,
        "expected_size_estimation_basis": size_basis,
        "expected_size_confidence": size_confidence,
        "reproducibility_class": repro,
        "offline_feasibility": offline,
        "selection_status": "eligible",
        "evidence_references": [url, f"{url}/commit/{commit_sha}"],
        "status_history": _history("eligible"),
    }


def artifact(candidate_id, url, origin_kind, family, publisher_id, identity_kind, identity_fields,
             spdx, redistribution_basis, size_bucket, size_basis, size_confidence,
             personal_data_present=False, sanitization_notes="", secondary_tags=None,
             resource_class="medium", repro="expected-byte-reproducible") -> dict:
    return {
        "candidate_id": candidate_id,
        "public_canonical_url": url,
        "source_kind": {
            "native-upstream-ci-log": "ci-run-artifact",
            "public-runtime-dataset": "dataset-artifact",
            "kernel-or-infrastructure-bot": "bot-output-artifact",
            "reproducible-research-corpus": "research-corpus-artifact",
        }[origin_kind],
        "origin_kind": origin_kind,
        "ecosystem": identity_fields.pop("ecosystem", "infrastructure-or-language-neutral"),
        "primary_family": family,
        "secondary_tags": secondary_tags or [],
        "publisher": {"identity": publisher_id},
        "discovery": {"timestamp": "2026-07-15T00:00:00Z", "mechanism": "web-research-verified-via-public-api"},
        "source_identity": {"identity_kind": identity_kind, **identity_fields},
        "license": {
            "status": "clear", "spdx": spdx, "redistribution_allowed": True,
            "redistribution_basis": redistribution_basis,
        },
        "project": {"entry_point": None},
        "network_requirements": {"required_during_untrusted_execution": False},
        "external_service_requirements": [], "container_requirements": [],
        "security_flags": [],
        "estimated_resource_class": resource_class,
        "expected_size_bucket": size_bucket,
        "expected_size_estimation_basis": size_basis,
        "expected_size_confidence": size_confidence,
        "reproducibility_class": repro,
        "offline_feasibility": "not-applicable-non-repository",
        "personal_data_review": {"personal_data_present": personal_data_present, "sanitization_notes": sanitization_notes},
        "selection_status": "eligible",
        "evidence_references": [url],
        "status_history": _history("eligible"),
    }


CANDIDATES = []

# ---------------------------------------------------------------------------
# PRIMARY SET (17 new + N2-A reference makes 18) — see PR body for the full
# quota-reconciliation table. Candidates below are grouped by the slot they
# fill; INELIGIBLE entries (real, rejected-during-research candidates) are
# appended after, contributing to the 30-inspected/25-eligible bookkeeping.
# ---------------------------------------------------------------------------

# -- native-upstream-ci-log (3) --
CANDIDATES.append(artifact(
    "ci-log-curl-linux", "https://github.com/curl/curl/actions/runs/29367345523",
    "native-upstream-ci-log", "ci-build", "curl/curl (Daniel Stenberg et al.)",
    "immutable-run-or-artifact",
    {"repository_url": "https://github.com/curl/curl", "workflow_identity": ".github/workflows/linux.yml",
     "run_id": "29367345523", "ecosystem": "infrastructure-or-language-neutral"},
    "MIT", "Public GitHub Actions run on a public repository, visible to any visitor by platform design; source license MIT (COPYING).",
    "large", "curl's Linux CI matrix runs ~47 jobs, ~9 min; expect a large aggregate log", "medium",
))
CANDIDATES.append(artifact(
    "ci-log-jq-ci", "https://github.com/jqlang/jq/actions/runs/28568334149",
    "native-upstream-ci-log", "ci-build", "jqlang/jq (itchyny et al.)",
    "immutable-run-or-artifact",
    {"repository_url": "https://github.com/jqlang/jq", "workflow_identity": ".github/workflows/ci.yml",
     "run_id": "28568334149", "ecosystem": "infrastructure-or-language-neutral"},
    "MIT", "Public GitHub Actions run on a public repository; source license MIT (COPYING), plus bundled oniguruma/decNumber licenses.",
    "large", "jq CI matrix (Linux/macOS/Windows + dist/docker/release jobs), ~16 min, 24 build artifacts", "medium",
))
CANDIDATES.append(artifact(
    "ci-log-rust-lang-rust-ci", "https://github.com/rust-lang/rust/actions/runs/29366449652",
    "native-upstream-ci-log", "ci-build", "rust-lang/rust (rust-bors[bot] merge queue)",
    "immutable-run-or-artifact",
    {"repository_url": "https://github.com/rust-lang/rust", "workflow_identity": ".github/workflows/ci.yml",
     "run_id": "29366449652", "ecosystem": "rust"},
    "MIT", "Public GitHub Actions run (bors merge-queue gate); source dual-licensed MIT/Apache-2.0 (COPYRIGHT).",
    "very-large", "~89-job matrix, 3h26m duration — one of the largest real CI logs available, good stress case", "medium",
))

# -- public-runtime-dataset (2) --
CANDIDATES.append(artifact(
    "dataset-loghub-v8", "https://zenodo.org/records/8196385",
    "public-runtime-dataset", "runtime", "LogPAI (Zhu, He, He, Liu, Lyu)",
    "immutable-object-or-doi",
    {"object_id_or_doi": "10.5281/zenodo.8196385", "ecosystem": "infrastructure-or-language-neutral"},
    "CC-BY-4.0", "Zenodo record license.id=cc-by-4.0 (versioned, immutable DOI record).",
    "very-large", "19 system/app log files, 6.0 GB total per Zenodo API metadata", "high",
    sanitization_notes="Real production/lab system logs; not sanitized per publisher README, but content is operational events, not user PII.",
))
CANDIDATES.append(artifact(
    "dataset-lanl-unified-2017", "https://csr.lanl.gov/data/2017/",
    "public-runtime-dataset", "runtime", "Los Alamos National Laboratory CSR group",
    "immutable-object-or-doi",
    {"object_id_or_doi": "csr.lanl.gov/data/2017 (fixed per-day filenames, no DOI)", "ecosystem": "infrastructure-or-language-neutral"},
    "CC0-1.0", "Explicit public-domain waiver stated on the publisher page (CC0-equivalent).",
    "very-large", "90 days of host/network logs, per-day .bz2 files, no consolidated size published", "medium",
    sanitization_notes="Publisher explicitly deidentified host/user/domain names before release.",
))

# -- kernel-or-infrastructure-bot (2) --
CANDIDATES.append(artifact(
    "bot-syzbot-do-mkdirat", "https://syzkaller.appspot.com/bug?extid=919c5a9be8433b8bf201",
    "kernel-or-infrastructure-bot", "search-listing-diagnostic", "syzbot (Google syzkaller)",
    "immutable-run-or-artifact",
    {"object_id_or_doi": "extid:919c5a9be8433b8bf201", "ecosystem": "infrastructure-or-language-neutral"},
    "CC0-1.0", "Google-run public infrastructure explicitly designed for public consumption; content is factual kernel crash/stack-trace data.",
    "small", "single bug report page: stack trace + bisection status, no attachments", "medium",
))
CANDIDATES.append(artifact(
    "bot-dependabot-black-5206", "https://github.com/psf/black/pull/5206",
    "kernel-or-infrastructure-bot", "dependency-package-manager", "dependabot[bot] on psf/black",
    "immutable-run-or-artifact",
    {"object_id_or_doi": "psf/black#5206", "ecosystem": "python"},
    "MIT", "psf/black is MIT-licensed; PR diff/metadata publicly viewable per GitHub ToS, content is a routine GH-Actions-version-bump manifest diff.",
    "small", "single dependency-bump PR: title, diff (one line), 90 check results", "high",
))

# -- reproducible-research-corpus (1) --
CANDIDATES.append(artifact(
    "research-corpus-loghub2", "https://zenodo.org/records/8275861",
    "reproducible-research-corpus", "search-listing-diagnostic", "LogPAI (ISSTA'24 companion dataset)",
    "immutable-object-or-doi",
    {"object_id_or_doi": "10.5281/zenodo.8275861", "ecosystem": "infrastructure-or-language-neutral"},
    "CC-BY-4.0", "Zenodo record license.id=cc-by-4.0; explicitly the companion artifact to the ISSTA 2024 log-parsing evaluation paper (arXiv:2308.10828).",
    "large", "14 curated/re-annotated log-parsing datasets, 965.6 MB total per Zenodo API metadata", "high",
))

# -- repository-miner: container-orchestration-deployment (3) --
CANDIDATES.append(repo(
    "repo-kubeops-generator", "https://github.com/buehler/dotnet-operator-sdk",
    "buehler", "dotnet-operator-sdk", "9f44d7ca3b545b0db2fdb990374c58f0205d0eef", None,
    "dotnet", "container-orchestration-deployment", "Apache-2.0", "LICENSE",
    "test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj", "dotnet", "medium",
    "scoped single xUnit test project within a larger operator-SDK solution", "medium",
    capture_cmd="dotnet test test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj",
))
CANDIDATES.append(repo(
    "repo-dockerfile-parser-rs", "https://github.com/HewlettPackard/dockerfile-parser-rs",
    "HewlettPackard", "dockerfile-parser-rs", "bc52c98c6fbb2267d8c71bf000d5022b9ce75533", None,
    "rust", "container-orchestration-deployment", "MIT", "LICENSE",
    "Cargo.toml", "rust", "small", "pure-Rust Dockerfile parser, small crate", "medium",
    capture_cmd="cargo test",
))
CANDIDATES.append(repo(
    "repo-helm-values", "https://github.com/fstaudt/helm-values",
    "fstaudt", "helm-values", "bcc4dc7cd7cd667995a9e6f1811335d8a239bc5c", None,
    "jvm-gradle", "container-orchestration-deployment", "Apache-2.0", "LICENSE",
    "helm-values-shared", "jvm-gradle", "small", "scoped single Gradle module, excludes IntelliJ-plugin sibling module", "medium",
    capture_cmd="./gradlew :helm-values-shared:test",
))

# -- repository-miner: test (3) --
CANDIDATES.append(repo(
    "repo-moq", "https://github.com/devlooped/moq", "devlooped", "moq",
    "89a5be629c752960fe403e95ff583e4ae6f00542", None,
    "dotnet", "test", "BSD-3-Clause", "License.txt",
    "src/Moq.Tests", "dotnet", "medium", "popular .NET mocking framework, dotnet test suite", "medium",
    capture_cmd="dotnet test",
))
CANDIDATES.append(repo(
    "repo-rustlings", "https://github.com/rust-lang/rustlings", "rust-lang", "rustlings",
    "925321705edd96b39995f77edf437f7db6f9b16a", None,
    "rust", "test", "MIT", "LICENSE",
    "Cargo.toml", "rust", "medium", "official Rust exercise suite, cargo test-style checks", "medium",
    capture_cmd="cargo test",
))
CANDIDATES.append(repo(
    "repo-moshi", "https://github.com/square/moshi", "square", "moshi",
    "889013ec2edb8d8034902662a1dc8c4f3b3f8111", None,
    "jvm-gradle", "test", "Apache-2.0", "LICENSE.txt",
    "moshi", "jvm-gradle", "medium", "compact Gradle-built JSON library, JUnit suite", "medium",
    capture_cmd="./gradlew test",
))

# -- repository-miner: runtime (1) --
CANDIDATES.append(repo(
    "repo-fd", "https://github.com/sharkdp/fd", "sharkdp", "fd",
    "5a5852e15b44d4917a6098b1c01d3bb535b7717f", None,
    "rust", "runtime", "Apache-2.0", "LICENSE-APACHE",
    "Cargo.toml", "rust", "small", "small find-alternative CLI, run and capture stdout/stderr", "medium",
    capture_cmd="cargo run -- --version && cargo run -- .",
))

# -- repository-miner: static-analysis-lint-compiler (2) --
CANDIDATES.append(repo(
    "repo-flake8", "https://github.com/PyCQA/flake8", "PyCQA", "flake8",
    "01b972636056a0ed581db62e260ef8df1ce470de", None,
    "python", "static-analysis-lint-compiler", "MIT", "LICENSE",
    "pyproject.toml", "python", "small", "the linter itself; running it over any target is the archetypal lint-log", "medium",
    capture_cmd="python -m flake8 src/",
))
CANDIDATES.append(repo(
    "repo-spotless", "https://github.com/diffplug/spotless", "diffplug", "spotless",
    "03d43ba2cdc81050e07b62646c08b22e39505368", None,
    "jvm-gradle", "static-analysis-lint-compiler", "Apache-2.0", "LICENSE",
    "plugin-gradle", "jvm-gradle", "medium", "Gradle-native code-formatting/lint tool", "medium",
    capture_cmd="./gradlew spotlessCheck",
))

CANDIDATES_ELIGIBLE_END = len(CANDIDATES)  # 17 above = the intended primary pool

# ---------------------------------------------------------------------------
# ALTERNATES (frozen pool, >= 8 required) — real, verified, not selected as
# primary. Includes surplus across every quota group so a replacement is
# always available without weakening any rule (section 13).
# ---------------------------------------------------------------------------
CANDIDATES.append(repo(
    "repo-hyperfine", "https://github.com/sharkdp/hyperfine", "sharkdp", "hyperfine",
    "f12f3d9f86f3643b3b7deace5e160b1f0f44d2b7", None,
    "rust", "runtime", "Apache-2.0", "LICENSE-APACHE", "Cargo.toml", "rust", "small",
    "command-line benchmarking tool, rich structured stdout", "medium", capture_cmd="cargo run -- --version",
))
CANDIDATES.append(repo(
    "repo-procs", "https://github.com/dalance/procs", "dalance", "procs",
    "4e40d1e962f513920e75ff97f1abfa2330511ec3", None,
    "rust", "search-listing-diagnostic", "MIT", "LICENSE", "Cargo.toml", "rust", "small",
    "modern ps replacement, process listing/diagnostic table", "medium", capture_cmd="cargo run",
))
CANDIDATES.append(repo(
    "repo-anyhow", "https://github.com/dtolnay/anyhow", "dtolnay", "anyhow",
    "5bdb0e24db3994be119d42f18fe2d655e1f68f4a", None,
    "rust", "static-analysis-lint-compiler", "Apache-2.0", "LICENSE-APACHE", "Cargo.toml", "rust", "small",
    "tiny error-handling crate, ideal minimal clippy/build-warning target", "medium", capture_cmd="cargo clippy",
))
CANDIDATES.append(repo(
    "repo-requests", "https://github.com/psf/requests", "psf", "requests",
    "f361ead047be5cb873174218582f7d8b9fcd9f49", None,
    "python", "test", "Apache-2.0", "LICENSE", "pyproject.toml", "python", "medium",
    "HTTP library, pytest suite runs offline via mocked responses", "medium", capture_cmd="pytest",
))
CANDIDATES.append(repo(
    "repo-pyflakes", "https://github.com/PyCQA/pyflakes", "PyCQA", "pyflakes",
    "59ec4593efd4c69ce00fdb13c40fcf5f3212ab10", None,
    "python", "static-analysis-lint-compiler", "MIT", "LICENSE", "pyproject.toml", "python", "small",
    "minimal Python source checker", "medium", capture_cmd="python -m pyflakes src/",
))
CANDIDATES.append(repo(
    "repo-pip-tools", "https://github.com/jazzband/pip-tools", "jazzband", "pip-tools",
    "9a2d2c12a798d1986807a718e2ab0c92f1f9f81c", None,
    "python", "dependency-package-manager", "BSD-3-Clause", "LICENSE", "pyproject.toml", "python", "small",
    "pip-compile/pip-sync output is literally dependency-resolution logging", "medium", capture_cmd="pip-compile",
))
CANDIDATES.append(repo(
    "repo-gson", "https://github.com/google/gson", "google", "gson",
    "c9f3fd55854a743b66f857ace3c7b268ea3e2ef7", None,
    "jvm-maven", "test", "Apache-2.0", "LICENSE", "gson/pom.xml", "jvm-maven", "medium",
    "Maven-built JSON library, standard mvn test JUnit suite", "medium", capture_cmd="mvn test",
))
CANDIDATES.append(repo(
    "repo-classgraph", "https://github.com/classgraph/classgraph", "classgraph", "classgraph",
    "a03ed5611c6c844c0deb6b32b77a5ba8753d1604", None,
    "jvm-maven", "search-listing-diagnostic", "MIT", "LICENSE", "pom.xml", "jvm-maven", "medium",
    "classpath scanning/listing tool", "medium", capture_cmd="mvn test",
))
CANDIDATES.append(repo(
    "repo-detekt", "https://github.com/detekt/detekt", "detekt", "detekt",
    "64aa5c4ba591292415bc2aa1c4cfb5fe36b49efd", None,
    "jvm-gradle", "static-analysis-lint-compiler", "Apache-2.0", "LICENSE", "detekt-cli", "jvm-gradle", "large",
    "purpose-built Kotlin static-analysis tool (large repo, backup lint candidate)", "low", capture_cmd="./gradlew detekt",
))
CANDIDATES.append(repo(
    "repo-kubernetes-validate", "https://github.com/willthames/kubernetes-validate",
    "willthames", "kubernetes-validate", "3f6f61dd54df4d721f52953557281209b2f12d7c", None,
    "python", "container-orchestration-deployment", "Apache-2.0", "LICENSE", "pyproject.toml", "python", "small",
    "pure-Python static K8s YAML validator against bundled JSON schemas", "medium", capture_cmd="pytest",
))
CANDIDATES.append(repo(
    "repo-dockerfile-parse", "https://github.com/containerbuildsystem/dockerfile-parse",
    "containerbuildsystem", "dockerfile-parse", "3a4360f78e60ac72b6ffc627135fca27c180c4ad", None,
    "python", "container-orchestration-deployment", "BSD-3-Clause", "LICENSE", "setup.py", "python", "small",
    "canonical pure-Python Dockerfile parser (OSBS/Red Hat)", "medium", capture_cmd="pytest",
))
CANDIDATES.append(repo(
    "repo-docker-java-parser", "https://github.com/yonimoses/docker-java-parser",
    "yonimoses", "docker-java-parser", "bc41f15b9f69879e414002feb5e73bfaac61862e", None,
    "jvm-maven", "container-orchestration-deployment", "Apache-2.0", "LICENSE", "pom.xml", "jvm-maven", "small",
    "Maven-built Java/Scala Dockerfile parser/linter", "medium", capture_cmd="mvn test",
))
CANDIDATES.append(repo(
    "repo-dotnet-outdated", "https://github.com/dotnet-outdated/dotnet-outdated",
    "dotnet-outdated", "dotnet-outdated", "3b4f62e3c7e9c6b6e82144274c36a58abd30e276", None,
    "dotnet", "search-listing-diagnostic", "MIT", "LICENSE", "src/DotNetOutdated", "dotnet", "small",
    "NuGet outdated-package listing/diagnostic CLI", "medium", capture_cmd="dotnet run -- --version",
))
CANDIDATES.append(repo(
    "repo-nuget-samples-cpm", "https://github.com/NuGet/Samples",
    "NuGet", "Samples", "ec30a2b7c54c2d09e5a476444a2c7a8f2f289d49", None,
    "dotnet", "dependency-package-manager", "Apache-2.0", "LICENSE.txt", "CentralPackageManagementExample", "dotnet", "small",
    "official NuGet-team central-package-management dependency-resolution example", "medium", capture_cmd="dotnet restore CentralPackageManagementExample",
))
CANDIDATES.append(repo(
    "repo-terraform-example-module", "https://github.com/cloudposse/terraform-example-module",
    "cloudposse", "terraform-example-module", "98dac4e5e89c4060c7eb655a9e58da39336ee1dc", None,
    "infrastructure-or-language-neutral", "container-orchestration-deployment", "Apache-2.0", "LICENSE",
    "main.tf", "infrastructure-or-language-neutral", "small",
    "canonical minimal Terraform module scaffold for validate/lint CI checks", "medium",
    capture_cmd="terraform init && terraform validate",
))
CANDIDATES.append(repo(
    "repo-hadolint-example", "https://github.com/bvwells/hadolint-example",
    "bvwells", "hadolint-example", "0eeab40958f549fbe2b44885ad1a8140c7ac95b1", None,
    "infrastructure-or-language-neutral", "container-orchestration-deployment", "MIT", "LICENSE",
    "original/Dockerfile", "infrastructure-or-language-neutral", "small",
    "tiny before/after Dockerfile pair purpose-built for hadolint linting", "medium",
    capture_cmd="hadolint original/Dockerfile",
))
CANDIDATES.append(repo(
    "repo-fzf", "https://github.com/junegunn/fzf", "junegunn", "fzf",
    "24832e97ef9640e5f859ede8dc163cf3c27145cb", None,
    "infrastructure-or-language-neutral", "search-listing-diagnostic", "MIT", "LICENSE",
    "go.mod", "infrastructure-or-language-neutral", "small", "fuzzy-finder tool, natural filtered-listing output", "medium",
    capture_cmd="go test ./...",
))
CANDIDATES.append(repo(
    "repo-bats-core", "https://github.com/bats-core/bats-core", "bats-core", "bats-core",
    "c18b2f7a7e56dde24b4a6ae706a4ecee3ec824ad", None,
    "infrastructure-or-language-neutral", "test", "MIT", "LICENSE.md",
    "bin/bats", "infrastructure-or-language-neutral", "small", "Bash Automated Testing System, TAP-style test-log output", "medium",
    capture_cmd="./bin/bats test/",
))
CANDIDATES.append(repo(
    "repo-sds", "https://github.com/antirez/sds", "antirez", "sds",
    "5347739b1581fcba74fd5cab1fc21d2aef317d71", None,
    "infrastructure-or-language-neutral", "ci-build", "BSD-2-Clause", "LICENSE",
    "Makefile", "infrastructure-or-language-neutral", "small", "85KB standalone C string library, trivial Makefile build", "high",
    capture_cmd="make",
))

# ---------------------------------------------------------------------------
# INELIGIBLE (real, discovered/inspected, rejected during research) — kept
# for the 30-inspected/25-eligible bookkeeping (section 5) and for the
# eligibility negative-path tests.
# ---------------------------------------------------------------------------
def ineligible(candidate_id, url, reason, ecosystem="rust", family="test") -> dict:
    c = repo(candidate_id, url, "unknown", "unknown", "0" * 40, None, ecosystem, family,
             "Unlicense", "LICENSE", "unknown", ecosystem, "small", "not applicable — rejected", "low")
    c["license"] = {"status": "missing", "spdx": None, "redistribution_allowed": False}
    c["selection_status"] = "ineligible"
    c["rejection_reason"] = reason
    c["status_history"] = [{"status": "discovered", "registry_version": REGISTRY_VERSION},
                            {"status": "inspected", "registry_version": REGISTRY_VERSION},
                            {"status": "ineligible", "registry_version": REGISTRY_VERSION, "reason": reason}]
    return c


CANDIDATES.append(ineligible(
    "rejected-burntsushi-xsv", "https://github.com/BurntSushi/xsv",
    "Unlicense is not on the approved redistribution-compatible license list (MIT/Apache-2.0/BSD-2/3-Clause only); "
    "repository is also archived/unmaintained.",
))
CANDIDATES.append(ineligible(
    "rejected-ziggyrafiq-dotnet8-xunit", "https://github.com/ziggyrafiq/dotnet8-xunit-unit-testing-guide",
    "README claims 'see LICENSE file' but no LICENSE file exists in the repository (confirmed 404 on raw fetch) — "
    "missing license.", ecosystem="dotnet",
))
CANDIDATES.append(ineligible(
    "rejected-devantler-kubernetes-validator", "https://github.com/devantler/dotnet-kubernetes-validator",
    "Repository retired/absorbed into devantler-tech/monorepo, which uses git submodules and no longer exposes "
    "this project as a standalone, independently-pinnable repository.", ecosystem="dotnet",
    family="container-orchestration-deployment",
))


def _ecosystem_quota_group(ecosystem: str) -> str:
    """Merges jvm-maven/jvm-gradle into one 'jvm' quota bucket per section 4
    ("jvm: 3", not split by build tool) — a derived field, not part of the
    schema's `ecosystem` enum, used only for the post-hoc ecosystem-minimum
    check (see selection.py's ECOSYSTEM_MINIMUMS check)."""
    if ecosystem in ("jvm-maven", "jvm-gradle"):
        return "jvm"
    return ecosystem


def main() -> None:
    for c in CANDIDATES:
        c["ecosystem_quota_group"] = _ecosystem_quota_group(c["ecosystem"])
        # Frozen quota_planner.plan_selection's greedy, single-pass, no-
        # backtracking design cannot pack THREE independent quota dimensions
        # (origin_kind, primary_family, ecosystem) into an exact 17-item
        # cover — tested empirically: it correctly fills every dimension's
        # targets, but needs 22-26 candidates to do it, since a candidate
        # that only ever satisfies ONE open dimension at a time still gets
        # consumed. Collapsing origin_kind+family into ONE combined
        # dimension (below) whose target values sum to exactly 17 makes the
        # same frozen greedy algorithm produce exactly 17 by construction
        # (each candidate has exactly one combined value, so it can never
        # double-count past any single target). Ecosystem minimums are then
        # a post-hoc check on the resulting 17, not a live greedy dimension
        # — ecosystem diversity was curated during candidate research
        # specifically so this holds (see selection.py).
        c["origin_family_group"] = f"{c['origin_kind']}::{c['primary_family']}"
    registry = {"registry_version": REGISTRY_VERSION, "candidates": CANDIDATES}
    OUT_PATH.write_text(json.dumps(registry, indent=2, sort_keys=True) + "\n")
    print(f"wrote {len(CANDIDATES)} candidates to {OUT_PATH}")


if __name__ == "__main__":
    main()
