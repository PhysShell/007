#!/usr/bin/env python3
"""CLI entry points that render the N2-B CI artifacts (section 21) from the
framework's own modules. Kept as one script (subcommands) rather than one
file per artifact, since every subcommand is a thin, testable wrapper around
functions already exercised directly by tests/.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
TOOLS_DIR = MINER_DIR / "tools"
sys.path.insert(0, str(TOOLS_DIR))

import registry  # noqa: E402
import eligibility  # noqa: E402
import scorer  # noqa: E402
import quota_planner  # noqa: E402
import n2a_compatibility  # noqa: E402
import sandbox_planner  # noqa: E402
from adapters import ADAPTERS, base  # noqa: E402

BASE_COMMIT = "d7fd03fdc6fcbf731de81d538ab0f7bca512a607"
FROZEN_PATHS = [
    "qodec/evals/interop/v2/coverage-matrix.json",
    "qodec/evals/interop/v2/benchmark-contract.json",
    "qodec/evals/interop/v2/heldout-policy.md",
    "qodec/evals/interop/v2/rtk-comparison-contract.json",
    "qodec/evals/interop/v2/schemas",
    "qodec/evals/interop/results",
    "qodec/src",
    "flake.lock",
    "qodec/evals/interop/v2/corpus",
    "qodec/evals/interop/v2/pilot",
    "qodec/evals/interop/v2/n2/canary",
    ".github/workflows/qodec-n2-miner-canary.yml",
]

_FIXTURE_TO_ECOSYSTEM = {
    "dotnet_simple": "dotnet", "dotnet_ambiguous": "dotnet", "dotnet_packageref": "dotnet",
    "dotnet_custom_msbuild": "dotnet", "no_license": "dotnet", "with_submodule": "dotnet",
    "with_lfs_pointer": "dotnet", "docker_socket_required": "dotnet", "private_feed_required": "dotnet",
    "rust_workspace": "rust", "rust_buildrs": "rust",
    "python_pytest_lock": "python", "python_no_lock": "python",
    "maven_multimodule": "jvm-maven",
    "gradle_wrapper_custom_repo": "jvm-gradle",
}


def _git(repo_root: Path, *args):
    return subprocess.run(["git", "-C", str(repo_root), *args], capture_output=True, text=True)


def cmd_frozen_base(args):
    repo_root = Path(args.repo_root)
    drift = [p for p in FROZEN_PATHS if _git(repo_root, "diff", "--quiet", BASE_COMMIT, "--", p).returncode != 0]
    sandboy_pin_ok = sandbox_planner.ACCEPTED_SANDBOY_COMMIT_SHA == "e925058ddea405b5821fc0aed4882c76650dcbe9"
    report = {
        "base_commit": BASE_COMMIT,
        "frozen_paths_checked": FROZEN_PATHS,
        "drift": drift,
        "sandboy_pin_unchanged": sandboy_pin_ok,
        "pass": not drift and sandboy_pin_ok,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["pass"]:
        print(f"::error::frozen-base check failed: {report}", file=sys.stderr)
        return 1
    return 0


def cmd_adapter_capabilities(args):
    per_ecosystem = {}
    for ecosystem, adapter in ADAPTERS.items():
        fixture_results = {}
        for fixture_name, fixture_eco in _FIXTURE_TO_ECOSYSTEM.items():
            if fixture_eco != ecosystem:
                continue
            fixture_path = MINER_DIR / "fixtures" / fixture_name
            fixture_results[fixture_name] = adapter.detect(fixture_path)
        per_ecosystem[ecosystem] = {
            "contract_functions_implemented": [fn for fn in base.REQUIRED_ADAPTER_FUNCTIONS if hasattr(adapter, fn)],
            "toolchain_identity_contract": adapter.toolchain_identity_contract(),
            "environment_allowlist": adapter.environment_allowlist({}),
            "sanitizer_profile": adapter.sanitizer_profile(),
            "fixture_detections": fixture_results,
        }
    report = {"registered_ecosystems": sorted(ADAPTERS), "per_ecosystem": per_ecosystem}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


def cmd_selection_report(args):
    reg = registry.load_registry(MINER_DIR / "candidate-registry.example.json")
    schema_errors = registry.validate_registry(reg)
    if schema_errors:
        print(f"::error::example registry failed validation: {schema_errors}", file=sys.stderr)
        return 1
    elig_reports = eligibility.evaluate_registry(reg)
    elig_candidates = registry.eligible_candidates(reg)
    ranked = scorer.rank_candidates(elig_candidates)
    by_id = {c["candidate_id"]: c for c in elig_candidates}
    quotas = {"ecosystem": {"dotnet": 1, "rust": 1, "python": 1, "jvm-maven": 1, "jvm-gradle": 1}}
    plan = quota_planner.plan_selection(ranked, by_id, quotas)
    report = {
        "note": "No real N2 source selection was performed. This report is built only from the "
                "synthetic/example candidate registry plus the frozen N2-A reference entry.",
        "registry_version": reg["registry_version"],
        "eligibility_reports": elig_reports,
        "ranking": ranked,
        "quota_plan": plan,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


def cmd_n2a_compatibility(args):
    report = n2a_compatibility.compute_compatibility_report()
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["zero_unexplained_incompatibilities"]:
        print(f"::error::N2-A compatibility gate failed: {report['incompatible_fields']}", file=sys.stderr)
        return 1
    return 0


def cmd_sandbox_planning(args):
    manifests = {
        "dotnet": {"project": {"entry_point": "Foo/Foo.csproj"}},
        "rust": {"project": {"entry_point": "Cargo.toml"}},
        "python": {"project": {"entry_point": "pyproject.toml"}},
        "jvm-maven": {"project": {"entry_point": "pom.xml"}},
        "jvm-gradle": {"project": {"entry_point": "build.gradle.kts"}},
    }
    per_ecosystem = {}
    capability_gaps_found = []
    for ecosystem, manifest in manifests.items():
        plan = sandbox_planner.plan_sandbox_execution(manifest, ADAPTERS[ecosystem])
        per_ecosystem[ecosystem] = plan
        if plan["capability_gaps"]:
            capability_gaps_found.append(ecosystem)
    report = {
        "note": "Planning only, against synthetic first-party fixture manifests — no third-party "
                "repository was fetched or executed by this job.",
        "accepted_sandboy_commit_sha": sandbox_planner.ACCEPTED_SANDBOY_COMMIT_SHA,
        "per_ecosystem": per_ecosystem,
        "capability_gaps_found_in": capability_gaps_found,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if capability_gaps_found:
        print(f"::error::capability gaps found for: {capability_gaps_found}", file=sys.stderr)
        return 1
    return 0


def cmd_summary(args):
    frozen_base = json.loads(Path(args.frozen_base_report).read_text())
    n2a_compat = json.loads(Path(args.n2a_compat_report).read_text())
    sandbox_planning = json.loads(Path(args.sandbox_planning_report).read_text())
    selection_report = json.loads(Path(args.selection_report).read_text())

    lines = [
        "# Scope N2-B — Miner Framework Summary\n",
        "**No real N2 source selection was performed.**",
        "**No external repository beyond the frozen N2-A reference was executed.**",
        "**No QODEC or RTK output was inspected.**",
        "**This is not the N2 corpus freeze.**\n",
        "## Frozen-base guard\n",
        f"- base commit: `{frozen_base['base_commit']}`",
        f"- drift: {frozen_base['drift'] or 'none'}",
        f"- Sandboy pin unchanged: {frozen_base['sandboy_pin_unchanged']}",
        f"- pass: {frozen_base['pass']}\n",
        "## N2-A compatibility gate\n",
        f"- zero unexplained incompatibilities: {n2a_compat['zero_unexplained_incompatibilities']}",
        f"- matched fields: {len(n2a_compat['matched_fields'])}",
        f"- intentionally generalized fields: {len(n2a_compat['intentionally_generalized_fields'])}\n",
        "## Sandbox planning (synthetic fixtures only)\n",
        f"- accepted Sandboy commit: `{sandbox_planning['accepted_sandboy_commit_sha']}`",
        f"- capability gaps found in: {sandbox_planning['capability_gaps_found_in'] or 'none'}\n",
        "## Provisional selection (synthetic + N2-A reference registry only)\n",
        f"- registry version: `{selection_report['registry_version']}`",
        f"- quota plan status: {selection_report['quota_plan']['status']}",
        f"- proposed (provisional, non-binding): {selection_report['quota_plan']['proposed_selection']}\n",
        "## Non-goals confirmed unstarted\n",
        "RepoSelector automatic discovery, final 18-case selection, new third-party checkouts/builds, "
        "RTK filter mapping, QODEC benchmark execution, hybrid benchmark, N2-C.\n",
    ]
    Path(args.out).write_text("\n".join(lines) + "\n")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="command", required=True)

    p = sub.add_parser("frozen-base")
    p.add_argument("--repo-root", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_frozen_base)

    p = sub.add_parser("adapter-capabilities")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_adapter_capabilities)

    p = sub.add_parser("selection-report")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_selection_report)

    p = sub.add_parser("n2a-compatibility")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_n2a_compatibility)

    p = sub.add_parser("sandbox-planning")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_sandbox_planning)

    p = sub.add_parser("summary")
    p.add_argument("--frozen-base-report", required=True)
    p.add_argument("--n2a-compat-report", required=True)
    p.add_argument("--sandbox-planning-report", required=True)
    p.add_argument("--selection-report", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_summary)

    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
