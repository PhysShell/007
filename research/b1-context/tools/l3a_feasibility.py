#!/usr/bin/env python3
"""L3A — model-free feasibility of a dictionary-free structural representation.

Answers one question and stops: does exact Q1R compact JSON contain a useful
reader-safe token win once phrase aliasing is forbidden? No model is called.

The arm under test is `qodec --codec jstruct`: an array of objects sharing a
key-set becomes a header plus rows, every scalar stays literal, and anything
that does not fit that shape stays canonical JSON. There is no legend, so there
is nothing for a reader to expand before it can inspect evidence -- which is
the property L1 and L2 measured the absence of.

Cold tokens are counted the way L1 and L2 counted them (system prompt + the
whole user prompt) so the number is comparable to the frozen 22.2% / 17.0%
already on record. The jstruct arm carries no notation block: unlike `%q1
deep`, it has no grammar to teach, only a one-line note naming the row shape.
"""
import hashlib
import json
import subprocess
import sys
import tempfile
import os
import re
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import l1_ab as L1  # noqa: E402

RB = Path("/home/tandem/007-c2/research/b1-context")
L2DIR = RB / "results" / "case-0002-l2-protected-representation-v1"
OUT = RB / "results" / "case-0002-l3a-jstruct-feasibility-v1"
QODEC = Path("/home/tandem/cargo-target/qodec-l2/release/qodec")

PRODUCER = {
    "repo": "PhysShell/qodec",
    "branch": "experiment/deep-protected-spans-v0",
    "exact_head": "0d5097a7142ae8e617c28aadc9dc6de70b1627f1",
    "exact_tree": "e5ec86fa7178dec668a29c4072945c95eca23a90",
    "codec": "jstruct",
}

GATE = {
    "aggregate_cold_saving_vs_RAW": ">= 5%",
    "every_task_not_worse_than_raw": True,
    "all_observation_identifiers_literal": True,
    "no_dictionary_or_legend": True,
    "roundtrip": "PASS",
}

# The one line of framing the arm needs. It names the row shape; it does not
# define a vocabulary and nothing has to be resolved before reading a value.
SHAPE_NOTE = ("CONTEXT (JSON, structurally compacted -- `@name[k1,k2,...]` is a header line and each "
              "following line is one record, cells in that key order, separated by |):")


