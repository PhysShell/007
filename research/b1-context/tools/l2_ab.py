#!/usr/bin/env python3
"""L2 — protected-identifier representation ablation.

Three contemporaneous arms over the same frozen case-0002 semantics:

    RAW_VALID                  exact frozen Q1R JSON (as L1 sent it)
    QODEC_FULL_ALIAS           the frozen L1 %q1 deep bytes -- negative control,
                               never regenerated
    QODEC_LITERAL_IDENTIFIERS  the same input bytes encoded by the newly
                               qualified protected-span producer, with every
                               observation identity claimed verbatim

The only intended difference between the two Qodec arms is *which source spans
may be aliased*. The reader notation is byte-identical to L1's, the prompt
wording is unchanged, and the scorer is imported from `l1_ab` rather than
restated, so nothing about the measuring instrument moved between experiments.

Two questions are kept apart on purpose. The *mechanism* question asks whether
protecting identifiers repaired referential fidelity relative to FULL_ALIAS.
The *product* question asks whether the protected representation reached RAW
non-inferiority while still saving tokens; a rescue that costs the entire token
win is a different outcome from a rescue that keeps it, and collapsing them
would hide that.
"""
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import l1_ab as L1  # noqa: E402  -- scorer/prompt surfaces reused verbatim

RB = Path("/home/tandem/007-c2/research/b1-context")
Q2 = RB / "results" / "case-0002-q2-qodec-projection-arm-v1"
L1DIR = RB / "results" / "case-0002-l1-representation-ab-v1"
OUT = RB / "results" / "case-0002-l2-protected-representation-v1"
QODEC_L2 = Path("/home/tandem/cargo-target/qodec-l2/release/qodec")

ARMS = ["RAW_VALID", "QODEC_FULL_ALIAS", "QODEC_LITERAL_IDENTIFIERS"]

FROZEN = {
    "l1_007_commit": "40edf6dc4ad6e96ecc81e7a71ce1b0765c369a3b",
    "l1_classification": "MODEL_QUALITY_REGRESSION_DESPITE_STRUCTURAL_PASS",
    "q1r_producer_qodec_qa": "6a5d40304d5997fd9ba6325521f3e212eba4aa75",
    "adapter": "f8fa87e349c48de0678ccc4a4ba4be14463dfc85",
    "q1r_authoritative_eval": "sha256:fc76d210277f328cc685e61b80df4214319f612333ea49c7e9126d51becce791",
    "provider_boundary": "fa93d63636bf53892edbd02fec01ab619ffc1071",
    "notation_sha256": "sha256:9d11406e732e4725b1abf4d318187b19dba4319467819993dea4d68539e2b5b1",
}

PROVIDER = dict(L1.PROVIDER)
PROVIDER["l2_note"] = ("identical to L1: temperature 0, reasoning disabled at key level, "
                       "one key pinned per model, requested==reported enforced, N=3")


