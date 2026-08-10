#!/usr/bin/env python3
"""L1 model-facing A/B: RAW_VALID (Q1R raw context) vs QODEC_VALID (Q1RQ Qodec encoding).

Frozen protocol (controller-authorized). Two subcommands:

    freeze  -- assemble every model-facing surface + the deterministic scorer
               identity, compute digests, write l1-freeze-manifest.json. NO calls.
    run     -- execute the frozen A/B through the L0-qualified transport
               (provider_matrix.send_json), record provider accounting per
               attempt, score every response blind to arm, aggregate, emit one
               principal classification, write the L1 receipt.

The scorer is deterministic and rule-based against frozen question IDs, gold
observation status, and required relation paths. No LLM judge is used, so no
judge can invent ground truth. The natural-language answer is captured but the
score is citation/relation-based.
"""
import base64
import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path

import yaml

RB = Path("/home/tandem/007-c2/research/b1-context")
FX = RB / "fixtures" / "case-0002"
Q2 = RB / "results" / "case-0002-q2-qodec-projection-arm-v1"
OUT = RB / "results" / "case-0002-l1-representation-ab-v1"
QODEC_BIN = Path("/home/tandem/cargo-target/release/qodec")
PM_DIR = Path("/tmp/claude-1001/-home-tandem/be909b9c-a365-400f-aafd-058b173f9f58/"
              "scratchpad/qodec-pr16/evals/provider-matrix")
CORPUS_TOOLS = Path("/tmp/claude-1001/-home-tandem/be909b9c-a365-400f-aafd-058b173f9f58/"
                    "scratchpad/qodec-pr16/evals/interop/v2/corpus/tools")
KEY_FILE = Path.home() / ".config" / "o7-research" / "arli.key"

# ---- frozen producer identities (do not modify for L1) ----
FROZEN = {
    "qodec_qa": "6a5d40304d5997fd9ba6325521f3e212eba4aa75",
    "adapter": "f8fa87e349c48de0678ccc4a4ba4be14463dfc85",
    "q1r_authoritative_eval": "sha256:fc76d210277f328cc685e61b80df4214319f612333ea49c7e9126d51becce791",
    "evaluator": "58a7a111",
    "l0_qualified_head": "fa93d63636bf53892edbd02fec01ab619ffc1071",
}

# ---- frozen provider / model / decoding surfaces ----
KEYDIR = Path.home() / ".config" / "o7-research"
PROVIDER = {
    "provider": "arliai",
    "endpoint_authority": "https://api.arliai.com/v1/chat/completions",
    "api_style": "openai-chat",
    "temperature": 0,
    "max_tokens": 8000,
    "timeout_s": 200.0,
    "retry_policy": "at most 1 transport retry on before-response failure; after-headers loss is not retried",
    "seed": None,   # Arli not verified to honor seed; determinism sought via temperature 0 + reasoning-off + N replicates
    "thinking_disabled_at_key_level": True,   # per-key server-side setting; verified rlen=0
    "repetitions": 3,
    "qualification_identity": FROZEN["l0_qualified_head"],
    "transport": "provider_matrix.send_json @ L0-qualified head fa93d63 (https-only, no-redirect, bounded read, credential-safe)",
    "substitution_check": "each key is pinned to one model; model_requested set to the pinned model; verified reported==requested, rlen==0",
}

# Cross-family panel: one Arli key per model, each pinned to that model with
# Model Thinking disabled. `principal` marks the model that carries the
# protocol's single principal classification; the rest are the diagnostic panel.
PANEL = [
    {"name": "gemma", "model": "Gemma-4-31B-it", "family": "Google",
     "key_file": str(KEYDIR / "arli-gemma.key"), "principal": True},
    {"name": "glm47", "model": "GLM-4.7", "family": "Zhipu",
     "key_file": str(KEYDIR / "arli-glm47.key")},
    {"name": "deepseek", "model": "DeepSeek-V4-Flash-0731", "family": "DeepSeek",
     "key_file": str(KEYDIR / "arli-deepseek.key")},
    {"name": "qwen", "model": "Qwen3.5-27B-Anko", "family": "Qwen",
     "key_file": str(KEYDIR / "arli-qwen.key")},
]

TASKS = [
    {"task_id": "case-0002-resume-product-integration", "questions_file": "questions-resume-v0.yaml"},
    {"task_id": "case-0002-audit-r21-evidence-provenance", "questions_file": "questions-audit-v0.yaml"},
    {"task_id": "case-0002-assess-change-impact-on-oracle-topology", "questions_file": "questions-impact-v0.yaml"},
]

SCORER_VERSION = "o7.l1.deterministic-scorer/v1"