def sha(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


def jstruct_encode(payload_path):
    out = subprocess.run(
        [str(QODEC), "encode", "--codec", "jstruct", "--meter", "o200k", "--json",
         "--input", str(payload_path)], capture_output=True, check=True)
    return json.loads(out.stdout)


def jstruct_decode(content):
    with tempfile.NamedTemporaryFile(suffix=".q", delete=False, mode="w") as f:
        f.write(content)
        p = f.name
    try:
        return subprocess.run([str(QODEC), "decode", "--input", p],
                              capture_output=True, check=True).stdout
    finally:
        os.unlink(p)


def has_legend(artifact_content):
    """A `%q1` legend lives between the header line and `%q1 body`."""
    head = artifact_content.split("%q1 body\n", 1)[0]
    return any(line and "=" in line for line in head.splitlines()[1:])


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    cells = OUT / "jstruct-cells"
    cells.mkdir(exist_ok=True)
    per_task = {}
    raw_cold = js_cold = 0
    lit_total = id_total = 0
    all_roundtrip = True
    any_legend = False

    for t in L1.TASKS:
        tid = t["task_id"]
        payload_path = L2DIR / "protected-spans" / f"{tid}.payload.json"
        payload = payload_path.read_bytes()
        env = jstruct_encode(payload_path)
        content = env["content"]
        (cells / f"{tid}.jstruct.q").write_text(content)

        back = jstruct_decode(content)
        roundtrip = back == payload
        all_roundtrip = all_roundtrip and roundtrip
        any_legend = any_legend or has_legend(content)

        text = payload.decode()
        ids = set(re.findall(r'"observation_id":"([^"]+)"', text))
        ids |= set(re.findall(r'"(?:from|to)":"([^"]+)"', text))
        literal = sum(1 for i in ids if i in content)
        lit_total += literal
        id_total += len(ids)

        questions = L1.load_questions(t["questions_file"])
        qb = L1.questions_block(questions)
        raw_ctx = L1.raw_context_bytes(tid).decode()
        raw_user = (f"CONTEXT (JSON):\n{raw_ctx}\n\nQUESTIONS:\n{qb}\n\n"
                    f"OUTPUT SHAPE:\n{L1.OUTPUT_SHAPE}")
        js_user = (f"{SHAPE_NOTE}\n{content}\n\nQUESTIONS:\n{qb}\n\n"
                   f"OUTPUT SHAPE:\n{L1.OUTPUT_SHAPE}")
        r = L1.o200k_tokens((L1.SYSTEM_PROMPT + "\n" + raw_user).encode())
        j = L1.o200k_tokens((L1.SYSTEM_PROMPT + "\n" + js_user).encode())
        raw_cold += r
        js_cold += j

        per_task[tid] = {
            "raw_cold_tokens": r,
            "jstruct_cold_tokens": j,
            "saving_fraction": round(1 - j / r, 4),
            "not_worse_than_raw": j < r,
            "context_only": {"raw_tokens": env["tokens_in"], "jstruct_tokens": env["tokens_out"],
                             "saving_fraction": round(1 - env["tokens_out"] / env["tokens_in"], 4)},
            "roundtrip_exact": roundtrip,
            "identifiers_literal": literal,
            "identifiers_total": len(ids),
            "jstruct_sha256": sha(content.encode()),
            "input_sha256": sha(payload),
        }
        print(f"{tid.split('-', 2)[-1][:32]:32} cold {r:5} -> {j:5}  {100 * (1 - j / r):5.2f}%   "
              f"ctx {env['tokens_in']:5} -> {env['tokens_out']:5}  "
              f"roundtrip={'EXACT' if roundtrip else 'FAIL'}  ids {literal}/{len(ids)}")

    saving = 1 - js_cold / raw_cold
    feasible = (saving >= 0.05
                and all(v["not_worse_than_raw"] for v in per_task.values())
                and lit_total == id_total
                and not any_legend
                and all_roundtrip)

    receipt = {
        "record": "o7.b1.l3a-jstruct-feasibility-receipt/v1",
        "purpose": ("Determine whether exact Q1R compact JSON contains a useful reader-safe "
                    "structural token win once phrase aliasing is forbidden. Model-free."),
        "case_role": "development", "holdout_evidence": False,
        "generalization_claim_allowed": False,
        "no_model_calls_made": True,
        "producer": PRODUCER,
        "producer_binary_sha256": sha(QODEC.read_bytes()),
        "frozen_inputs": {
            "source": "exact frozen Q1R contexts used by L1 and L2 (canonical, no trailing LF)",
            "l1_007_commit": "40edf6dc4ad6e96ecc81e7a71ce1b0765c369a3b",
            "b1_serialization_unchanged": True,
        },
        "gate": GATE,
        "RAW": {"aggregate_cold_tokens": raw_cold,
                "per_task_tokens": {k: v["raw_cold_tokens"] for k, v in per_task.items()}},
        "JSTRUCT": {
            "aggregate_cold_tokens": js_cold,
            "per_task_tokens": {k: v["jstruct_cold_tokens"] for k, v in per_task.items()},
            "identifier_literal_count": lit_total,
            "identifier_total_count": id_total,
            "roundtrip": "PASS" if all_roundtrip else "FAIL",
            "dictionary_or_legend_required": any_legend,
            "notation_block_required": False,
        },
        "saving_fraction": round(saving, 4),
        "context_only_saving_fraction": round(
            1 - sum(v["context_only"]["jstruct_tokens"] for v in per_task.values())
            / sum(v["context_only"]["raw_tokens"] for v in per_task.values()), 4),
        "per_task": per_task,
        "reference_l2_economics": {
            "RAW_cold": 8552, "FULL_ALIAS_cold": 6651, "PROTECTED_DEEP_cold": 7097,
            "note": "L1/L2 arms, same cold definition; both required a 261-token notation block",
        },
        "L3A_FEASIBLE": feasible,
        "verdict": ("L3A_FEASIBLE — stop and return for L3B authorization" if feasible
                    else "READER_REPRESENTATION_LINE_CLOSED"),
    }
    body = (json.dumps(receipt, indent=2, sort_keys=True) + "\n").encode()
    (OUT / "l3a-receipt.json").write_bytes(body)
    print(f"\nAGGREGATE cold RAW={raw_cold} JSTRUCT={js_cold} saving={saving * 100:.2f}%")
    print(f"identifiers literal {lit_total}/{id_total} | legend_required={any_legend} | "
          f"roundtrip={'PASS' if all_roundtrip else 'FAIL'}")
    print(f"L3A_FEASIBLE = {feasible}")
    print("receipt:", sha(body))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
