import json, os, subprocess, sys, tempfile, hashlib, yaml
sys.path.insert(0, "/home/tandem/007-c2/research/b1-context/tools")
from o7b1.canonical import canonical_bytes, sha256_hex
from o7b1 import evaluate_v1 as ev

RB = "/home/tandem/007-c2/research/b1-context"
HOLD = f"{RB}/holdout"
BIN = "/home/tandem/cargo-target/release/qodec"
QA_COMMIT = "6a5d40304d5997fd9ba6325521f3e212eba4aa75"
QA_TREE = "c5866a5f04c0be381f76756d60fa13158ac39079"
def sha(b): return "sha256:" + sha256_hex(b)
def canon(o): return "sha256:" + sha256_hex(canonical_bytes(o))
def dual(p):
    b = open(p, "rb").read(); o = json.loads(b) if p.endswith(".json") else yaml.safe_load(b)
    return {"artifact_bytes_sha256": sha(b), "canonical_object_sha256": canon(o), "byte_length": len(b)}

verdict = {"record": "o7.b1.holdout-run-verdict/v0",
           "producer": {"qodec_commit": QA_COMMIT, "binary_sha256": sha(open(BIN, "rb").read()),
                        "adapter": "o7.b1.qodec-project-arm/v0"},
           "discipline": "run AFTER freeze; frozen producer + adapter unchanged; C1 not used",
           "cases": {}}

for case in ("h1", "h2", "h3"):
    d = f"{HOLD}/{case}"
    contract = yaml.safe_load(open(f"{d}/evaluation-contract-v1-r3.yaml", "rb").read())
    tids = sorted(contract["tasks"].keys())
    arm = f"{d}/qodec-run/qodec-project-arm"
    os.makedirs(arm, exist_ok=True)
    env = dict(os.environ, QODEC_EXECUTION_COMMIT=QA_COMMIT, QODEC_EXECUTION_TREE=QA_TREE)
    cmd = [sys.executable, f"{RB}/tools/build_qodec_project_request.py",
           "--contract", f"{d}/evaluation-contract-v1-r3.yaml", "--gold", f"{d}/gold-state.json",
           "--b0-report", f"{d}/report.json", "--b0-comparison", f"{d}/projection-comparison.json",
           *sum([["--b0-context", f"{t}={d}/baseline/tasks/{t}/context.json"] for t in tids], []),
           "--qodec-bin", BIN, "--out", arm]
    r = subprocess.run(cmd, capture_output=True, env=env)
    if r.returncode != 0:
        verdict["cases"][case] = {"qodec_project_operational": "REFUSAL", "detail": r.stderr.decode()[:300]}
        print(f"{case}: qodec project OPERATIONAL REFUSAL: {r.stderr.decode()[:120]}")
        continue
    # rebind contract to Q1R contexts
    reb = json.loads(json.dumps(contract))
    reb["bound_inputs"]["r3_1_report"] = dict(logical_id="report.json", visibility="public", **dual(f"{arm}/report.json"))
    reb["bound_inputs"]["r3_1_projection_comparison"] = dict(logical_id="projection-comparison.json", visibility="public", **dual(f"{arm}/projection-comparison.json"))
    for t in tids:
        reb["bound_inputs"]["task_contexts"][t] = dict(visibility="public", **dual(f"{arm}/tasks/{t}/context.json"))
    cpath = f"{d}/qodec-run/arm-evaluation-contract-v1-r3.yaml"
    open(cpath, "w").write(yaml.safe_dump(reb, sort_keys=False, allow_unicode=True, width=120))
    # evaluate Q1R (dev)
    priv = tempfile.mkdtemp(); bp = f"{priv}/s.jsonl"
    open(bp, "wb").write(open(f"{d}/derived-body.jsonl", "rb").read())
    args = ["--contract", cpath, "--out", f"{priv}/eval.json", "--mode", "dev"]
    def inp(role, logical, path): args.extend(["--input", f"{role}:{logical}={path}"])
    inp("gold_state", "gold-state.json", f"{d}/gold-state.json"); inp("fixture_manifest", "manifest.yaml", f"{d}/manifest.yaml")
    inp("source_selectors", "source-selectors.yaml", f"{d}/source-selectors.yaml"); inp("task_registry", "tasks.yaml", f"{d}/tasks.yaml")
    inp("budget", "budget-v0.yaml", f"{d}/budget-v0.yaml"); inp("report", "report.json", f"{arm}/report.json")
    inp("projection_comparison", "projection-comparison.json", f"{arm}/projection-comparison.json")
    inp("derived_manifest", "derived-manifest.json", f"{d}/derived-manifest.json")
    for f in os.listdir(d):
        if f.startswith("task-") and f.endswith(".yaml"): inp("task_file", f, f"{d}/{f}")
        if f.startswith("questions-"): inp("questions_file", f, f"{d}/{f}")
    for t in tids: inp("task_context", t, f"{arm}/tasks/{t}/context.json")
    inp("derived_body", "holdout-session", bp)
    rc = ev.main(args)
    out = json.load(open(f"{priv}/eval.json")) if rc == 0 else None
    # baseline verdict from freeze manifest
    fz = json.load(open(f"{HOLD}/holdout-freeze-manifest.json"))["cases"][case]
    # compare selection + relations vs baseline, gold-grounding
    gold = json.load(open(f"{d}/gold-state.json"))
    gold_edges = {(x["from"], x["kind"], x["to"]) for x in gold["relations"]}
    per = []
    for t in tids:
        q = json.load(open(f"{arm}/tasks/{t}/context.json")); b = json.load(open(f"{d}/baseline/tasks/{t}/context.json"))
        qs = sorted(o["observation_id"] for o in q["selected"]); bs = sorted(o["observation_id"] for o in b["selected"])
        qr = sorted((x["from"], x["kind"], x["to"]) for x in q["relations"])
        grounded = all(e in gold_edges for e in qr)
        per.append({"task": t, "baseline_selected": bs, "q1r_selected": qs, "selection_changed": qs != bs,
                    "q1r_added": sorted(set(qs) - set(bs)), "every_q1r_relation_gold_grounded": grounded})
    verdict["cases"][case] = {
        "shape": fz["shape"], "baseline_overall": fz["baseline_overall"], "baseline_failed_axes": fz["baseline_failed_axes"],
        "qodec_project_operational": "OK", "q1r_overall": out["overall"] if out else "REFUSAL(exit %d)" % rc,
        "q1r_axes": out["outcome_axes"] if out else {},
        "q1r_failed_axes": sorted(k for k, v in (out["outcome_axes"] if out else {}).items() if v == "FAIL"),
        "repaired": (fz["baseline_overall"] == "FAIL" and out and out["overall"] == "PASS"),
        "per_task": per}
    print(f"{case} ({fz['shape']}): baseline={fz['baseline_overall']}{fz['baseline_failed_axes']} -> Q1R={verdict['cases'][case]['q1r_overall']}{verdict['cases'][case]['q1r_failed_axes']} | repaired={verdict['cases'][case]['repaired']}")

open(f"{HOLD}/holdout-run-verdict.json", "w").write(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
print("holdout-run-verdict.json written")