def sha(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def payload_bytes(tid):
    """The exact input bytes both Qodec arms encode (canonical, no trailing LF)."""
    return (OUT / "protected-spans" / f"{tid}.payload.json").read_bytes()


def arm_context_bytes(arm, tid):
    if arm == "RAW_VALID":
        # Byte-identical to what L1 sent, so RAW is comparable across experiments.
        return L1.raw_context_bytes(tid)
    if arm == "QODEC_FULL_ALIAS":
        return (Q2 / "q1rq-cells" / tid / "encoded.q").read_bytes()
    return (OUT / "protected-cells" / f"{tid}.encoded.q").read_bytes()


def legend_glyphs_union():
    """Alias glyphs from both Qodec arms — the leak check must recognise a
    glyph from whichever encoding produced it."""
    glyphs = set(L1.legend_alias_glyphs())
    for t in L1.TASKS:
        enc = (OUT / "protected-cells" / f"{t['task_id']}.encoded.q").read_text(errors="ignore")
        lines = enc.splitlines()
        try:
            body_i = next(i for i, l in enumerate(lines) if l.startswith("%q1 body"))
        except StopIteration:
            body_i = len(lines)
        for line in lines[1:body_i]:
            if "=" in line:
                for ch in line.split("=", 1)[0]:
                    if ord(ch) > 0x2000:
                        glyphs.add(ch)
    return glyphs


def build_user_prompt(arm, tid, questions, notation):
    qb = L1.questions_block(questions)
    ctx = arm_context_bytes(arm, tid).decode("utf-8")
    if arm == "RAW_VALID":
        return (f"CONTEXT (JSON):\n{ctx}\n\n"
                f"QUESTIONS:\n{qb}\n\n"
                f"OUTPUT SHAPE:\n{L1.OUTPUT_SHAPE}")
    # Both Qodec arms get the identical, unmodified L1 reader notation.
    return (f"{notation.decode('utf-8')}\n\n"
            f"CONTEXT (Qodec %q1 encoded -- decode it via the legend before answering):\n{ctx}\n\n"
            f"QUESTIONS:\n{qb}\n\n"
            f"OUTPUT SHAPE:\n{L1.OUTPUT_SHAPE}")


def cmd_freeze():
    OUT.mkdir(parents=True, exist_ok=True)
    notation = (L1DIR / "notation.txt").read_bytes()
    if sha(notation) != FROZEN["notation_sha256"]:
        raise SystemExit("notation drifted from the L1 binding — L2 would not be a pure ablation")
    glyphs = sorted(legend_glyphs_union())
    notation_tokens = L1.o200k_tokens(notation)

    # Balanced three-arm rotation: each arm starts, sits middle and ends once
    # per task across the three replicates, so no arm systematically runs first.
    schedule = {}
    for ti, t in enumerate(L1.TASKS):
        for r in range(PROVIDER["repetitions"]):
            shift = (r + ti) % 3
            schedule[f"{t['task_id']}|rep{r}"] = ARMS[shift:] + ARMS[:shift]

    manifest = {
        "record": "o7.b1.l2-protected-ablation-freeze/v1",
        "purpose": ("Test whether preserving machine-significant identifiers verbatim recovers the "
                    "referential fidelity that L1 found the %q1 deep representation destroys, and "
                    "at what share of the token win."),
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "frozen": FROZEN,
        "arms": ARMS,
        "provider_surfaces": PROVIDER,
        "panel": L1.PANEL,
        "system_prompt_sha256": sha(L1.SYSTEM_PROMPT.encode()),
        "scorer": {"version": L1.SCORER_VERSION, "reused_from": "l1_ab.score_question (imported, not restated)",
                   "alias_glyphs_union": glyphs},
        "notation": {"sha256": sha(notation), "tokens_o200k": notation_tokens,
                     "identical_to_l1": True},
        "producer": {
            "protected_span_producer_branch": "experiment/deep-protected-spans-v0",
            "binary_sha256": sha(QODEC_L2.read_bytes()) if QODEC_L2.exists() else None,
            "spans_manifest": json.loads((OUT / "l2-protected-spans-manifest.json").read_text()),
        },
        "arm_order_schedule": schedule,
        "acceptance_rule": {
            "quality_noninferior": {"qodec_correct_gte_raw": True, "hard_regression_count_vs_RAW": 0,
                                    "note": "the exact L1 rule; no relaxation for a rescue experiment"},
        },
        "tasks": [],
    }

    prompts = OUT / "frozen-prompts"
    prompts.mkdir(exist_ok=True)
    (prompts / "system.txt").write_text(L1.SYSTEM_PROMPT)
    for t in L1.TASKS:
        tid = t["task_id"]
        questions = L1.load_questions(t["questions_file"])
        row = {"task_id": tid, "n_questions": len(questions), "tokens_o200k": {}, "arms": {}}
        for arm in ARMS:
            user = build_user_prompt(arm, tid, questions, notation)
            (prompts / f"{tid}.{arm}.user.txt").write_text(user)
            cold = L1.o200k_tokens((L1.SYSTEM_PROMPT + "\n" + user).encode())
            ctx = arm_context_bytes(arm, tid)
            row["arms"][arm] = {
                "context_sha256": sha(ctx), "context_bytes": len(ctx),
                "user_prompt_sha256": sha(user.encode()),
                "context_only_tokens": L1.o200k_tokens(ctx),
            }
            row["tokens_o200k"][arm] = {
                "cold": cold,
                "warm": cold - notation_tokens if arm != "RAW_VALID" else cold,
            }
        manifest["tasks"].append(row)

    # Token economics of the ablation, before any model sees anything.
    raw = sum(t["tokens_o200k"]["RAW_VALID"]["cold"] for t in manifest["tasks"])
    full = sum(t["tokens_o200k"]["QODEC_FULL_ALIAS"]["cold"] for t in manifest["tasks"])
    prot = sum(t["tokens_o200k"]["QODEC_LITERAL_IDENTIFIERS"]["cold"] for t in manifest["tasks"])
    manifest["token_economics"] = {
        "RAW_cold": raw, "FULL_ALIAS_cold": full, "PROTECTED_cold": prot,
        "full_alias_saving": raw - full,
        "residual_cold_saving": raw - prot,
        "retained_saving_fraction": round((raw - prot) / (raw - full), 4) if raw != full else None,
        "reading": ("~1.0 protection keeps almost all compression gain; ~0.5 half survives; "
                    "~0 identifier protection consumes the gain; <0 protected is dearer than RAW"),
    }
    (OUT / "l2-freeze-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print("freeze:", sha((OUT / "l2-freeze-manifest.json").read_bytes()))
    e = manifest["token_economics"]
    print(f"tokens cold: RAW={e['RAW_cold']} FULL_ALIAS={e['FULL_ALIAS_cold']} PROTECTED={e['PROTECTED_cold']}")
    print(f"retained_saving_fraction = {e['retained_saving_fraction']}")
    return 0


# ------------------------------- run ---------------------------------------

def cmd_run():
    manifest = json.loads((OUT / "l2-freeze-manifest.json").read_text())
    gold_status, gold_valid, _rel, gold_rel_fk = L1.gold_status_map()
    glyphs = set(manifest["scorer"]["alias_glyphs_union"])
    pm = L1._pm()
    system_text = (OUT / "frozen-prompts" / "system.txt").read_text()
    log_path = OUT / "l2-run-progress.log"
    log_path.write_text("")

    def log(m):
        with log_path.open("a") as fh:
            fh.write(m + "\n")
        print(m, flush=True)

    per_model = {}
    for entry in L1.PANEL:
        model, name = entry["model"], entry["name"]
        key = Path(entry["key_file"]).read_text().strip()
        mdir = OUT / "models" / name
        (mdir / "responses").mkdir(parents=True, exist_ok=True)
        (mdir / "attempts.jsonl").write_text("")
        attempts, cells = [], []
        log(f"==== {name} ({model}) ====")
        for t in L1.TASKS:
            tid = t["task_id"]
            questions = L1.load_questions(t["questions_file"])
            present = L1.context_present_ids(L1.raw_context_bytes(tid))
            for r in range(PROVIDER["repetitions"]):
                for arm in manifest["arm_order_schedule"][f"{tid}|rep{r}"]:
                    user = (OUT / "frozen-prompts" / f"{tid}.{arm}.user.txt").read_text()
                    rec = L1.call_model(pm, key, model, system_text, user)
                    rec.update({"model_name": name, "task_id": tid, "arm": arm, "replicate": r})
                    attempts.append(rec)
                    with (mdir / "attempts.jsonl").open("a") as fh:
                        fh.write(json.dumps({k: v for k, v in rec.items() if k != "content"}) + "\n")
                    (mdir / "responses" / f"{tid}.{arm}.rep{r}.txt").write_text(rec.get("content") or "")
                    obj, malformed = L1.parse_model_json(rec.get("content"))
                    by_qid = {}
                    if isinstance(obj, dict) and isinstance(obj.get("answers"), list):
                        for a in obj["answers"]:
                            if isinstance(a, dict) and a.get("question_id"):
                                by_qid[a["question_id"]] = a
                    for q in questions:
                        s = L1.score_question(q, by_qid.get(q["id"]), gold_status, gold_valid,
                                              gold_rel_fk, present, glyphs)
                        cells.append({"model_name": name, "task_id": tid, "question_id": q["id"],
                                      "arm": arm, "replicate": r, "http_outcome": rec["http_outcome"],
                                      "model_reported": rec.get("model_reported"),
                                      "substitution": rec.get("substitution"), **s})
                    log(f"[{name:8}] {tid.split('-', 2)[-1][:26]:26} rep{r} {arm:26} "
                        f"http={rec['http_outcome']:12} lat={rec.get('latency_s')}s "
                        f"correct={sum(1 for c in cells[-len(questions):] if c['answer_correct'])}/{len(questions)}")
        (mdir / "cells.json").write_text(json.dumps(cells, indent=2, sort_keys=True) + "\n")
        per_model[name] = {"entry": entry, "attempts": attempts, "cells": cells}

    receipt = classify(manifest, per_model)
    (OUT / "l2-receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print("\n=== L2 ===")
    print("mechanism:", receipt["mechanism_classification"])
    print("product  :", receipt["product_classification"])
    return 0


def classify(manifest, per_model):
    axes = ["answer_correct", "missing_required_witness", "stale_as_current_error",
            "unsupported_or_fabricated_claim", "representation_parse_failure",
            "required_relation_evidence_used", "refusal", "malformed_output", "contradicts_gold"]

    def tally(cells, arm):
        sub = [c for c in cells if c["arm"] == arm]
        out = {a: sum(1 for c in sub if c[a]) for a in axes}
        out["n"] = len(sub)
        return out

    def hard_regressions(cells, arm, base="RAW_VALID"):
        key = lambda c: (c["task_id"], c["question_id"], c["replicate"])
        ref = {key(c): c for c in cells if c["arm"] == base}
        bad = []
        for c in cells:
            if c["arm"] != arm:
                continue
            b = ref.get(key(c))
            if b is None:
                continue
            if ((c["stale_as_current_error"] and not b["stale_as_current_error"])
                    or (c["unsupported_or_fabricated_claim"] and not b["unsupported_or_fabricated_claim"])
                    or (c["missing_required_witness"] and not b["missing_required_witness"])):
                bad.append(key(c))
        return bad

    models = {}
    pooled = {arm: {a: 0 for a in axes} for arm in ARMS}
    pooled_n = {arm: 0 for arm in ARMS}
    any_sub = False
    for name, blob in per_model.items():
        cells = blob["cells"]
        any_sub = any_sub or any(a.get("substitution") for a in blob["attempts"]
                                 if a["http_outcome"] == "completed")
        row = {"model": blob["entry"]["model"], "family": blob["entry"]["family"]}
        for arm in ARMS:
            t = tally(cells, arm)
            row[arm] = t
            for a in axes:
                pooled[arm][a] += t[a]
            pooled_n[arm] += t["n"]
        row["hard_regressions_vs_RAW"] = {
            "QODEC_FULL_ALIAS": len(hard_regressions(cells, "QODEC_FULL_ALIAS")),
            "QODEC_LITERAL_IDENTIFIERS": len(hard_regressions(cells, "QODEC_LITERAL_IDENTIFIERS")),
        }
        row["noninferior_protected_vs_raw"] = (
            row["QODEC_LITERAL_IDENTIFIERS"]["answer_correct"] >= row["RAW_VALID"]["answer_correct"]
            and row["hard_regressions_vs_RAW"]["QODEC_LITERAL_IDENTIFIERS"] == 0)
        models[name] = row

    econ = manifest["token_economics"]
    reps = PROVIDER["repetitions"]
    token_win = econ["PROTECTED_cold"] < econ["RAW_cold"]

    # Mechanism: did protection repair referential fidelity vs FULL_ALIAS?
    ref_axes = ["unsupported_or_fabricated_claim", "representation_parse_failure",
                "missing_required_witness"]
    fa = sum(pooled["QODEC_FULL_ALIAS"][a] for a in ref_axes)
    pr = sum(pooled["QODEC_LITERAL_IDENTIFIERS"][a] for a in ref_axes)
    raw_ref = sum(pooled["RAW_VALID"][a] for a in ref_axes)
    all_noninferior = all(m["noninferior_protected_vs_raw"] for m in models.values())
    if pr > fa:
        mechanism = "IDENTIFIER_PROTECTION_WORSENS"
    elif all_noninferior and pr <= raw_ref:
        mechanism = "IDENTIFIER_PROTECTION_RESCUES_REFERENTIAL_FIDELITY"
    elif pr < fa:
        mechanism = "IDENTIFIER_PROTECTION_PARTIALLY_RESCUES"
    else:
        mechanism = "IDENTIFIER_PROTECTION_DOES_NOT_RESCUE"

    prot_correct = pooled["QODEC_LITERAL_IDENTIFIERS"]["answer_correct"]
    raw_correct = pooled["RAW_VALID"]["answer_correct"]
    if any_sub:
        product = "PROTECTED_REPRESENTATION_OPERATIONAL_REFUSAL"
    elif all_noninferior and token_win and prot_correct > raw_correct:
        product = "PROTECTED_REPRESENTATION_GAIN_WITH_TOKEN_WIN"
    elif all_noninferior and token_win:
        product = "PROTECTED_REPRESENTATION_PARITY_WITH_TOKEN_WIN"
    elif all_noninferior and not token_win:
        product = "PROTECTED_REPRESENTATION_PARITY_WITHOUT_TOKEN_WIN"
    else:
        product = "PROTECTED_REPRESENTATION_REGRESSION_WITH_TOKEN_WIN" if token_win else \
                  "PROTECTED_REPRESENTATION_REGRESSION_WITHOUT_TOKEN_WIN"

    return {
        "record": "o7.b1.l2-protected-ablation-receipt/v1",
        "mechanism_classification": mechanism,
        "product_classification": product,
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "frozen": FROZEN,
        "freeze_manifest_sha256": sha((OUT / "l2-freeze-manifest.json").read_bytes()),
        "harness_sha256": sha(Path(__file__).read_bytes()),
        "provider_integrity": {"any_substitution": any_sub},
        "per_model": models,
        "pooled": {"cells_per_arm": pooled_n, **{arm: pooled[arm] for arm in ARMS}},
        "referential_error_totals": {"RAW_VALID": raw_ref, "QODEC_FULL_ALIAS": fa,
                                     "QODEC_LITERAL_IDENTIFIERS": pr,
                                     "axes": ref_axes},
        "token_economics": {**econ,
                            "RAW_cold_all_reps": econ["RAW_cold"] * reps,
                            "PROTECTED_cold_all_reps": econ["PROTECTED_cold"] * reps,
                            "token_win_cold": token_win},
        "all_models_noninferior_vs_raw": all_noninferior,
    }


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "freeze"
    if cmd == "freeze":
        raise SystemExit(cmd_freeze())
    if cmd == "run":
        raise SystemExit(cmd_run())
    print("unknown command", cmd)
    raise SystemExit(2)
