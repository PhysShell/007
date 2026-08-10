#!/usr/bin/env python3
"""L3B — does dictionary-free structural compression keep model-facing quality?

Two contemporaneous arms over the same frozen case-0002 semantics:

    RAW_VALID      exact frozen Q1R JSON
    JSTRUCT_VALID  the same bytes encoded by the qualified `jstruct` producer

The scaffolding is deliberately **representation-neutral in both arms**. Neither
is labelled "JSON", "compressed", "Qodec" or "table", and no reading tutorial is
supplied for either. That is the property under test: a representation whose
affordance is carried by its own surface —

    @relations[from,kind,to]
    obs-a|supports|obs-b

— rather than by an instruction telling the reader how to cope with it. L1's
arms needed a 261-token notation block; if this one needs prose to be legible,
it has not solved the problem it was built for.

Because the neutral label changes the RAW prompt relative to L1/L2, RAW is run
fresh here. No response is reused across experiments, and the token accounting
is recomputed from the final assembled prompts rather than inherited.
"""
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import l1_ab as L1  # noqa: E402  -- scorer + question surfaces reused verbatim

RB = Path("/home/tandem/007-c2/research/b1-context")
L2DIR = RB / "results" / "case-0002-l2-protected-representation-v1"
L3ADIR = RB / "results" / "case-0002-l3a-jstruct-feasibility-v1"
OUT = RB / "results" / "case-0002-l3b-jstruct-model-ab-v1"
QODEC = Path("/home/tandem/cargo-target/qodec-l2/release/qodec")

ARMS = ["RAW_VALID", "JSTRUCT_VALID"]

FROZEN = {
    "qodec_commit": "0d5097a7142ae8e617c28aadc9dc6de70b1627f1",
    "qodec_tree": "e5ec86fa7178dec668a29c4072945c95eca23a90",
    "qualification_run": 31415814840,
    "qualification_result": "PASS",
    "provider_boundary": "fa93d63636bf53892edbd02fec01ab619ffc1071",
    "l1_007_commit": "40edf6dc4ad6e96ecc81e7a71ce1b0765c369a3b",
    "q1r_producer_qodec_qa": "6a5d40304d5997fd9ba6325521f3e212eba4aa75",
    "adapter": "f8fa87e349c48de0678ccc4a4ba4be14463dfc85",
}

PROVIDER = dict(L1.PROVIDER)
PROVIDER["l3b_note"] = ("identical to L1/L2: temperature 0, reasoning disabled at key level, "
                        "one key pinned per model, requested==reported enforced, N=3")