SYSTEM_PROMPT = (
    "You are answering questions about the current state of a software project. "
    "You are given a machine-generated context: a set of observations (each has an "
    "observation_id, a kind, a status, topics, and a statement) and relations between "
    "them. Rules:\n"
    "1. Answer ONLY from the provided context. Do not use outside knowledge and do not "
    "invent observation_ids.\n"
    "2. An observation describes the CURRENT active state only if its status is "
    "\"current\". Treat any observation whose status is not \"current\" (e.g. "
    "\"superseded\", \"rejected\", \"pending\") as NOT the current state.\n"
    "3. For each question, cite the exact observation_id(s) you relied on, and, when the "
    "question is about what is superseded or what depends on what, cite the relation(s) "
    "you relied on as {\"from\":<observation_id>,\"kind\":<relation kind>}.\n"
    "4. Respond with ONLY one JSON object of the exact shape shown under OUTPUT SHAPE, "
    "and nothing else -- no prose, no markdown fences."
)

OUTPUT_SHAPE = (
    '{"answers":[{"question_id":"<id>","answer":"<one or two sentences>",'
    '"cited_observation_ids":["obs-..."],'
    '"current_state_observation_ids":["obs-..."],'
    '"cited_relations":[{"from":"obs-...","kind":"..."}]}]}'
)


