#!/usr/bin/env python3
"""Non-model four-arm pilot runner (Scope N1).

Evaluates every pilot case under four logical arms — RAW, QODEC, RTK, RTK+QODEC —
using the pinned qodec binary, the pinned RTK snapshots already captured in each
bundle, and the o200k token meter. Emits pilot-report.json, pilot-summary.md,
case-manifest.json and provenance-report.json.

No model calls, no judging: this is a lossless-notation / lossy-reducer token and
integrity comparison. It is explicitly NON-GATING and is not a production verdict.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot_lib as pl  # noqa: E402

ARMS = ["RAW", "QODEC", "RTK", "RTK+QODEC"]


def _text_metrics(data: bytes) -> dict:
    return {"bytes": len(data), "lines": len(data.splitlines()),
            "output_sha256": pl.sha256_bytes(data)}


def _ratio(t: int, raw: int) -> float:
    return round(t / raw, 4) if raw else 0.0


def run_case(case_id: str, qodec_bin: str) -> dict:
    b = pl.bundle_dir(case_id)
    case = pl.load_json(b / "case.json")
    nat = pl.load_json(b / pl.snap.NATIVE_RECEIPT)
    rtk_r = pl.load_json(b / pl.snap.RTK_RECEIPT)
    sm = pl.load_json(b / case["snapshot_manifest_path"])
    anchors = pl.load_json(b / case["anchors_path"]).get("anchors", [])

    raw = (b / pl.STREAM_FILE[case["primary_stream"]]).read_bytes()
    rtk_out = (b / pl.snap.RTK_STDOUT).read_bytes()

    env_raw, _ = pl.qodec_envelope(qodec_bin, raw)
    raw_tokens = int(env_raw["tokens_in"])
    qodec_tokens = int(env_raw["tokens_out"])
    qodec_content = env_raw["content"]
    qodec_decoded = pl.qodec_decode(qodec_bin, qodec_content) if env_raw["encoded"] else raw
    qodec_bytes = qodec_content.encode("utf-8")

    env_rtk, _ = pl.qodec_envelope(qodec_bin, rtk_out)
    rtk_tokens = int(env_rtk["tokens_in"])
    hybrid_tokens = int(env_rtk["tokens_out"])
    hybrid_content = env_rtk["content"]
    hybrid_decoded = pl.qodec_decode(qodec_bin, hybrid_content) if env_rtk["encoded"] else rtk_out
    hybrid_bytes = hybrid_content.encode("utf-8")

    raw_sha = pl.sha256_bytes(raw)
    rtk_sha = pl.sha256_bytes(rtk_out)
    arms = {
        "RAW": {"input_sha256": raw_sha, "tokens": raw_tokens, "token_ratio_vs_raw": 1.0,
                "command": nat["argv"], "exit_code": nat["exit_code"],
                "stderr_sha256": nat["stderr_sha256"], "wall_time_s": nat["wall_time_s"],
                "tool_identity": nat["tool_identity"], **_text_metrics(raw)},
        "QODEC": {"input_sha256": raw_sha, "tokens": qodec_tokens,
                  "token_ratio_vs_raw": _ratio(qodec_tokens, raw_tokens),
                  "command": [qodec_bin, "encode", "--codec", pl.CODEC, "--meter", pl.METER],
                  "exit_code": 0, "encoded": env_raw["encoded"], "codec": env_raw["codec"],
                  "roundtrip_ok": qodec_decoded == raw, **_text_metrics(qodec_bytes)},
        "RTK": {"input_sha256": raw_sha, "tokens": rtk_tokens,
                "token_ratio_vs_raw": _ratio(rtk_tokens, raw_tokens),
                "command": rtk_r["argv"], "exit_code": rtk_r["exit_code"],
                "stderr_sha256": rtk_r["stderr_sha256"], "wall_time_s": rtk_r["wall_time_s"],
                "tool_identity": rtk_r["tool_identity"],
                "rtk_classification": rtk_r["rtk_classification"], **_text_metrics(rtk_out)},
        "RTK+QODEC": {"input_sha256": rtk_sha, "tokens": hybrid_tokens,
                      "token_ratio_vs_raw": _ratio(hybrid_tokens, raw_tokens),
                      "command": [qodec_bin, "encode", "--codec", pl.CODEC, "--meter", pl.METER],
                      "exit_code": 0, "encoded": env_rtk["encoded"],
                      "roundtrip_ok": hybrid_decoded == rtk_out, **_text_metrics(hybrid_bytes)},
    }

    # anchor survival through RTK output
    rtk_text = rtk_out.decode("utf-8", errors="replace")
    survived = [a["anchor_id"] for a in anchors if a["value"] in rtk_text]
    lost = [a["anchor_id"] for a in anchors if a["value"] not in rtk_text]

    inv = [
        ("decode(qodec(raw)) == raw", arms["QODEC"]["roundtrip_ok"]),
        ("rtk exit code == 0", rtk_r["exit_code"] == 0),
        ("rtk output non-empty (or raw empty)", len(rtk_out) > 0 or len(raw) == 0),
        ("decode(qodec(rtk)) == rtk", arms["RTK+QODEC"]["roundtrip_ok"]),
        ("qodec tokens <= raw tokens", qodec_tokens <= raw_tokens),
        ("hybrid tokens <= rtk tokens", hybrid_tokens <= rtk_tokens),
        ("committed raw hash matches", raw_sha == sm["raw_stdout_sha256"] if case["primary_stream"] == "raw.stdout" else raw_sha == sm["raw_stderr_sha256"]),
        ("committed rtk hash matches", rtk_sha == sm["rtk_stdout_sha256"]),
    ]
    invariants = [{"invariant": k, "ok": bool(v)} for k, v in inv]
    invariants_ok = all(v for _, v in inv)
    best = min(ARMS, key=lambda a: arms[a]["tokens"])

    return {
        "case_id": case_id, "family": case["family"], "ecosystem": case["ecosystem"],
        "tool": case["tool"], "outcome": case["outcome"], "primary_stream": case["primary_stream"],
        "rtk_classification": rtk_r["rtk_classification"],
        "arms": arms, "best_token_arm": best,
        "anchors": {"total": len(anchors), "survived_rtk": len(survived),
                    "survivors": survived, "lost": lost},
        "invariants": invariants, "invariants_ok": invariants_ok,
    }


def aggregate(cases: list[dict]) -> dict:
    def grp(key):
        out = {}
        for c in cases:
            g = out.setdefault(c[key], {"n": 0, "qodec_ratio": 0.0, "rtk_ratio": 0.0, "hybrid_ratio": 0.0})
            g["n"] += 1
            g["qodec_ratio"] += c["arms"]["QODEC"]["token_ratio_vs_raw"]
            g["rtk_ratio"] += c["arms"]["RTK"]["token_ratio_vs_raw"]
            g["hybrid_ratio"] += c["arms"]["RTK+QODEC"]["token_ratio_vs_raw"]
        for g in out.values():
            n = g["n"]
            for k in ("qodec_ratio", "rtk_ratio", "hybrid_ratio"):
                g[k] = round(g[k] / n, 4)
        return out

    classes = {}
    for c in cases:
        classes[c["rtk_classification"]] = classes.get(c["rtk_classification"], 0) + 1
    best = {}
    for c in cases:
        best[c["best_token_arm"]] = best.get(c["best_token_arm"], 0) + 1
    tot_raw = sum(c["arms"]["RAW"]["tokens"] for c in cases)
    return {
        "per_family": grp("family"), "per_ecosystem": grp("ecosystem"),
        "rtk_classification_counts": classes, "best_token_arm_counts": best,
        "total_raw_tokens": tot_raw,
        "total_qodec_tokens": sum(c["arms"]["QODEC"]["tokens"] for c in cases),
        "total_rtk_tokens": sum(c["arms"]["RTK"]["tokens"] for c in cases),
        "total_hybrid_tokens": sum(c["arms"]["RTK+QODEC"]["tokens"] for c in cases),
        "anchor_survival_total": sum(c["anchors"]["survived_rtk"] for c in cases),
        "anchor_total": sum(c["anchors"]["total"] for c in cases),
    }


def summary_md(report: dict) -> str:
    a = report["aggregates"]
    L = ["# Interop Benchmark v2 — Scope N1 public-log pilot", "",
         "**Non-gating pilot.** Public-development cases only. No model calls, no",
         "reader/judge, no production-promotion verdict. Token counts use the o200k",
         f"meter; QODEC is the `{report['codec']}` lossless notation layer, RTK is a",
         "lossy reducer (may return raw via never_worse).", "",
         f"- Cases: **{report['case_count']}** · arms: {', '.join(report['arms'])}",
         f"- All correctness invariants passed: **{report['all_invariants_ok']}**",
         f"- Total tokens — RAW {a['total_raw_tokens']} · QODEC {a['total_qodec_tokens']}"
         f" · RTK {a['total_rtk_tokens']} · RTK+QODEC {a['total_hybrid_tokens']}",
         f"- RTK classification: {a['rtk_classification_counts']}",
         f"- Best token arm counts: {a['best_token_arm_counts']}",
         f"- Anchors surviving RTK: {a['anchor_survival_total']}/{a['anchor_total']}", ""]

    L += ["## Per-case four-arm table", "",
          "| Case | Family | Ecosystem | RAW tok | QODEC | RTK | RTK+QODEC | RTK class | Best | Roundtrip |",
          "|---|---|---|--:|--:|--:|--:|---|---|---|"]
    for c in report["cases"]:
        ar = c["arms"]
        rt = "ok" if ar["QODEC"]["roundtrip_ok"] and ar["RTK+QODEC"]["roundtrip_ok"] else "FAIL"
        L.append(f"| {c['case_id']} | {c['family']} | {c['ecosystem']} | {ar['RAW']['tokens']} "
                 f"| {ar['QODEC']['tokens']} | {ar['RTK']['tokens']} | {ar['RTK+QODEC']['tokens']} "
                 f"| {c['rtk_classification']} | {c['best_token_arm']} | {rt} |")

    L += ["", "## Per-family aggregation (mean token ratio vs RAW)", "",
          "| Family | n | QODEC | RTK | RTK+QODEC |", "|---|--:|--:|--:|--:|"]
    for fam, g in sorted(a["per_family"].items()):
        L.append(f"| {fam} | {g['n']} | {g['qodec_ratio']} | {g['rtk_ratio']} | {g['hybrid_ratio']} |")
    L += ["", "## Per-ecosystem aggregation (mean token ratio vs RAW)", "",
          "| Ecosystem | n | QODEC | RTK | RTK+QODEC |", "|---|--:|--:|--:|--:|"]
    for eco, g in sorted(a["per_ecosystem"].items()):
        L.append(f"| {eco} | {g['n']} | {g['qodec_ratio']} | {g['rtk_ratio']} | {g['hybrid_ratio']} |")

    L += ["", "## Anchor-survival observations", ""]
    for c in report["cases"]:
        L.append(f"- `{c['case_id']}`: {c['anchors']['survived_rtk']}/{c['anchors']['total']} "
                 f"anchors survive RTK ({c['rtk_classification']})"
                 + (f"; lost: {c['anchors']['lost']}" if c["anchors"]["lost"] else ""))

    failures = [c["case_id"] for c in report["cases"] if not c["invariants_ok"]]
    L += ["", "## Failures and exclusions", "",
          ("- None: all cases passed every correctness invariant." if not failures
           else f"- Cases with failing invariants: {failures}"),
          "- Excluded families (justified): container/orchestrator and application/CI-log "
          "(no runtime / no deterministic first-party tool); runtime exception traceback "
          "(absolute-path irreproducibility) — see pilot-manifest.json.", "",
          "## Status", "",
          "This pilot is **non-gating** and is **not** a production-promotion verdict. It "
          "measures token behaviour and lossless-roundtrip integrity over real logs only."]
    return "\n".join(L) + "\n"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="output directory for the four report artifacts")
    args = ap.parse_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    qodec_bin = pl.resolve_exe("qodec")

    manifest = pl.load_json(pl.MANIFEST_PATH)
    cases = [run_case(cid, qodec_bin) for cid in manifest["cases"]]
    agg = aggregate(cases)
    identity = pl.rcpt.assemble_identity(pl.REPO_ROOT)
    identity["rtk_source_sha"] = __import__("os").environ.get("RTK_SOURCE_SHA")
    report = {
        "scope": "N1", "contract_version": manifest["contract_version"], "non_gating": True,
        "meter": pl.METER, "codec": pl.CODEC, "arms": ARMS, "identity": identity,
        "case_count": len(cases), "cases": cases, "aggregates": agg,
        "all_invariants_ok": all(c["invariants_ok"] for c in cases),
    }
    for e in pl.js.validate(report, pl.load_schema("pilot-report.schema.json")):
        print(f"[report-schema] {e}", file=sys.stderr)
        return 2

    (out / "pilot-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    (out / "pilot-summary.md").write_text(summary_md(report))
    case_manifest = {"scope": "N1", "contract_version": manifest["contract_version"],
                     "cases": [{"case_id": c["case_id"], "family": c["family"],
                                "ecosystem": c["ecosystem"], "outcome": c["outcome"],
                                "rtk_classification": c["rtk_classification"],
                                "raw_tokens": c["arms"]["RAW"]["tokens"]} for c in cases],
                     "distribution": manifest["distribution"]}
    (out / "case-manifest.json").write_text(json.dumps(case_manifest, indent=2, sort_keys=True) + "\n")
    prov = {"scope": "N1", "source_policy": manifest["source_policy"], "cases": []}
    for cid in manifest["cases"]:
        p = pl.load_json(pl.bundle_dir(cid) / "provenance.json")
        c = pl.load_json(pl.bundle_dir(cid) / "case.json")
        prov["cases"].append({"case_id": cid, "origin_kind": p["origin_kind"],
                              "license": p["license"], "source_revision": p["source_revision"],
                              "generator_identity": p["generator_identity"],
                              "secret_review": p["secret_review"], "pii_review": p["pii_review"],
                              "hand_authored": c["hand_authored"]})
    (out / "provenance-report.json").write_text(json.dumps(prov, indent=2, sort_keys=True) + "\n")

    print(f"pilot four-arm run: {len(cases)} cases, all_invariants_ok={report['all_invariants_ok']}")
    print(f"artifacts -> {out}/pilot-report.json, pilot-summary.md, case-manifest.json, provenance-report.json")
    return 0 if report["all_invariants_ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