def sha(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


def arm_context_bytes(arm, tid):
    if arm == "RAW_VALID":
        # The exact frozen Q1R semantic context, as L1/L2 sent it.
        return L1.raw_context_bytes(tid)
    return (L3ADIR / "jstruct-cells" / f"{tid}.jstruct.q").read_bytes()


def build_user_prompt(arm, tid, questions):
    """Identical scaffolding for both arms. The only difference is the bytes."""
    qb = L1.questions_block(questions)
    ctx = arm_context_bytes(arm, tid).decode("utf-8")
    return (f"CONTEXT:\n{ctx}\n\n"
            f"QUESTIONS:\n{qb}\n\n"
            f"OUTPUT SHAPE:\n{L1.OUTPUT_SHAPE}")


def cmd_freeze():
    OUT.mkdir(parents=True, exist_ok=True)
    prompts = OUT / "frozen-prompts"
    prompts.mkdir(exist_ok=True)
    (prompts / "system.txt").write_text(L1.SYSTEM_PROMPT)

    # Balanced alternation: neither arm systematically goes first.
    schedule = {}
    for ti, t in enumerate(L1.TASKS):
        for r in range(PROVIDER["repetitions"]):
            schedule[f"{t['task_id']}|rep{r}"] = (
                ARMS if (ti + r) % 2 == 0 else list(reversed(ARMS)))

    l3a = json.loads((L3ADIR / "l3a-receipt.json").read_text())
    tasks = []
    cold = {a: 0 for a in ARMS}
    for t in L1.TASKS:
        tid = t["task_id"]
        questions = L1.load_questions(t["questions_file"])
        row = {"task_id": tid, "n_questions": len(questions), "arms": {}}
        for arm in ARMS:
            user = build_user_prompt(arm, tid, questions)
            (prompts / f"{tid}.{arm}.user.txt").write_text(user)
            ctx = arm_context_bytes(arm, tid)
            tok = L1.o200k_tokens((L1.SYSTEM_PROMPT + "\n" + user).encode())
            cold[arm] += tok
            row["arms"][arm] = {
                "context_sha256": sha(ctx),
                "context_bytes": len(ctx),
                "prompt_sha256": sha(user.encode()),
                "cold_o200k": tok,
            }
        tasks.append(row)

    manifest = {
        "record": "o7.b1.l3b-jstruct-model-ab-freeze/v1",
        "purpose": ("Test whether the dictionary-free JSON structural representation preserves "
                    "model-facing quality relative to RAW while retaining its independently "
                    "established token saving."),
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "model_results_present": False,
        "qodec": {
            "commit": FROZEN["qodec_commit"], "tree": FROZEN["qodec_tree"],
            "binary_sha256": sha(QODEC.read_bytes()),
            "qualification_run": FROZEN["qualification_run"],
            "qualification_result": FROZEN["qualification_result"],
            "codec": "jstruct",
        },
        "provider_boundary": {"commit": FROZEN["provider_boundary"]},
        "frozen": FROZEN,
        "arms": ARMS,
        "arms_excluded": {
            "QODEC_FULL_ALIAS": "L1 historical reference only; no provider calls spent",
            "QODEC_PROTECTED_DEEP": "L2 historical reference only; no provider calls spent",
        },
        "prompt_fairness": {
            "scaffolding": "CONTEXT: / QUESTIONS: / OUTPUT SHAPE:, identical in both arms",
            "representation_labelled": False,
            "notation_tutorial_supplied": False,
            "raw_rerun_fresh": True,
            "reason": ("the neutral label changes the RAW prompt relative to L1/L2, so RAW is "
                       "measured fresh here rather than inherited"),
        },
        "jstruct_precall_properties": {
            "roundtrip": l3a["JSTRUCT"]["roundtrip"],
            "identifiers_literal": f'{l3a["JSTRUCT"]["identifier_literal_count"]}/'
                                   f'{l3a["JSTRUCT"]["identifier_total_count"]}',
            "dictionary_or_legend": l3a["JSTRUCT"]["dictionary_or_legend_required"],
        },
        "questions_gold": {"unchanged": True,
                           "files": [t["questions_file"] for t in L1.TASKS]},
        "scorer": {
            "imported_from_l1_ab": True,
            "version": L1.SCORER_VERSION,
            "implementation_sha256": sha(Path(L1.__file__).read_bytes()),
            "alias_glyph_check_retained": True,
        },
        "panel": L1.PANEL,
        "provider_surfaces": PROVIDER,
        "system_prompt_sha256": sha(L1.SYSTEM_PROMPT.encode()),
        "arm_order_schedule": schedule,
        "calls_planned": len(L1.PANEL) * len(L1.TASKS) * PROVIDER["repetitions"] * len(ARMS),
        "quality_rule": {
            "per_model_noninferior": {
                "jstruct_answer_correct_gte_raw": True,
                "paired_hard_regressions_vs_raw": 0,
            },
            "hard_regression_axes": ["stale_as_current_error",
                                     "unsupported_or_fabricated_claim",
                                     "missing_required_witness"],
            "panel_success_requires_all_four_models": True,
            "note": ("not relaxed when JSTRUCT gains correctness in another cell; that exact "
                     "phenomenon occurred in L2 and the paired rule correctly rejected it"),
        },
        "token_accounting_recomputed_from_final_prompts": True,
        "token_economics": {
            "RAW_cold": cold["RAW_VALID"],
            "JSTRUCT_cold": cold["JSTRUCT_VALID"],
            "saving_fraction": round(1 - cold["JSTRUCT_VALID"] / cold["RAW_VALID"], 4),
            "l3a_model_free_reference": l3a["saving_fraction"],
        },
        "tasks": tasks,
    }
    body = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    (OUT / "l3b-freeze-manifest.json").write_bytes(body)
    e = manifest["token_economics"]
    print(f"cold RAW={e['RAW_cold']} JSTRUCT={e['JSTRUCT_cold']} "
          f"saving={e['saving_fraction'] * 100:.2f}% (L3A model-free ref {e['l3a_model_free_reference'] * 100:.2f}%)")
    print("calls planned:", manifest["calls_planned"])
    print("freeze:", sha(body))
    return 0


def cmd_run():
    manifest = json.loads((OUT / "l3b-freeze-manifest.json").read_text())
    gold_status, gold_valid, _rel, gold_rel_fk = L1.gold_status_map()
    glyphs = L1.legend_alias_glyphs()   # retained: a jstruct body must never contain one
    pm = L1._pm()
    system_text = (OUT / "frozen-prompts" / "system.txt").read_text()
    log_path = OUT / "l3b-run-progress.log"
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
                    obj, _malformed = L1.parse_model_json(rec.get("content"))
                    by_qid = {}
                    if isinstance(obj, dict) and isinstance(obj.get("answers"), list):
                        for a in obj["answers"]:
                            if isinstance(a, dict) and a.get("question_id"):
                                by_qid[a["question_id"]] = a
                    for q in questions:
                        s = L1.score_question(q, by_qid.get(q["id"]), gold_status, gold_valid,
                                              gold_rel_fk, present, glyphs)
                        cells.append({"model_name": name, "task_id": tid, "question_id": q["id"],
                                      "arm": arm, "replicate": r,
                                      "http_outcome": rec["http_outcome"],
                                      "model_reported": rec.get("model_reported"),
                                      "substitution": rec.get("substitution"), **s})
                    log(f"[{name:8}] {tid.split('-', 2)[-1][:26]:26} rep{r} {arm:13} "
                        f"http={rec['http_outcome']:12} lat={rec.get('latency_s')}s "
                        f"correct={sum(1 for c in cells[-len(questions):] if c['answer_correct'])}"
                        f"/{len(questions)}")
        (mdir / "cells.json").write_text(json.dumps(cells, indent=2, sort_keys=True) + "\n")
        per_model[name] = {"entry": entry, "attempts": attempts, "cells": cells}

    receipt = classify(manifest, per_model)
    (OUT / "l3b-receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print("\n=== L3B ===")
    print("principal:", receipt["principal_classification"])
    return 0


AXES = ["answer_correct", "required_evidence_used", "required_relation_evidence_used",
        "missing_required_witness", "stale_as_current_error",
        "unsupported_or_fabricated_claim", "contradicts_gold",
        "representation_parse_failure", "refusal", "malformed_output"]
HARD = ["stale_as_current_error", "unsupported_or_fabricated_claim", "missing_required_witness"]


def classify(manifest, per_model):
    def tally(cells, arm):
        sub = [c for c in cells if c["arm"] == arm]
        out = {a: sum(1 for c in sub if c[a]) for a in AXES}
        out["n"] = len(sub)
        return out

    def hard_regressions(cells):
        def key(c):
            return (c["task_id"], c["question_id"], c["replicate"])
        ref = {key(c): c for c in cells if c["arm"] == "RAW_VALID"}
        bad = []
        for c in cells:
            if c["arm"] != "JSTRUCT_VALID":
                continue
            b = ref.get(key(c))
            if b is None:
                continue
            if any(c[a] and not b[a] for a in HARD):
                bad.append({"cell": key(c), "axes": [a for a in HARD if c[a] and not b[a]]})
        return bad

    models, pooled = {}, {a: {ax: 0 for ax in AXES} for a in ARMS}
    pooled_n = {a: 0 for a in ARMS}
    any_sub = False
    completed_any = False
    for name, blob in per_model.items():
        cells = blob["cells"]
        any_sub = any_sub or any(a.get("substitution") for a in blob["attempts"]
                                 if a["http_outcome"] == "completed")
        completed_any = completed_any or any(a["http_outcome"] == "completed"
                                             for a in blob["attempts"])
        row = {"model": blob["entry"]["model"], "family": blob["entry"]["family"]}
        for arm in ARMS:
            t = tally(cells, arm)
            row[arm] = t
            for ax in AXES:
                pooled[arm][ax] += t[ax]
            pooled_n[arm] += t["n"]
        hr = hard_regressions(cells)
        row["paired_hard_regressions"] = len(hr)
        row["hard_regression_cells"] = hr
        row["noninferior"] = (
            row["JSTRUCT_VALID"]["answer_correct"] >= row["RAW_VALID"]["answer_correct"]
            and len(hr) == 0)
        models[name] = row

    e = manifest["token_economics"]
    token_win = e["JSTRUCT_cold"] < e["RAW_cold"]
    all_pass = all(m["noninferior"] for m in models.values())
    pooled_gain = pooled["JSTRUCT_VALID"]["answer_correct"] - pooled["RAW_VALID"]["answer_correct"]

    if any_sub:
        principal = "PROVIDER_QUALIFICATION_FAILED"
    elif not completed_any:
        principal = "JSTRUCT_OPERATIONAL_REFUSAL"
    elif all_pass and token_win and pooled_gain > 0:
        principal = "JSTRUCT_GAIN_WITH_TOKEN_WIN"
    elif all_pass and token_win:
        principal = "JSTRUCT_PARITY_WITH_TOKEN_WIN"
    elif all_pass and not token_win:
        principal = "JSTRUCT_PARITY_WITHOUT_TOKEN_WIN"
    else:
        principal = "JSTRUCT_REGRESSION_WITH_TOKEN_WIN"

    return {
        "record": "o7.b1.l3b-jstruct-model-ab-receipt/v1",
        "principal_classification": principal,
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "frozen": manifest["frozen"],
        "qodec": manifest["qodec"],
        "freeze_manifest_sha256": sha((OUT / "l3b-freeze-manifest.json").read_bytes()),
        "harness_sha256": sha(Path(__file__).read_bytes()),
        "provider_integrity": {"any_substitution": any_sub},
        "panel_success_requires_all_four": True,
        "all_models_noninferior": all_pass,
        "per_model": models,
        "pooled": {"cells_per_arm": pooled_n, **{a: pooled[a] for a in ARMS}},
        "pooled_correct_delta_diagnostic": pooled_gain,
        "token_economics": {**e, "token_win_cold": token_win},
        "residual_axis_of_interest": {
            "axis": "missing_required_witness",
            "RAW": pooled["RAW_VALID"]["missing_required_witness"],
            "JSTRUCT": pooled["JSTRUCT_VALID"]["missing_required_witness"],
            "note": "L2 localized the surviving compression penalty here",
        },
    }


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "freeze"
    if cmd == "freeze":
        raise SystemExit(cmd_freeze())
    if cmd == "run":
        raise SystemExit(cmd_run())
    print("unknown command", cmd)
    raise SystemExit(2)