def sha(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


def load_questions(fname):
    q = yaml.safe_load((FX / fname).read_text())
    return q["questions"]


def raw_context_bytes(task_id):
    return (Q2 / "qodec-project-arm" / "tasks" / task_id / "context.json").read_bytes()


def qodec_context_bytes(task_id):
    return (Q2 / "q1rq-cells" / task_id / "encoded.q").read_bytes()


def notation_bytes():
    import subprocess
    return subprocess.run([str(QODEC_BIN), "notation"], capture_output=True, check=True).stdout


def questions_block(questions):
    lines = []
    for q in questions:
        lines.append(f'- question_id: {q["id"]}\n  text: {q["text"]}')
    return "\n".join(lines)


def build_user_prompt(arm, task_id, questions, raw_ctx, qodec_ctx, notation):
    qb = questions_block(questions)
    if arm == "RAW_VALID":
        ctx = raw_ctx.decode("utf-8")
        return (f"CONTEXT (JSON):\n{ctx}\n\n"
                f"QUESTIONS:\n{qb}\n\n"
                f"OUTPUT SHAPE:\n{OUTPUT_SHAPE}")
    else:
        note = notation.decode("utf-8")
        enc = qodec_ctx.decode("utf-8")
        return (f"{note}\n\n"
                f"CONTEXT (Qodec %q1 encoded -- decode it via the legend before answering):\n{enc}\n\n"
                f"QUESTIONS:\n{qb}\n\n"
                f"OUTPUT SHAPE:\n{OUTPUT_SHAPE}")


def o200k_tokens(text_bytes):
    """Self-contained (cold) token count via the frozen meter, codec identity."""
    import subprocess
    import tempfile
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as f:
        f.write(text_bytes)
        p = f.name
    try:
        o = subprocess.run([str(QODEC_BIN), "encode", "--codec", "identity",
                            "--meter", "o200k", "--json", "--input", p],
                           capture_output=True, check=True)
        return json.loads(o.stdout)["tokens_in"]
    finally:
        os.unlink(p)


def gold_status_map():
    g = json.loads((FX / "gold-state-v0.json").read_text())
    status = {o["observation_id"]: o.get("status") for o in g["observations"]}
    valid = set(status)
    relations = {(r["from"], r["kind"], r.get("to")) for r in g["relations"]}
    rel_from_kind = {(r["from"], r["kind"]) for r in g["relations"]}
    return status, valid, relations, rel_from_kind


def context_present_ids(raw_ctx_bytes):
    c = json.loads(raw_ctx_bytes)
    ids = {o["observation_id"] for o in c.get("selected", [])}
    return ids


# alias glyphs used by the %q1 legends across the three tasks (frozen: computed
# from the encoded.q legend lines). Any of these appearing in a model's
# structured output means it emitted undecoded Qodec -> representation parse
# failure. Detected regardless of arm (a RAW answer never contains them).
def legend_alias_glyphs():
    glyphs = set()
    for t in TASKS:
        enc = qodec_context_bytes(t["task_id"]).decode("utf-8", errors="ignore")
        # legend lines are between the first "%q1 <codec>" line and "%q1 body"
        lines = enc.splitlines()
        try:
            body_i = next(i for i, l in enumerate(lines) if l.startswith("%q1 body"))
        except StopIteration:
            body_i = len(lines)
        for l in lines[1:body_i]:
            if "=" in l:
                alias = l.split("=", 1)[0]
                for ch in alias:
                    if ord(ch) > 0x2000:  # non-ascii legend glyph
                        glyphs.add(ch)
    return glyphs


# ============================ deterministic scorer ============================

OBSID_RE = re.compile(r"^obs-[a-z0-9-]+$")


def parse_model_json(content):
    """Extract the single answer JSON object from the model content. Returns
    (obj, malformed_bool)."""
    if not content or not content.strip():
        return None, True
    txt = content.strip()
    # strip markdown fences if present
    txt = re.sub(r"^```(?:json)?\s*", "", txt)
    txt = re.sub(r"\s*```$", "", txt)
    # try direct
    for candidate in (txt, ):
        try:
            return json.loads(candidate), False
        except Exception:
            pass
    # fall back: last {...} block
    starts = [m.start() for m in re.finditer(r"\{", txt)]
    for s in starts:
        depth = 0
        for i in range(s, len(txt)):
            if txt[i] == "{":
                depth += 1
            elif txt[i] == "}":
                depth -= 1
                if depth == 0:
                    try:
                        return json.loads(txt[s:i + 1]), False
                    except Exception:
                        break
    return None, True


REFUSAL_MARKERS = ("i cannot", "i can't", "i'm unable", "cannot assist", "as an ai",
                   "i am unable", "insufficient information to")


def score_question(q, ans_obj, gold_status, gold_valid, gold_rel_from_kind,
                   present_ids, alias_glyphs):
    """Deterministic per-question score. `ans_obj` is the model's answer dict for
    this question (or None). Blind to arm."""
    req = set(q.get("required_observation_ids", []))
    forb = set(q.get("forbidden_stale_as_current", []))
    req_rel = {(r["from"], r["kind"]) for r in q.get("required_relation_paths", [])}

    s = {
        "answer_correct": False,
        "required_evidence_used": False,
        "required_relation_evidence_used": (len(req_rel) == 0),
        "missing_required_witness": True,
        "stale_as_current_error": False,
        "unsupported_or_fabricated_claim": False,
        "contradicts_gold": False,
        "representation_parse_failure": False,
        "refusal": False,
        "malformed_output": False,
    }
    if ans_obj is None:
        s["malformed_output"] = True
        return s

    cited = ans_obj.get("cited_observation_ids") or []
    current = ans_obj.get("current_state_observation_ids") or []
    cited_rel = ans_obj.get("cited_relations") or []
    answer_text = (ans_obj.get("answer") or "")
    blob = json.dumps(ans_obj, ensure_ascii=False)

    # refusal
    low = answer_text.lower()
    if any(m in low for m in REFUSAL_MARKERS):
        s["refusal"] = True

    # representation parse failure: undecoded alias glyph anywhere in structured output
    if any(g in blob for g in alias_glyphs):
        s["representation_parse_failure"] = True
    # or malformed id shapes in citations (garbled decode)
    cited_norm = [c for c in cited if isinstance(c, str)]
    if any(not OBSID_RE.match(c) for c in cited_norm):
        s["representation_parse_failure"] = True

    cited_set = set(cited_norm)
    # required evidence
    s["required_evidence_used"] = req.issubset(cited_set)
    s["missing_required_witness"] = not req.issubset(cited_set)
    # fabrication: cited an id that is not a real gold observation id
    fabricated = {c for c in cited_set if c not in gold_valid}
    if fabricated:
        s["unsupported_or_fabricated_claim"] = True
    # stale-as-current: a forbidden-stale id presented as current, OR any cited
    # "current_state" id whose gold status is not "current"
    current_norm = {c for c in current if isinstance(c, str)}
    if forb & current_norm:
        s["stale_as_current_error"] = True
    noncurrent_as_current = {c for c in current_norm
                             if c in gold_valid and gold_status.get(c) != "current"}
    if noncurrent_as_current:
        s["stale_as_current_error"] = True
        s["contradicts_gold"] = True
    # required relations
    if req_rel:
        cited_rel_set = {(r.get("from"), r.get("kind")) for r in cited_rel if isinstance(r, dict)}
        s["required_relation_evidence_used"] = req_rel.issubset(cited_rel_set)

    # overall correctness: deterministic conjunction
    s["answer_correct"] = (
        s["required_evidence_used"]
        and s["required_relation_evidence_used"]
        and not s["missing_required_witness"]
        and not s["stale_as_current_error"]
        and not s["unsupported_or_fabricated_claim"]
        and not s["contradicts_gold"]
        and not s["representation_parse_failure"]
        and not s["refusal"]
        and not s["malformed_output"]
    )
    return s


# ================================ freeze ================================

def cmd_freeze():
    OUT.mkdir(parents=True, exist_ok=True)
    notation = notation_bytes()
    binary = QODEC_BIN.read_bytes()
    gold_status, gold_valid, gold_rel, gold_rel_fk = gold_status_map()
    alias_glyphs = sorted(legend_alias_glyphs())

    # frozen arm-order schedule (recorded; alternating)
    schedule = {}
    for ti, t in enumerate(TASKS):
        for r in range(PROVIDER["repetitions"]):
            order = ["RAW_VALID", "QODEC_VALID"] if (ti + r) % 2 == 0 else ["QODEC_VALID", "RAW_VALID"]
            schedule[f"{t['task_id']}|rep{r}"] = order

    manifest = {
        "record": "o7.l1.representation-ab-freeze/v1",
        "purpose": ("Determine whether a real qualified model can use the cheaper Qodec "
                    "representation of an already-valid semantic context (case-0002 Q1R) "
                    "without material quality loss. Development A/B; not a generalization holdout."),
        "case_role": "development",
        "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "frozen_producers": FROZEN,
        "provider_surfaces": PROVIDER,
        "panel": PANEL,
        "system_prompt": SYSTEM_PROMPT,
        "system_prompt_sha256": sha(SYSTEM_PROMPT.encode()),
        "output_shape": OUTPUT_SHAPE,
        "scorer": {
            "version": SCORER_VERSION,
            "kind": "deterministic rule-based; no LLM judge",
            "axes": ["answer_correct", "required_evidence_used", "required_relation_evidence_used",
                     "missing_required_witness", "stale_as_current_error",
                     "unsupported_or_fabricated_claim", "contradicts_gold",
                     "representation_parse_failure", "refusal", "malformed_output"],
            "answer_correct_rule": ("required_evidence_used AND required_relation_evidence_used "
                                    "AND NOT missing_required_witness AND NOT stale_as_current_error "
                                    "AND NOT unsupported_or_fabricated_claim AND NOT contradicts_gold "
                                    "AND NOT representation_parse_failure AND NOT refusal AND NOT malformed_output"),
            "blind_to_arm": True,
            "alias_glyphs_frozen": alias_glyphs,
        },
        "acceptance_rule": {
            "hard_regression": {
                "any_new_stale_as_current_error": True,
                "any_new_fabricated_claim": True,
                "any_new_required_witness_loss": True,
                "meaning": ("a hard regression is any QODEC_VALID (task,question,replicate) cell "
                            "with stale_as_current_error / unsupported_or_fabricated_claim / "
                            "missing_required_witness that the paired RAW_VALID cell did not have"),
            },
            "quality_noninferior": {
                "no_hard_regression": True,
                "total_correctness_score_not_lower": True,
                "meaning": ("QODEC total answer_correct count over all cells >= RAW total, "
                            "and no hard regression"),
            },
        },
        "classification_rule": {
            "note": "evaluated in cmd_classify; thresholds frozen here before any call",
            "qodec_parse_failure_rate_threshold": 0.5,
            "decision_tree": [
                "if any attempt has model_reported != model_requested -> PROVIDER_QUALIFICATION_FAILED",
                "elif no successful attempts / all transport-refused -> LIVE_RUN_OPERATIONAL_REFUSAL",
                "elif qodec_parse_failure_rate >= 0.5 -> MODEL_CANNOT_USE_QODEC_REPRESENTATION",
                "elif hard_regression_count > 0 or qodec_correct < raw_correct -> MODEL_QUALITY_REGRESSION_DESPITE_STRUCTURAL_PASS",
                "elif qodec_correct > raw_correct and token_win -> MODEL_QUALITY_GAIN_WITH_TOKEN_WIN",
                "elif qodec_correct == raw_correct and token_win -> MODEL_PARITY_WITH_TOKEN_WIN",
                "else -> MODEL_PARITY_WITHOUT_TOKEN_WIN",
            ],
            "token_win_definition": "aggregate qodec cold_input_tokens (o200k) < raw cold_input_tokens (o200k)",
        },
        "gold_status_map": gold_status,
        "notation": {"sha256": sha(notation), "bytes": len(notation),
                     "tokens_o200k": o200k_tokens(notation)},
        "qodec_binary_sha256": sha(binary),
        "arm_order_schedule": schedule,
        "tasks": [],
    }

    for t in TASKS:
        tid = t["task_id"]
        questions = load_questions(t["questions_file"])
        raw_ctx = raw_context_bytes(tid)
        qodec_ctx = qodec_context_bytes(tid)
        present = sorted(context_present_ids(raw_ctx))
        raw_user = build_user_prompt("RAW_VALID", tid, questions, raw_ctx, qodec_ctx, notation)
        qodec_user = build_user_prompt("QODEC_VALID", tid, questions, raw_ctx, qodec_ctx, notation)
        # token accounting (self-contained/cold; o200k meter)
        raw_cold = o200k_tokens((SYSTEM_PROMPT + "\n" + raw_user).encode())
        qodec_cold = o200k_tokens((SYSTEM_PROMPT + "\n" + qodec_user).encode())
        # warm = cold minus the frozen notation tokens (notation amortized across a session)
        qodec_warm = qodec_cold - manifest["notation"]["tokens_o200k"]
        manifest["tasks"].append({
            "task_id": tid,
            "questions_file": t["questions_file"],
            "n_questions": len(questions),
            "question_ids": [q["id"] for q in questions],
            "context_present_observation_ids": present,
            "raw_context_sha256": sha(raw_ctx),
            "raw_context_bytes": len(raw_ctx),
            "qodec_encoded_sha256": sha(qodec_ctx),
            "qodec_encoded_bytes": len(qodec_ctx),
            "raw_user_prompt_sha256": sha(raw_user.encode()),
            "qodec_user_prompt_sha256": sha(qodec_user.encode()),
            "tokens_o200k": {
                "raw_cold_input": raw_cold,
                "qodec_cold_input": qodec_cold,       # notation charged
                "qodec_warm_input": qodec_warm,       # notation amortized
                "context_only_raw": o200k_tokens(raw_ctx),
                "context_only_qodec": o200k_tokens(qodec_ctx),
            },
        })

    (OUT / "l1-freeze-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    # also persist the exact frozen prompts (for byte-level audit)
    prompts_dir = OUT / "frozen-prompts"
    prompts_dir.mkdir(exist_ok=True)
    for t in TASKS:
        tid = t["task_id"]
        questions = load_questions(t["questions_file"])
        raw_ctx = raw_context_bytes(tid)
        qodec_ctx = qodec_context_bytes(tid)
        (prompts_dir / f"{tid}.RAW_VALID.user.txt").write_text(
            build_user_prompt("RAW_VALID", tid, questions, raw_ctx, qodec_ctx, notation))
        (prompts_dir / f"{tid}.QODEC_VALID.user.txt").write_text(
            build_user_prompt("QODEC_VALID", tid, questions, raw_ctx, qodec_ctx, notation))
    (prompts_dir / "system.txt").write_text(SYSTEM_PROMPT)
    (OUT / "notation.txt").write_bytes(notation)
    mb = (OUT / "l1-freeze-manifest.json").read_bytes()
    print("freeze written:", (OUT / "l1-freeze-manifest.json"))
    print("manifest sha256:", sha(mb))
    print("tasks:", [(tt["task_id"].split("-", 2)[-1], tt["n_questions"],
                      tt["tokens_o200k"]["raw_cold_input"],
                      tt["tokens_o200k"]["qodec_cold_input"],
                      tt["tokens_o200k"]["qodec_warm_input"]) for tt in manifest["tasks"]])
    print("alias glyphs frozen:", len(alias_glyphs))
    return 0


# ================================ run ================================

def _pm():
    sys.path.insert(0, str(CORPUS_TOOLS))
    sys.path.insert(0, str(PM_DIR))
    import provider_matrix
    return provider_matrix


def read_key():
    return KEY_FILE.read_text().strip()


def call_model(pm, key, model, system_text, user_text):
    """One attempt through the L0-qualified transport. Returns an accounting dict."""
    body_obj = {
        "model": model,
        "messages": [{"role": "system", "content": system_text},
                     {"role": "user", "content": user_text}],
        "temperature": PROVIDER["temperature"],
        "max_tokens": PROVIDER["max_tokens"],
        "stream": False,
    }
    body = json.dumps(body_obj).encode("utf-8")
    url = PROVIDER["endpoint_authority"]
    attempts = 0
    t0 = time.time()
    result = None
    for attempt in range(2):  # at most 1 retry on before-response
        attempts += 1
        result = pm.send_json(url, body, key, PROVIDER["timeout_s"])
        if result.status is not None or result.stage != "before-response":
            break
    latency = time.time() - t0
    rec = {
        "request_digest": sha(body),
        "http_status": result.status,
        "stage": result.stage,
        "attempt_count": attempts,
        "latency_s": round(latency, 3),
        "provider": PROVIDER["provider"],
        "model_requested": model,
        "qualification_identity": PROVIDER["qualification_identity"],
    }
    if result.status != 200 or not result.body:
        rec.update({"http_outcome": "non-200-or-empty", "response_digest": None,
                    "model_reported": None, "content": None, "reasoning_len": 0,
                    "finish_reason": None, "usage": {}, "detail": result.detail})
        return rec
    rec["response_digest"] = sha(result.body)
    try:
        parsed = json.loads(result.body)
    except Exception:
        rec.update({"http_outcome": "unparseable-response", "model_reported": None,
                    "content": None, "reasoning_len": 0, "finish_reason": None, "usage": {}})
        return rec
    ch = (parsed.get("choices") or [{}])[0]
    msg = ch.get("message", {}) or {}
    content = msg.get("content")
    reasoning = msg.get("reasoning_content") or msg.get("reasoning") or ""
    usage = parsed.get("usage", {}) or {}
    rec.update({
        "http_outcome": "completed",
        "model_reported": parsed.get("model"),
        "finish_reason": ch.get("finish_reason"),
        "content": content,
        "reasoning_len": len(reasoning) if isinstance(reasoning, str) else 0,
        "usage": {
            "prompt_tokens_provider": usage.get("prompt_tokens"),
            "completion_tokens_provider": usage.get("completion_tokens"),
            "total_tokens_provider": usage.get("total_tokens"),
        },
        "substitution": parsed.get("model") != model,
    })
    return rec


def run_one_model(pm, entry, manifest, gold_status, gold_valid, gold_rel_fk,
                  alias_glyphs, system_text, log):
    """Run the full A/B for one panel model. Writes per-model artifacts; returns
    (per-model receipt, attempts, cells)."""
    model = entry["model"]
    key = Path(entry["key_file"]).read_text().strip()
    mdir = OUT / "models" / entry["name"]
    mdir.mkdir(parents=True, exist_ok=True)
    (mdir / "attempts.jsonl").write_text("")
    (mdir / "responses").mkdir(exist_ok=True)
    attempts, cells = [], []
    N = PROVIDER["repetitions"]
    for ti, t in enumerate(TASKS):
        tid = t["task_id"]
        questions = load_questions(t["questions_file"])
        present = context_present_ids(raw_context_bytes(tid))
        for r in range(N):
            order = manifest["arm_order_schedule"][f"{tid}|rep{r}"]
            for arm in order:
                user_text = (OUT / "frozen-prompts" / f"{tid}.{arm}.user.txt").read_text()
                rec = call_model(pm, key, model, system_text, user_text)
                rec.update({"model_name": entry["name"], "task_id": tid, "arm": arm, "replicate": r})
                attempts.append(rec)
                with (mdir / "attempts.jsonl").open("a") as fh:
                    fh.write(json.dumps({k: v for k, v in rec.items() if k != "content"}) + "\n")
                (mdir / "responses" / f"{tid}.{arm}.rep{r}.txt").write_text(rec.get("content") or "")
                ans_obj, malformed = parse_model_json(rec.get("content"))
                by_qid = {}
                if isinstance(ans_obj, dict) and isinstance(ans_obj.get("answers"), list):
                    for a in ans_obj["answers"]:
                        if isinstance(a, dict) and a.get("question_id"):
                            by_qid[a["question_id"]] = a
                for q in questions:
                    s = score_question(q, by_qid.get(q["id"]), gold_status, gold_valid,
                                       gold_rel_fk, present, alias_glyphs)
                    cells.append({"model_name": entry["name"], "task_id": tid,
                                  "question_id": q["id"], "arm": arm, "replicate": r,
                                  "http_outcome": rec["http_outcome"],
                                  "model_reported": rec.get("model_reported"),
                                  "substitution": rec.get("substitution"), **s})
                log(f"[{entry['name']:8}] {tid.split('-',2)[-1]:30} rep{r} {arm:12} "
                    f"http={rec['http_outcome']:16} reported={str(rec.get('model_reported'))[:20]:20} "
                    f"lat={rec.get('latency_s')}s ptok={rec['usage'].get('prompt_tokens_provider')} "
                    f"ctok={rec['usage'].get('completion_tokens_provider')} "
                    f"correct={sum(1 for c in cells[-len(questions):] if c['answer_correct'])}/{len(questions)}")
    receipt = classify(manifest, attempts, cells, entry)
    (mdir / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    (mdir / "cells.json").write_text(json.dumps(cells, indent=2, sort_keys=True) + "\n")
    return receipt, attempts, cells


def cmd_run():
    manifest = json.loads((OUT / "l1-freeze-manifest.json").read_text())
    gold_status, gold_valid, gold_rel, gold_rel_fk = gold_status_map()
    alias_glyphs = legend_alias_glyphs()
    pm = _pm()
    system_text = (OUT / "frozen-prompts" / "system.txt").read_text()
    run_log = OUT / "l1-run-progress.log"
    run_log.write_text("")

    def log(m):
        with run_log.open("a") as fh:
            fh.write(m + "\n")
        print(m, flush=True)

    per_model = {}
    principal_receipt = None
    for entry in PANEL:
        log(f"==== model {entry['name']} ({entry['model']}, {entry['family']}) ====")
        receipt, _att, _cells = run_one_model(
            pm, entry, manifest, gold_status, gold_valid, gold_rel_fk,
            alias_glyphs, system_text, log)
        per_model[entry["name"]] = receipt
        if entry.get("principal"):
            principal_receipt = receipt

    # cross-family panel summary
    summary = {
        "record": "o7.l1.representation-ab-panel/v1",
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "freeze_manifest_sha256": sha((OUT / "l1-freeze-manifest.json").read_bytes()),
        "harness_sha256": sha(Path(__file__).read_bytes()),
        "principal_model": next(e["model"] for e in PANEL if e.get("principal")),
        "principal_classification": principal_receipt["principal_classification"] if principal_receipt else None,
        "per_model": {name: {
            "model": r["provider_surfaces_model"],
            "family": r["family"],
            "principal_classification": r["principal_classification"],
            "any_substitution": r["provider_integrity"]["any_substitution"],
            "RAW_correct": r["result"]["RAW_VALID"]["quality_answer_correct"],
            "QODEC_correct": r["result"]["QODEC_VALID"]["quality_answer_correct"],
            "n_cells": r["result"]["n_cells_per_arm"],
            "qodec_parse_failure_rate": r["result"]["QODEC_VALID"]["representation_parse_failure_rate"],
            "hard_regression_count": r["result"]["hard_regression_count"],
            "token_win_cold": r["result"]["token_win_cold"],
            "RAW_cold_tokens": r["result"]["RAW_VALID"]["cold_input_tokens_o200k"],
            "QODEC_cold_tokens": r["result"]["QODEC_VALID"]["cold_input_tokens_o200k"],
        } for name, r in per_model.items()},
    }
    (OUT / "l1-panel-summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    (OUT / "l1-receipt.json").write_text(json.dumps(principal_receipt, indent=2, sort_keys=True) + "\n")
    print("\n=== L1 PANEL SUMMARY ===")
    for name, m in summary["per_model"].items():
        print(f"  {name:9} {m['model']:24} RAW={m['RAW_correct']}/{m['n_cells']} "
              f"QODEC={m['QODEC_correct']}/{m['n_cells']} parsefail={m['qodec_parse_failure_rate']} "
              f"hardreg={m['hard_regression_count']} -> {m['principal_classification']}")
    print("principal:", summary["principal_model"], "->", summary["principal_classification"])
    return 0


def classify(manifest, attempts, cells, entry):
    # provider integrity
    any_substitution = any(a.get("substitution") for a in attempts if a["http_outcome"] == "completed")
    completed = [a for a in attempts if a["http_outcome"] == "completed"]
    thr = manifest["classification_rule"]["qodec_parse_failure_rate_threshold"]

    def arm_cells(arm):
        return [c for c in cells if c["arm"] == arm]
    raw_cells = arm_cells("RAW_VALID")
    qodec_cells = arm_cells("QODEC_VALID")
    raw_correct = sum(1 for c in raw_cells if c["answer_correct"])
    qodec_correct = sum(1 for c in qodec_cells if c["answer_correct"])
    n_cells = len(raw_cells)
    qodec_parse_fail = sum(1 for c in qodec_cells if c["representation_parse_failure"])
    qodec_parse_fail_rate = (qodec_parse_fail / len(qodec_cells)) if qodec_cells else 1.0

    # paired hard regression: index by (task,question,replicate)
    def key(c):
        return (c["task_id"], c["question_id"], c["replicate"])
    raw_by = {key(c): c for c in raw_cells}
    hard_regressions = []
    for c in qodec_cells:
        rk = raw_by.get(key(c))
        if rk is None:
            continue
        new_stale = c["stale_as_current_error"] and not rk["stale_as_current_error"]
        new_fab = c["unsupported_or_fabricated_claim"] and not rk["unsupported_or_fabricated_claim"]
        new_miss = c["missing_required_witness"] and not rk["missing_required_witness"]
        if new_stale or new_fab or new_miss:
            hard_regressions.append({"cell": key(c), "new_stale": new_stale,
                                     "new_fabricated": new_fab, "new_missing_witness": new_miss})

    # token win (cold, o200k, aggregate)
    raw_cold = sum(t["tokens_o200k"]["raw_cold_input"] for t in manifest["tasks"]) * PROVIDER["repetitions"]
    qodec_cold = sum(t["tokens_o200k"]["qodec_cold_input"] for t in manifest["tasks"]) * PROVIDER["repetitions"]
    qodec_warm = sum(t["tokens_o200k"]["qodec_warm_input"] for t in manifest["tasks"]) * PROVIDER["repetitions"]
    token_win = qodec_cold < raw_cold
    # provider-reported prompt tokens
    def ptok(arm):
        return sum((a["usage"].get("prompt_tokens_provider") or 0)
                   for a in completed if a["arm"] == arm)

    no_hard = len(hard_regressions) == 0
    noninferior = no_hard and (qodec_correct >= raw_correct)

    if any_substitution or not completed:
        principal = "PROVIDER_QUALIFICATION_FAILED" if any_substitution else "LIVE_RUN_OPERATIONAL_REFUSAL"
    elif qodec_parse_fail_rate >= thr:
        principal = "MODEL_CANNOT_USE_QODEC_REPRESENTATION"
    elif (not no_hard) or (qodec_correct < raw_correct):
        principal = "MODEL_QUALITY_REGRESSION_DESPITE_STRUCTURAL_PASS"
    elif qodec_correct > raw_correct and token_win:
        principal = "MODEL_QUALITY_GAIN_WITH_TOKEN_WIN"
    elif qodec_correct == raw_correct and token_win:
        principal = "MODEL_PARITY_WITH_TOKEN_WIN"
    else:
        principal = "MODEL_PARITY_WITHOUT_TOKEN_WIN"

    return {
        "record": "o7.l1.representation-ab-receipt/v1",
        "principal_classification": principal,
        "provider_surfaces_model": entry["model"],
        "family": entry["family"],
        "is_principal_model": bool(entry.get("principal")),
        "case_role": "development",
        "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "frozen_producers": manifest["frozen_producers"],
        "provider_surfaces": manifest["provider_surfaces"],
        "freeze_manifest_sha256": sha((OUT / "l1-freeze-manifest.json").read_bytes()),
        "harness_sha256": sha(Path(__file__).read_bytes()),
        "structural": {"Q1R": "PASS", "Q1RQ_roundtrip": "PASS",
                       "note": "structural verdicts inherited from Q2 (frozen); not re-run here"},
        "provider_integrity": {
            "attempts_total": len(attempts),
            "attempts_completed": len(completed),
            "any_substitution": any_substitution,
            "models_reported": sorted({a.get("model_reported") for a in completed if a.get("model_reported")}),
        },
        "result": {
            "n_cells_per_arm": n_cells,
            "RAW_VALID": {
                "quality_answer_correct": raw_correct,
                "quality_rate": round(raw_correct / n_cells, 4) if n_cells else None,
                "cold_input_tokens_o200k": raw_cold,
                "provider_prompt_tokens": ptok("RAW_VALID"),
                "completion_tokens": sum((a["usage"].get("completion_tokens_provider") or 0)
                                         for a in completed if a["arm"] == "RAW_VALID"),
            },
            "QODEC_VALID": {
                "quality_answer_correct": qodec_correct,
                "quality_rate": round(qodec_correct / n_cells, 4) if n_cells else None,
                "cold_input_tokens_o200k": qodec_cold,
                "warm_input_tokens_o200k": qodec_warm,
                "provider_prompt_tokens": ptok("QODEC_VALID"),
                "completion_tokens": sum((a["usage"].get("completion_tokens_provider") or 0)
                                         for a in completed if a["arm"] == "QODEC_VALID"),
                "representation_parse_failure_rate": round(qodec_parse_fail_rate, 4),
            },
            "cold": {"notation_charged": True},
            "warm": {"notation_amortized": True},
            "token_win_cold": token_win,
            "hard_regressions": hard_regressions,
            "hard_regression_count": len(hard_regressions),
            "quality_noninferior": noninferior,
        },
        "model_facing": {
            "representation_comprehension": ("FAIL" if qodec_parse_fail_rate >= thr else "PASS"),
            "quality_noninferiority": ("PASS" if noninferior else "FAIL"),
            "token_result": ("QODEC_CHEAPER_COLD" if token_win else "NO_TOKEN_WIN"),
        },
        "interpretation_allowed_if_supported": (
            "On case-0002, the qualified model answered from the Qodec representation with no "
            "material quality regression while using fewer input tokens." if principal in
            ("MODEL_PARITY_WITH_TOKEN_WIN", "MODEL_QUALITY_GAIN_WITH_TOKEN_WIN") else None),
        "prohibited_claims": ["Qodec improves LLMs generally", "Qodec generalizes to unseen real projects",
                              "qodec project v0 is sufficient under arbitrary budgets"],
    }


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "freeze"
    if cmd == "freeze":
        raise SystemExit(cmd_freeze())
    elif cmd == "run":
        raise SystemExit(cmd_run())
    else:
        print("unknown command", cmd); raise SystemExit(2)
