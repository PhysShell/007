"""Level-2 reader: build the three comprehension arms and score answers by rule.

Three arms per question, all seeing the same brief where present so quality is
comparable and only cost accounting (cold vs warm) differs:

  raw              tool-only text, no brief
  raw+brief        notation brief + tool-only text  (control: brief-as-distraction)
  encoded+brief    notation brief + qodec artifact

The model must answer ONLY in the fixed JSON schema. Scoring is deterministic
(no LLM judge): exact numeric counts, set-superset match on files/symbols,
accepted substrings for facts — plus two integrity checks the design doc calls
for: invalid identifiers (a file/symbol not present in the source) and alias
leakage (a qodec legend glyph copied into the answer).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field

ANSWER_SCHEMA = '{"facts": [], "files": [], "symbols": [], "call_path": [], "answer": ""}'

SYSTEM = (
    "You are a precise code-context reader. Answer ONLY about the provided text. "
    "Reply with a single JSON object and nothing else, using exactly these keys: "
    f"{ANSWER_SCHEMA}. Put integers as strings in \"answer\". Use exact identifiers "
    "and paths copied from the text; never invent names. Do not include any "
    "decoding notation or alias glyphs in your answer."
)


def build_messages(arm: str, payload: str, brief: str, question: str) -> list[dict]:
    """Assemble the chat messages for one (arm, question)."""
    if arm == "raw":
        content = payload
    elif arm == "raw+brief":
        content = f"{brief}\n\n{payload}"
    elif arm == "encoded+brief":
        content = f"{brief}\n\n{payload}"
    else:  # pragma: no cover - guarded by caller
        raise ValueError(f"unknown arm {arm!r}")
    user = f"CONTEXT:\n{content}\n\nQUESTION: {question}\n\nRespond with the JSON object only."
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def parse_answer(text: str) -> dict:
    """Extract the JSON object from a model reply (tolerate chatter around it)."""
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if not m:
        return {}
    try:
        obj = json.loads(m.group(0))
        return obj if isinstance(obj, dict) else {}
    except json.JSONDecodeError:
        return {}


def _norm(s: str) -> str:
    return re.sub(r"\s+", "", str(s)).lower()


def legend_glyphs(artifact: str) -> set[str]:
    """The alias characters a `%q1` container assigns (legend lines `X=phrase`),
    which the reader must never copy into an answer."""
    glyphs: set[str] = set()
    lines = artifact.splitlines()
    in_header = lines and lines[0].startswith("%q1 ")
    for line in lines[1:] if in_header else []:
        if line.startswith("%q1 body"):
            break
        if "=" in line:
            alias = line.split("=", 1)[0]
            if alias and len(alias) <= 3:  # alias tokens are short (glyph/sigil)
                glyphs.update(alias)
    return glyphs


@dataclass
class QuestionScore:
    id: str
    type: str
    correct: bool
    invalid_identifiers: list[str] = field(default_factory=list)
    alias_leak: int = 0
    got: object = None


def score_question(q: dict, answer: dict, *, source_text: str, glyphs: set[str]) -> QuestionScore:
    qtype = q["type"]
    gold = q["gold"]
    correct = False
    got: object = None

    if qtype == "count":
        got = _norm(answer.get("answer", ""))
        # accept the gold integer appearing as the answer (exact) or as a bare
        # leading token; reject if any other integer is given instead.
        want = _norm(gold["answer"])
        nums = re.findall(r"\d+", str(answer.get("answer", "")))
        correct = (got == want) or (nums == [gold["answer"]])
    elif qtype in ("files", "symbols"):
        key = qtype
        want = {_norm(x) for x in gold[key]}
        have = {_norm(x) for x in answer.get(key, []) if isinstance(x, str)}
        correct = want.issubset(have)
        got = sorted(have)
    elif qtype == "facts":
        blob = _norm(json.dumps(answer.get("facts", [])) + str(answer.get("answer", "")))
        correct = all(_norm(x) in blob for x in gold["facts"])
        got = answer.get("facts")
    else:  # pragma: no cover
        raise ValueError(f"unknown question type {qtype!r}")

    # Integrity: any file/symbol the model returned that is NOT present verbatim
    # in the source text is an invented identifier.
    src = source_text
    invalid = []
    for key in ("files", "symbols"):
        for x in answer.get(key, []) or []:
            if isinstance(x, str) and x.strip() and x not in src and x.split("/")[-1] not in src:
                invalid.append(x)

    # Alias leakage: any legend glyph copied into the answer body.
    answer_blob = json.dumps(answer, ensure_ascii=False)
    leak = sum(answer_blob.count(g) for g in glyphs) if glyphs else 0

    return QuestionScore(id=q["id"], type=qtype, correct=correct,
                         invalid_identifiers=invalid, alias_leak=leak, got=got)


def load_tasks(path) -> dict[str, list[dict]]:
    obj = json.loads(path.read_text())
    return {c["case"]: c["questions"] for c in obj["cases"]}
