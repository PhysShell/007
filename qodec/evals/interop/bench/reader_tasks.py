"""Level-2 reader: build the three comprehension arms and score by rule.

Three arms per question, all seeing the same brief where present so quality is
comparable and only cost accounting differs:

  raw              tool-only text, no brief
  raw+brief        notation brief + tool-only text  (control: brief-as-distraction)
  encoded+brief    notation brief + qodec artifact

The model answers ONLY in the fixed JSON schema. Scoring is deterministic (no
LLM judge). Each question declares a `field` (which answer key), a `category`
(for the report), and a `match` mode:

  exact / exact-set   set equality, no extra identifiers allowed
  one-of              at least one gold value (extra *existing* identifiers ok)
  contains-all        every gold value present (extras ok)
  ordered-path        gold is an ordered subsequence of the answer's call_path

Exact file paths are matched in full — never by basename. Two integrity checks
ride along: invalid identifiers (a file/symbol/path element absent from the
source) and alias leakage (a full qodec legend alias copied into an answer
value — aliases are matched as whole strings, only those actually used in the
encoded body, and only against structured answer values).
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field as dc_field

ANSWER_SCHEMA = '{"facts": [], "files": [], "symbols": [], "call_path": [], "answer": ""}'

SYSTEM = (
    "You are a precise code-context reader. Answer ONLY about the provided text. "
    "Reply with a single JSON object and nothing else, using exactly these keys: "
    f"{ANSWER_SCHEMA}. Put integers as strings in \"answer\". Use exact identifiers "
    "and full paths copied verbatim from the text; never invent or abbreviate names. "
    "Do not include any decoding notation or alias glyphs in your answer."
)

CATEGORIES = ["fact", "count", "locator", "call_path", "actionability"]


def build_messages(arm: str, payload: str, brief: str, question: str) -> list[dict]:
    if arm == "raw":
        content = payload
    elif arm in ("raw+brief", "encoded+brief"):
        content = f"{brief}\n\n{payload}"
    else:  # pragma: no cover
        raise ValueError(f"unknown arm {arm!r}")
    user = f"CONTEXT:\n{content}\n\nQUESTION: {question}\n\nRespond with the JSON object only."
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}]


def parse_answer(text: str) -> dict | None:
    """Extract the JSON object from a model reply. Returns None when the reply
    has no parseable object (malformed JSON), which scoring treats as a miss and
    the runner flags for a repeat."""
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if not m:
        return None
    try:
        obj = json.loads(m.group(0))
        return obj if isinstance(obj, dict) else None
    except json.JSONDecodeError:
        return None


def _s(x) -> str:
    return str(x).strip()


def _norm(x) -> str:
    return re.sub(r"\s+", "", str(x)).lower()


def _list(answer: dict, key: str) -> list[str]:
    v = answer.get(key)
    if isinstance(v, list):
        return [_s(e) for e in v if isinstance(e, (str, int)) and _s(e)]
    if isinstance(v, str) and v.strip():
        return [_s(v)]
    return []


def _is_subsequence(needle: list[str], hay: list[str]) -> bool:
    it = iter(hay)
    return all(any(n == h for h in it) for n in needle)


def used_aliases(artifact: str) -> set[str]:
    """Full alias strings assigned by a `%q1` container (legend lines
    `alias=phrase`) that actually occur in the encoded body — never split into
    characters."""
    lines = artifact.splitlines()
    if not lines or not lines[0].startswith("%q1 "):
        return set()
    aliases: list[str] = []
    body_start = None
    for i, line in enumerate(lines[1:], start=1):
        if line.startswith("%q1 body"):
            body_start = i + 1
            break
        if "=" in line:
            aliases.append(line.split("=", 1)[0])
    body = "\n".join(lines[body_start:]) if body_start is not None else ""
    return {a for a in aliases if a and a in body}


@dataclass
class QuestionScore:
    id: str
    category: str
    correct: bool
    invalid_identifiers: list[str] = dc_field(default_factory=list)
    alias_leaks: list[str] = dc_field(default_factory=list)
    got: object = None


def _match(mode: str, gold: list[str], got: list[str]) -> bool:
    g = [_s(x) for x in gold]
    a = [_s(x) for x in got]
    if mode in ("exact", "exact-set"):
        return sorted(a) == sorted(g)          # no extra identifiers permitted
    if mode == "one-of":
        return bool(set(a) & set(g))
    if mode == "contains-all":
        return set(g) <= set(a)
    if mode == "ordered-path":
        return _is_subsequence(g, a)
    raise ValueError(f"unknown match mode {mode!r}")


def score_question(q: dict, answer: dict | None, *, source_text: str,
                   aliases: set[str]) -> QuestionScore:
    cat = q["category"]
    fld = q["field"]
    gold = q["gold"] if isinstance(q["gold"], list) else [q["gold"]]
    ans = answer or {}
    correct = False

    if cat == "count":
        raw = _s(ans.get("answer", ""))
        nums = re.findall(r"-?\d+", raw)
        correct = (len(nums) == 1 and nums[0] == _s(gold[0])) or (_norm(raw) == _norm(gold[0]))
        got = raw
    elif cat in ("fact", "actionability"):
        blob = _norm(json.dumps(ans.get("facts", []), ensure_ascii=False) + _s(ans.get("answer", "")))
        correct = all(_norm(x) in blob for x in gold)
        got = ans.get("facts") or ans.get("answer")
    else:  # locator / call_path
        got_list = _list(ans, fld)
        correct = _match(q["match"], gold, got_list)
        got = got_list

    # Invalid identifiers — presence in the source. Files and symbols are
    # matched in full (no basename fallback, per the exact-path rule). A
    # call-path step is a method reference; the source shows it method-first
    # (`<Self as CommandFactory>::command()`), so its final `::` segment counts.
    invalid = []
    for key in ("files", "symbols"):
        for x in _list(ans, key):
            if x not in source_text:
                invalid.append(x)
    for x in _list(ans, "call_path"):
        if x not in source_text and x.split("::")[-1] not in source_text:
            invalid.append(x)

    # Alias leakage — whole aliases used in the body, checked against structured
    # answer VALUES only (not the JSON punctuation).
    values = []
    for key in ("files", "symbols", "call_path", "facts"):
        values += _list(ans, key)
    values.append(_s(ans.get("answer", "")))
    leaks = [a for a in aliases for v in values if a and a in v]

    return QuestionScore(id=q["id"], category=cat, correct=correct,
                         invalid_identifiers=invalid, alias_leaks=leaks, got=got)


def load_tasks(path) -> list[dict]:
    obj = json.loads(path.read_text())
    out = []
    for c in obj["cases"]:
        for q in c["questions"]:
            out.append({**q, "case": c["case"]})
    return out
