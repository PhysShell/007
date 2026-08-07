#!/usr/bin/env python3
"""Run one fixture under one measurement profile against a code-graph engine.

Minimal by design. This reads eight cases; it is not a general runner over 257
languages, and the stop rules in docs/gortex-evaluation-harness.md say it must
not become one before a first verdict.

  tools/measure.py --engine <path-to-gortex> --case case-0001-... --profile manifested

What it does, in the order the corpus's scoring rule requires:

  1. materialise the fixture into a scratch workspace and overlay the profile;
  2. index it;
  3. resolve the seed, and CHECK IT AGAINST gold_identity before anything else
     -- a mis-resolved seed is a localization failure and must never be
     recorded as a fidelity result;
  4. query the relation, score expected/forbidden;
  5. record `origin` and `caveats` verbatim as the observed capability signal.

On (3): a profile is an INPUT to the observer, never evidence about it. The
presence of tsconfig.json does not mean a language server resolved anything --
measured 2026-08-07, installing typescript-language-server alone changed no
edge, and only the manifests moved edges from text_matched to lsp_resolved. So
the profile is recorded for reproducibility, and the thing scored is the
`origin` the engine actually emitted.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent


def sh(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def gortex(engine, args, cwd, env, timeout=180):
    return sh([engine, *args, "--no-progress"], cwd=cwd, env=env, timeout=timeout)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", required=True)
    ap.add_argument("--case", required=True)
    ap.add_argument("--profile", required=True, choices=["bare", "manifested"])
    ap.add_argument("--workspace", required=True)
    ap.add_argument("--out", help="write the result record here as JSON")
    a = ap.parse_args()

    case_dir = ROOT / "fixtures" / a.case
    profile = yaml.safe_load((ROOT / "profiles" / f"{a.profile}.yaml").read_text())
    oracle_file = case_dir / "oracle.yaml"
    if not oracle_file.exists():
        print(f"  {a.case}: no oracle.yaml (lifecycle cases use lifecycle.yaml)", file=sys.stderr)
        return 2
    oracle = yaml.safe_load(oracle_file.read_text())

    # 1. materialise
    ws = pathlib.Path(a.workspace) / f"{a.case}--{a.profile}"
    if ws.exists():
        shutil.rmtree(ws)
    ws.mkdir(parents=True)
    shutil.copytree(case_dir / "source", ws / "source")
    if profile.get("overlay"):
        for f in (ROOT / profile["overlay"]).iterdir():
            shutil.copy2(f, ws / f.name)
    sh(["git", "init", "-q", "."], cwd=ws)
    sh(["git", "add", "-A"], cwd=ws)
    sh(["git", "-c", "user.email=a@b", "-c", "user.name=c", "commit", "-qm", a.case], cwd=ws)

    # Per-run daemon state. A shared HOME leaks tracked repos and index state
    # between profiles, which would silently measure one profile's graph while
    # claiming the other's -- the same "reported without being measured" fault
    # the corpus exists to detect.
    env = dict(os.environ)
    env["HOME"] = str(ws.parent / f"home--{a.case}--{a.profile}")
    env["XDG_CONFIG_HOME"] = env["HOME"] + "/.config"
    env["XDG_DATA_HOME"] = env["HOME"] + "/.local/share"
    shutil.rmtree(env["HOME"], ignore_errors=True)
    pathlib.Path(env["HOME"]).mkdir(parents=True, exist_ok=True)

    gortex(a.engine, ["daemon", "start", "--detach"], ws, env, timeout=120)
    gortex(a.engine, ["track", "."], ws, env, timeout=900)
    # let indexing and any deferred LSP resolution settle before querying
    for _ in range(30):
        st = gortex(a.engine, ["status"], ws, env)
        if "nodes" in st.stdout:
            break
        time.sleep(2)

    repo = ws.name
    record = {
        "case": a.case,
        "profile": a.profile,
        "engine": a.engine,
        "engine_version": None,
        "seed_binding": [],
        "facts": [],
    }
    st = gortex(a.engine, ["status"], ws, env)
    for line in st.stdout.splitlines():
        if line.startswith("daemon "):
            record["engine_version"] = line.split()[1]

    # 3. seed resolution, checked against gold_identity FIRST
    seeds = {}
    for s in oracle["seeds"]:
        g = s["gold_identity"]
        want_name = g["symbol"].split(" ")[0].split(".")[-1]
        r = gortex(a.engine, ["query", "symbol", want_name, "--format", "json"], ws, env)
        try:
            hits = json.loads(r.stdout or "{}").get("results", []) or []
        except json.JSONDecodeError:
            hits = []
        lo, hi = g["line_range"]
        want_path = g["path"]
        match = None
        for h in hits:
            fp = h.get("file_path", "")
            if fp.endswith(want_path.replace("source/", "source/")) or want_path in fp:
                if lo <= (h.get("start_line") or -1) <= hi:
                    match = h
                    break
        seeds[s["id"]] = match
        record["seed_binding"].append({
            "seed": s["id"],
            "gold": g,
            "resolved_id": (match or {}).get("id"),
            "resolved_line": (match or {}).get("start_line"),
            "bound": match is not None,
        })

    REL = {"callers": "callers", "references": "usages", "implementations": "implementations"}

    for i, fact in enumerate(oracle["facts"]):
        seed = seeds.get(fact["seed"])
        entry = {
            "index": i,
            "seed": fact["seed"],
            "relation": fact["relation"],
            "seed_bound": seed is not None,
        }
        if seed is None:
            entry["outcome"] = "SEED_UNBOUND"
            entry["note"] = "localization failure (S1); not scored as fidelity (S2)"
            record["facts"].append(entry)
            continue
        sub = REL.get(fact["relation"])
        if sub is None:
            entry["outcome"] = "RELATION_UNSUPPORTED"
            record["facts"].append(entry)
            continue
        r = gortex(a.engine, ["query", sub, seed["id"], "--format", "json"], ws, env)
        try:
            d = json.loads(r.stdout or "{}")
        except json.JSONDecodeError:
            d = {}
        edges = d.get("edges") or []
        origins = sorted({e.get("origin") for e in edges if e.get("origin")})
        # confidence_label and tier are part of the admission key. Dropping them
        # would measure less than the engine reported and then present the
        # result as the engine's answer -- the fault this corpus exists to find.
        conf_labels = sorted({e.get("confidence_label") for e in edges if e.get("confidence_label")})
        tiers = sorted({e.get("tier") for e in edges if e.get("tier")})
        caveat = d.get("caveat") or {}

        observed = set()
        for e in edges:
            other = e.get("from") if e.get("to", "").endswith(seed["id"].split("::")[-1]) else e.get("to")
            if other:
                observed.add(other.split("::")[-1])
        # implementations come back as nodes rather than edges
        if fact["relation"] == "implementations" and not edges:
            for n in d.get("nodes") or []:
                if n.get("id") != seed["id"]:
                    observed.add(n["id"].split("::")[-1])

        def leaf(e):
            return e["symbol"].split(" ")[0].split(".")[-1]

        ts_expected = [e for e in fact["expected"] if e["path"].endswith(".ts")]
        missing = [e["symbol"] for e in ts_expected if leaf(e) not in observed]
        present_forbidden = [e["symbol"] for e in fact["forbidden"] if leaf(e) in observed]

        adm = fact["admissibility"]
        correct = not missing and not present_forbidden
        marked = bool(caveat) or (bool(origins) and all(o == "text_matched" for o in origins))
        if not edges and not observed:
            # An empty answer is the most dangerous shape: read as "no callers"
            # it licenses a delete. It is only admissible if marked unverified.
            outcome = "SILENT_EMPTY" if not caveat else "EMPTY_CAVEATED"
        elif correct:
            outcome = "PASS"
        elif marked:
            outcome = "MISS_HONEST"
        else:
            outcome = "FALSE_SAFE"
        # A correct edge set still fails when exhaustiveness was not available
        # and nothing said so.
        if outcome == "PASS" and adm["may_claim_complete"] is False and not caveat:
            outcome = "FALSE_SAFE_OVERCLAIM"

        entry.update({
            "observed": sorted(observed),
            "expected_missing": missing,
            "forbidden_present": present_forbidden,
            "origin": origins,
            "confidence_label": conf_labels,
            "tier": tiers,
            "caveat_class": caveat.get("class"),
            "oracle_may_claim_complete": adm["may_claim_complete"],
            "oracle_required_caveats": adm.get("required_caveats", []),
            "outcome": outcome,
        })
        record["facts"].append(entry)

    gortex(a.engine, ["daemon", "stop"], ws, env)
    out = json.dumps(record, indent=2, sort_keys=True)
    if a.out:
        pathlib.Path(a.out).write_text(out + "\n")
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
