"""Offline decomposition of stable codec losses from a canonical Level-2 record.

Pure, deterministic, and dependency-free (stdlib only): NO model endpoint, NO
qodec subprocess, NO network. The source of truth is the immutable canonical run
directory — records.jsonl, meta.json, report.txt, stability, and the task
snapshot, all pinned by the run's own SHA256SUMS, which is verified before any
analysis. Everything here reasons from bytes already on disk.

A "loss" is reproduced from the scorer's own criterion — primary raw+brief
correct, primary encoded+brief incorrect, question stable under the full repeat
signature — not from a hardcoded list. Mechanism labels are attached only with
machine-readable evidence linking a concrete transform to a gold-bearing span;
when the record cannot distinguish a mechanism the label is "unresolved" rather
than an invented story.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

ARMS = ["raw", "raw+brief", "encoded+brief"]
# The real container header sits at column 0 as `%q1 <codec-name> ...` where the
# codec name is a lowercase word. The notation brief only mentions `%q1 <codec>`
# (literal angle brackets), `%q1 body`, and `%q1 xN` — none of which is
# `%q1 <lowercase-word>` other than `body`, which we exclude. This detects any
# codec (mine/tmpl/toon/grep/…) without enumerating them.
CONTAINER_RE = re.compile(r"^%q1 (?!body\b)([a-z][a-z0-9]*)\b", re.M)

MECHANISMS = {
    "alias-decoding", "identifier-or-path-aliasing", "structural-folding",
    "grouping-or-boundary-loss", "notation-ambiguity", "information-absent",
    "format-or-integrity", "mixed", "unresolved",
}


class CanonicalMismatch(RuntimeError):
    """A canonical file's SHA-256 does not match the run's own SHA256SUMS."""


class LossSetMismatch(RuntimeError):
    """The losses derived from records disagree with the canonical report."""


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha256_text(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


# --------------------------------------------------------------------------- #
# Canonical loading + integrity
# --------------------------------------------------------------------------- #

def verify_sha256sums(run_dir: Path) -> dict:
    """Recompute every hash in the run's SHA256SUMS and confirm it matches.
    Raises CanonicalMismatch on any drift or missing/extra file."""
    run_dir = Path(run_dir)
    sums = (run_dir / "SHA256SUMS").read_text(encoding="utf-8")
    expected = {}
    for line in sums.splitlines():
        line = line.strip()
        if not line:
            continue
        digest, rel = line.split(None, 1)
        expected[rel.lstrip("./")] = digest
    for rel, want in sorted(expected.items()):
        path = run_dir / rel
        if not path.exists():
            raise CanonicalMismatch(f"missing canonical file: {rel}")
        got = sha256_bytes(path.read_bytes())
        if got != want:
            raise CanonicalMismatch(f"{rel}: sha256 {got} != canonical {want}")
    return expected


def load_canonical(run_dir: Path) -> dict:
    run_dir = Path(run_dir)
    records = [json.loads(l) for l in (run_dir / "records.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    meta = json.loads((run_dir / "meta.json").read_text(encoding="utf-8"))
    snap = json.loads((run_dir / "snapshots" / "reader-tasks.json").read_text(encoding="utf-8"))
    tasks = {}
    for c in snap["cases"]:
        for q in c["questions"]:
            tasks[(c["case"], q["id"])] = {**q, "case": c["case"]}
    report = (run_dir / "report.txt").read_text(encoding="utf-8")
    return {"records": records, "meta": meta, "tasks": tasks, "report": report,
            "tasks_snapshot_sha256": sha256_bytes((run_dir / "snapshots" / "reader-tasks.json").read_bytes())}


# --------------------------------------------------------------------------- #
# Loss detection — reproduce the scorer criterion from records
# --------------------------------------------------------------------------- #

def _sig(r: dict) -> tuple:
    return (r["correct"], r.get("format_compliant", not r["malformed"]),
            tuple(sorted(set(r.get("alias_leaks") or []))),
            tuple(sorted(set(r.get("invalid_identifiers") or []))))


def _index(records: list[dict]) -> dict:
    idx: dict[tuple, dict] = {}
    for r in records:
        idx.setdefault((r["case"], r["question"], r["arm"]), {})[r["repeat"]] = r
    return idx


def unstable_questions(records: list[dict]) -> set:
    idx = _index(records)
    unstable = set()
    for (case, q, arm), reps in idx.items():
        if len(reps) > 1 and len({_sig(reps[k]) for k in reps}) > 1:
            unstable.add((case, q))
    return unstable


def stable_codec_losses(records: list[dict]) -> list[tuple]:
    """(case, question) pairs that are stable codec losses: raw+brief primary
    correct, encoded+brief primary incorrect, and stable across repeats."""
    idx = _index(records)
    unstable = unstable_questions(records)
    out = []
    for (case, q) in sorted({(r["case"], r["question"]) for r in records}):
        rb = idx.get((case, q, "raw+brief"), {}).get(0)
        eb = idx.get((case, q, "encoded+brief"), {}).get(0)
        if rb and eb and rb["correct"] and not eb["correct"] and (case, q) not in unstable:
            out.append((case, q))
    return out


def _report_stbl_loss_counts(report: str) -> dict:
    """Parse the canonical report's stbl_loss column for all/facts-counts/locator."""
    counts = {}
    for line in report.splitlines():
        m = re.match(r"^(all|facts/counts|locator|call_path|actionability)\s+"
                     r"\d+\s+\S+\s+\S+\s+\S+\s+\d+\s+\d+\s+(\d+)\s+\d+", line)
        if m:
            counts[m.group(1)] = int(m.group(2))
    return counts


def crosscheck_losses(losses: list[tuple], tasks: dict, report: str) -> None:
    """The derived loss set must match the canonical report's stable-loss counts
    (total and by category); otherwise abort rather than tell a different story."""
    counts = _report_stbl_loss_counts(report)
    facts = {"fact", "count"}
    by_cat = {"all": len(losses),
              "facts/counts": sum(1 for l in losses if tasks[l]["category"] in facts),
              "locator": sum(1 for l in losses if tasks[l]["category"] == "locator")}
    for key, got in by_cat.items():
        want = counts.get(key)
        if want is not None and got != want:
            raise LossSetMismatch(
                f"derived stable losses[{key}]={got} != canonical report stbl_loss={want}; "
                f"losses={losses}")


# --------------------------------------------------------------------------- #
# Prompt evidence — reconstruct system / brief / payload / artifact from requests
# --------------------------------------------------------------------------- #

def system_prompt(rec: dict) -> str:
    return rec["request"]["messages"][0]["content"]


def _user(rec: dict) -> str:
    return rec["request"]["messages"][1]["content"]


def _context_and_suffix(rec: dict) -> tuple[str, str]:
    u = _user(rec)
    body = u[len("CONTEXT:\n"):] if u.startswith("CONTEXT:\n") else u
    parts = body.split("\n\nQUESTION:", 1)
    context = parts[0]
    suffix = ("QUESTION:" + parts[1]) if len(parts) > 1 else ""
    return context, suffix


def question_suffix(rec: dict) -> str:
    return _context_and_suffix(rec)[1]


def brief_of(rec: dict) -> str:
    """The notation brief: text between CONTEXT and the container header."""
    context, _ = _context_and_suffix(rec)
    m = CONTAINER_RE.search(context)
    return context[:m.start()].rstrip("\n") if m else ""


def raw_payload_of(rec: dict) -> str:
    """The tool-only payload (no brief). For a raw arm this is the whole context;
    for a *+brief arm it is the context after the brief."""
    context, _ = _context_and_suffix(rec)
    m = CONTAINER_RE.search(context)
    if m:                          # encoded arm — strip up to the container start
        return context[m.start():]
    return context                 # raw arm — context is exactly the tool payload


def artifact_of(rec: dict) -> str | None:
    """The %q1 container the encoded arm shows the model, or None if absent."""
    context, _ = _context_and_suffix(rec)
    m = CONTAINER_RE.search(context)
    return context[m.start():] if m else None


# --------------------------------------------------------------------------- #
# Container parsing + legend decode (pure string ops; not a qodec call)
# --------------------------------------------------------------------------- #

class Container:
    def __init__(self, header: str, codec: str, legend: dict, body: str, used: set):
        self.header = header
        self.codec = codec
        self.legend = legend
        self.body = body
        self.used = used


def parse_container(artifact: str) -> Container:
    lines = artifact.split("\n")
    header = lines[0]
    codec = header.split()[1] if len(header.split()) > 1 else ""
    legend: dict[str, str] = {}
    body_idx = None
    for i, ln in enumerate(lines[1:], start=1):
        if ln.startswith("%q1 body"):
            body_idx = i + 1
            break
        if "=" in ln:
            a, phrase = ln.split("=", 1)
            if a:
                legend[a] = phrase
    body = "\n".join(lines[body_idx:]) if body_idx is not None else "\n".join(lines[1:])
    used = {a for a in legend if a and a in body}
    return Container(header=header, codec=codec, legend=legend, body=body, used=used)


def decode_via_legend(body: str, legend: dict, max_iter: int = 64) -> str:
    """Expand alias glyphs to their phrases to a fixpoint — the mental decode the
    notation brief tells the reader to perform. Aliases can nest, so iterate."""
    text = body
    for _ in range(max_iter):
        nxt = text
        for a, phrase in legend.items():
            if a in nxt:
                nxt = nxt.replace(a, phrase)
        if nxt == text:
            break
        text = nxt
    return text


# --------------------------------------------------------------------------- #
# Answer extraction
# --------------------------------------------------------------------------- #

def answer_value(rec: dict, task: dict):
    """What the scorer compares for this task's category/field."""
    ans = rec.get("answer_parsed") or {}
    cat, field = task["category"], task.get("field")
    if cat == "count":
        return ans.get("answer")
    if cat in ("fact", "actionability"):
        return {"facts": ans.get("facts"), "answer": ans.get("answer")}
    return ans.get(field)


# --------------------------------------------------------------------------- #
# Gold-span fate
# --------------------------------------------------------------------------- #

FATES = ("preserved_verbatim", "represented_by_alias", "structurally_rewritten",
         "absent_from_encoded_artifact", "ambiguous_after_encoding", "not_applicable")


def _aliases_covering(span: str, legend: dict) -> list[str]:
    """Candidate aliases whose (non-trivial) phrase is a substring of the span —
    the aliases through which the span is reconstructed in the body. Evidence,
    not proof of exact position, so it may list more than the minimal set."""
    return [a for a, phrase in sorted(legend.items())
            if len(phrase.strip()) >= 2 and phrase.strip() in span]


def _aliased_exactly(span: str, legend: dict) -> list[str]:
    """Aliases whose phrase IS exactly the span — the token is aliased even if it
    also appears verbatim elsewhere (the value_parser `错=ValueParser` case)."""
    return [a for a, phrase in sorted(legend.items()) if phrase.strip() == span]


def span_fate(span: str, artifact: str, decoded: str, legend: dict) -> dict:
    """Fate of one gold span in the encoded artifact, with evidence."""
    if not span:
        return {"span": span, "fate": "not_applicable", "aliases": []}
    if span in artifact:
        return {"span": span, "fate": "preserved_verbatim", "aliases": []}
    covering = _aliases_covering(span, legend)
    if span in decoded:
        return {"span": span, "fate": "represented_by_alias", "aliases": covering}
    # Basename / final segment survival for structural rewrite vs absence.
    tail = re.split(r"[/:]", span)[-1]
    if tail and (tail in artifact or tail in decoded):
        return {"span": span, "fate": "structurally_rewritten", "aliases": covering,
                "surviving_segment": tail}
    return {"span": span, "fate": "absent_from_encoded_artifact", "aliases": covering}


def locator_span_analysis(gold_path: str, artifact: str, decoded: str, legend: dict) -> dict:
    """Per the spec: full path, basename, symbol, enclosing relation, ordering."""
    base = re.split(r"[/:]", gold_path)[-1]
    prefix = gold_path[: len(gold_path) - len(base)]
    return {
        "full_path": span_fate(gold_path, artifact, decoded, legend)["fate"],
        "basename": span_fate(base, artifact, decoded, legend)["fate"],
        "path_prefix": span_fate(prefix.rstrip("/:"), artifact, decoded, legend)["fate"] if prefix.strip("/:") else "not_applicable",
        "prefix_aliases": _aliases_covering(prefix, legend) if prefix.strip("/:") else [],
    }


def count_span_analysis(gold_count: str, unit_hint: str, raw_payload: str, artifact: str) -> dict:
    """For counts: did the counted items / grouping survive, and is a competing
    count also literally present (visual collapse of distinct groups)?"""
    def line_with(tok):
        for ln in artifact.split("\n"):
            if tok and tok in ln and re.search(r"\d", ln):
                return ln.strip()
        return None
    gold_line = line_with(f"{gold_count} {unit_hint}".strip()) or line_with(gold_count)
    return {
        "gold_count": gold_count,
        "gold_count_line_in_artifact": gold_line,
        "gold_count_preserved_verbatim": bool(gold_line),
        "fold_markers_present": bool(re.search(r"(\[×\d+\]|%q1 x\d+)", artifact)),
    }


# --------------------------------------------------------------------------- #
# Mechanism labelling (evidence-linked)
# --------------------------------------------------------------------------- #

def _answer_glyphs(rec: dict, legend: dict) -> list[str]:
    blob = json.dumps(rec.get("answer_parsed") or {}, ensure_ascii=False)
    return sorted({a for a in legend if a and a in blob})


def _wrong_tokens(wrong) -> list[str]:
    if isinstance(wrong, list):
        return [str(w) for w in wrong if w]
    if isinstance(wrong, str) and wrong:
        return [wrong]
    return []


def _is_grep_body(artifact: str, container: Container) -> bool:
    """The tool payload is grep-style (file→`line:text` groups), regardless of the
    qodec codec that wrapped it — `mine` often carries grep output."""
    return bool(artifact) and ("»" in artifact or "mark=" in container.header
                               or re.search(r"^\d+:", container.body, re.M) is not None)


def label_mechanism(task: dict, spans: list[dict], eb0: dict, container: Container,
                    artifact: str, decoded: str, raw_payload: str) -> dict:
    cat = task["category"]
    legend = container.legend
    evidence: list[dict] = []
    secondary: list[str] = []
    leaked = _answer_glyphs(eb0, legend) or list(eb0.get("alias_leaks") or [])
    wrong = answer_value(eb0, task)
    wrong_tokens = _wrong_tokens(wrong)
    wrong_present = bool(wrong_tokens) and any(w in artifact for w in wrong_tokens)

    def add(kind, **kw):
        evidence.append({"kind": kind, **kw})

    # ---- counts / facts --------------------------------------------------- #
    if cat in ("fact", "count"):
        gold = str(task["gold"][0] if isinstance(task["gold"], list) else task["gold"])
        csa = count_span_analysis(gold, "", raw_payload, artifact)
        competitor = None
        if isinstance(wrong, str):
            for ln in artifact.split("\n"):
                if wrong and wrong in ln and re.search(r"\d", ln) and gold not in ln:
                    competitor = ln.strip()
                    break
        if csa["gold_count_preserved_verbatim"] and competitor:
            primary = "notation-ambiguity"
            add("gold_count_preserved_verbatim", value=gold, line=csa["gold_count_line_in_artifact"])
            add("competing_count_present_verbatim", value=str(wrong), line=competitor)
            if csa["fold_markers_present"]:
                secondary.append("structural-folding")
                add("fold_markers_present_in_artifact")
        elif not csa["gold_count_preserved_verbatim"] and csa["fold_markers_present"]:
            primary = "structural-folding"
            add("gold_count_absent_but_fold_markers_present", value=gold)
        elif not csa["gold_count_preserved_verbatim"]:
            primary = "information-absent"
            add("gold_count_absent_from_artifact", value=gold)
        else:
            primary = "unresolved"
        return _finish(primary, secondary, evidence, spans, csa)

    # ---- locators / symbols / paths --------------------------------------- #
    aliasing = [s for s in spans if s["fate"] == "represented_by_alias"]
    verbatim = [s for s in spans if s["fate"] == "preserved_verbatim"]
    absent = [s for s in spans if s["fate"] == "absent_from_encoded_artifact"]
    also_aliased = [(s["span"], a) for s in verbatim for a in _aliased_exactly(s["span"], legend)]
    grep_body = _is_grep_body(artifact, container)

    if grep_body and aliasing and not wrong_present:
        # aliased path segments AND a grep grouping the model could not attribute,
        # answering a file that is not even in the artifact — a fold×alias failure.
        primary = "mixed"
        secondary = ["identifier-or-path-aliasing", "grouping-or-boundary-loss", "alias-decoding"]
        for s in aliasing:
            add("gold_path_represented_by_alias", span=s["span"], candidate_aliases=s["aliases"][:4])
        add("grep_grouping_present", note="file→hits markers; gold path not a clean marker")
        add("model_answer_not_present_in_artifact", answer=wrong_tokens)
    elif grep_body and (aliasing or absent):
        # the model picked a different file marker that IS in the body — boundary
        # / attribution loss among the grep groups.
        primary = "grouping-or-boundary-loss"
        if aliasing:
            secondary.append("identifier-or-path-aliasing")
            for s in aliasing:
                add("gold_path_represented_by_alias", span=s["span"], candidate_aliases=s["aliases"][:4])
        if wrong_present:
            add("model_answer_is_a_different_present_file_marker", answer=wrong_tokens)
    elif also_aliased:
        # gold symbol appears verbatim but is ALSO aliased; the model conflated the
        # two and emitted the alias glyph.
        primary = "identifier-or-path-aliasing"
        for span, a in also_aliased:
            add("gold_identifier_also_aliased", span=span, alias=a, phrase=legend.get(a))
        if leaked:
            secondary += ["alias-decoding", "format-or-integrity"]
            add("alias_glyph_leaked_in_answer", glyphs=leaked)
    elif aliasing:
        primary = "identifier-or-path-aliasing"
        for s in aliasing:
            add("gold_span_represented_only_by_alias", span=s["span"], candidate_aliases=s["aliases"][:4])
        if leaked:
            secondary += ["alias-decoding", "format-or-integrity"]
            add("alias_glyph_leaked_in_answer", glyphs=leaked)
        elif not wrong_present and wrong_tokens:
            secondary.append("alias-decoding")
            add("model_decoded_to_value_not_in_artifact", answer=wrong_tokens)
    elif leaked:
        primary = "alias-decoding"
        secondary.append("format-or-integrity")
        add("alias_glyph_leaked_in_answer", glyphs=leaked)
    elif verbatim and wrong_present:
        primary = "notation-ambiguity"
        add("gold_preserved_verbatim_but_competitor_present", answer=wrong_tokens)
    elif absent:
        primary = "information-absent"
        for s in absent:
            add("gold_span_absent", span=s["span"])
    else:
        primary = "unresolved"
    return _finish(primary, secondary, evidence, spans, None)


def _finish(primary, secondary, evidence, spans, extra):
    sec = []
    for s in secondary:
        if s != primary and s not in sec:
            sec.append(s)
    out = {"primary_mechanism": primary, "secondary_mechanisms": sec, "evidence": evidence}
    if extra is not None:
        out["count_analysis"] = extra
    assert primary in MECHANISMS, primary
    return out


# --------------------------------------------------------------------------- #
# Dossiers
# --------------------------------------------------------------------------- #

import difflib  # noqa: E402


def _gold_list(task: dict) -> list[str]:
    g = task["gold"]
    return [str(x) for x in (g if isinstance(g, list) else [g])]


def alias_stats(eb: dict) -> dict:
    art = artifact_of(eb)
    if not art:
        return {"alias_count": 0, "alias_density_per_kchar": 0.0, "legend_size": 0}
    c = parse_container(art)
    body_len = max(1, len(c.body))
    return {"alias_count": len(c.used), "legend_size": len(c.legend),
            "alias_density_per_kchar": round(len(c.used) / body_len * 1000, 4)}


def _record_view(r: dict) -> dict:
    return {
        "arm": r["arm"], "repeat": r["repeat"],
        "correct": r["correct"], "format_compliant": r.get("format_compliant", not r["malformed"]),
        "malformed": r["malformed"], "alias_leaks": r.get("alias_leaks") or [],
        "invalid_identifiers": r.get("invalid_identifiers") or [],
        "answer_parsed": r.get("answer_parsed"),
        "prompt_tokens": r.get("server_prompt_tokens"),
        "completion_tokens": r.get("completion_tokens"),
        "latency_ms": r.get("total_ms"),
        "answer_sha256": sha256_text(r.get("answer_raw") or ""),
        "request_sha256": sha256_text(json.dumps(r["request"], ensure_ascii=False, sort_keys=True)),
    }


def _prompt_evidence(records_for_q: dict) -> dict:
    eb0 = records_for_q["encoded+brief"][0]
    raw0 = (records_for_q.get("raw") or {}).get(0) or records_for_q["raw+brief"][0]
    art = artifact_of(eb0)
    cont = parse_container(art) if art else None
    return {
        "system_prompt": system_prompt(eb0),
        "notation_brief": brief_of(eb0),
        "raw_tool_payload": raw_payload_of(raw0),
        "encoded_artifact": art,
        "alias_dictionary": cont.legend if cont else {},
        "used_aliases": sorted(cont.used) if cont else [],
        "question_suffix": question_suffix(eb0),
    }


def _line_diff(raw_payload: str, artifact: str) -> list[str]:
    return list(difflib.unified_diff(raw_payload.split("\n"), artifact.split("\n"),
                                     fromfile="raw", tofile="encoded", lineterm="", n=2))


def _gold_hunks(diff: list[str], gold: list[str]) -> list[str]:
    """Diff lines that touch a gold token — the relevant hunks for the summary."""
    return [ln for ln in diff if any(g and g.split('/')[-1] in ln or g in ln for g in gold)][:40]


def _by_arm(records: list[dict], case: str, q: str) -> dict:
    out: dict[str, dict] = {}
    for r in records:
        if r["case"] == case and r["question"] == q:
            out.setdefault(r["arm"], {})[r["repeat"]] = r
    return out


def _span_analysis(task: dict, arm_recs: dict) -> dict:
    eb0 = arm_recs["encoded+brief"][0]
    art = artifact_of(eb0) or ""
    cont = parse_container(art) if art else Container("", "", {}, "", set())
    decoded = decode_via_legend(cont.body, cont.legend)
    gold = _gold_list(task)
    spans = [span_fate(g, art, decoded, cont.legend) for g in gold]
    detail: dict = {"gold_spans": spans}
    if task["category"] == "locator":
        detail["locator_checks"] = [locator_span_analysis(g, art, decoded, cont.legend) for g in gold]
    if task["category"] in ("count", "fact"):
        detail["count_checks"] = count_span_analysis(gold[0], "", raw_payload_of(
            (arm_recs.get("raw") or {}).get(0) or arm_recs["raw+brief"][0]), art)
    return detail, spans, cont, art, decoded


def build_dossier(kind: str, case: str, q: str, records: list[dict], tasks: dict,
                  meta: dict, source: dict) -> dict:
    task = tasks[(case, q)]
    arm_recs = _by_arm(records, case, q)
    eb0 = arm_recs["encoded+brief"][0]
    raw0 = (arm_recs.get("raw") or {}).get(0) or arm_recs["raw+brief"][0]
    detail, spans, cont, art, decoded = _span_analysis(task, arm_recs)

    all_records = []
    for arm in ARMS:
        for rep in sorted(arm_recs.get(arm, {})):
            all_records.append(_record_view(arm_recs[arm][rep]))

    gold = _gold_list(task)
    diff = _line_diff(raw_payload_of(raw0), art)
    dossier = {
        "kind": kind,
        "identity": {
            "case": case, "question_id": q, "category": task["category"],
            "field": task.get("field"), "match_mode": task.get("match"),
            "question_text": task.get("q"), "gold": gold,
            "source_run": meta.get("run_id"), "source_commit": source.get("commit"),
            "records_sha256": source.get("records_sha256"),
            "model": meta.get("model_requested"), "qodec": meta.get("qodec_version"),
            "tokenizer_sha256": (meta.get("tokenizer") or {}).get("sha256"),
        },
        "answers": all_records,
        "prompt_evidence": _prompt_evidence(arm_recs),
        "span_analysis": detail,
        "alias_stats": alias_stats(eb0),
        "cost": {"encoded_prompt_tokens": eb0.get("server_prompt_tokens"),
                 "raw_brief_prompt_tokens": arm_recs["raw+brief"][0].get("server_prompt_tokens")},
        "diff_stats": {"added": sum(1 for l in diff if l.startswith("+") and not l.startswith("+++")),
                       "removed": sum(1 for l in diff if l.startswith("-") and not l.startswith("---"))},
        "gold_diff_hunks": _gold_hunks(diff, gold),
    }
    if kind == "loss":
        dossier["mechanism"] = label_mechanism(task, spans, eb0, cont, art, decoded, raw_payload_of(raw0))
    else:
        dossier["retained"] = True
    dossier["_full_diff"] = "\n".join(diff)   # split out to a per-case file by the CLI
    return dossier


# --------------------------------------------------------------------------- #
# Matched controls
# --------------------------------------------------------------------------- #

def both_correct_controls(records: list[dict], tasks: dict) -> list[tuple]:
    idx = _index(records)
    unstable = unstable_questions(records)
    out = []
    for (case, q) in sorted({(r["case"], r["question"]) for r in records}):
        rb = idx.get((case, q, "raw+brief"), {}).get(0)
        eb = idx.get((case, q, "encoded+brief"), {}).get(0)
        if not (rb and eb and rb["correct"] and eb["correct"] and (case, q) not in unstable):
            continue
        if eb["malformed"] or eb.get("alias_leaks") or eb.get("invalid_identifiers"):
            continue
        out.append((case, q))
    return out


def select_controls(losses: list[tuple], records: list[dict], tasks: dict) -> list[dict]:
    """Deterministic matched-control choice with recorded scores, so nobody can
    quietly hand-pick a flattering control. One control per loss, no reuse."""
    idx = _index(records)
    pool = both_correct_controls(records, tasks)
    chosen: set = set()
    result = []
    for loss in losses:
        lcase, lqid = loss
        lcat = tasks[loss]["category"]
        leb = idx[(lcase, lqid, "encoded+brief")][0]
        ltok = leb.get("server_prompt_tokens") or 0
        lst = alias_stats(leb)
        scored = []
        for cand in pool:
            if cand in chosen:
                continue
            ccat = tasks[cand]["category"]
            ceb = idx[(cand[0], cand[1], "encoded+brief")][0]
            cst = alias_stats(ceb)
            score = (0 if ccat == lcat else 1,
                     0 if cand[0] == lcase else 1,
                     abs((ceb.get("server_prompt_tokens") or 0) - ltok),
                     abs(cst["alias_count"] - lst["alias_count"]),
                     abs(cst["alias_density_per_kchar"] - lst["alias_density_per_kchar"]),
                     cand[0], cand[1])
            scored.append((score, cand))
        scored.sort(key=lambda x: x[0])
        if not scored:
            continue
        best_score, best = scored[0]
        chosen.add(best)
        result.append({
            "loss": {"case": lcase, "question_id": lqid, "category": lcat},
            "control": {"case": best[0], "question_id": best[1], "category": tasks[best]["category"]},
            "selection_score": {
                "same_category": best_score[0] == 0, "same_case": best_score[1] == 0,
                "encoded_token_diff": best_score[2], "alias_count_diff": best_score[3],
                "alias_density_diff": round(best_score[4], 4), "candidate_pool_size": len(scored),
            },
        })
    return result


# --------------------------------------------------------------------------- #
# Aggregate summary + honest conclusion
# --------------------------------------------------------------------------- #

ALIAS_MECHS = {"identifier-or-path-aliasing", "alias-decoding"}
FOLD_MECHS = {"structural-folding", "grouping-or-boundary-loss"}


def _conclusion(primaries: list[str], all_mechs: set) -> str:
    alias = bool(ALIAS_MECHS & all_mechs)
    fold = bool(FOLD_MECHS & all_mechs)
    absent = "information-absent" in all_mechs
    resolved = [p for p in primaries if p != "unresolved"]
    if not resolved:
        return "existing record cannot distinguish the mechanism"
    if alias and fold:
        return "evidence suggests fold × alias interaction"
    if alias:
        return "evidence implicates alias mining"
    if fold:
        return "evidence implicates structural folding"
    if absent:
        return "evidence shows actual information loss"
    return "existing record cannot distinguish the mechanism"


def _fate_share(dossiers: list[dict]) -> dict:
    tally = {f: 0 for f in FATES}
    total = 0
    for d in dossiers:
        for s in d["span_analysis"]["gold_spans"]:
            tally[s["fate"]] = tally.get(s["fate"], 0) + 1
            total += 1
    return {"total_gold_spans": total,
            "verbatim": tally["preserved_verbatim"], "alias_only": tally["represented_by_alias"],
            "structurally_rewritten": tally["structurally_rewritten"],
            "absent": tally["absent_from_encoded_artifact"]}


def summarize(loss_dossiers: list[dict], control_dossiers: list[dict],
              controls_map: list[dict], meta: dict) -> dict:
    primaries = [d["mechanism"]["primary_mechanism"] for d in loss_dossiers]
    all_mechs = set(primaries)
    for d in loss_dossiers:
        all_mechs |= set(d["mechanism"]["secondary_mechanisms"])

    def cat_of(d):
        c = d["identity"]["category"]
        return "facts/counts" if c in ("fact", "count") else c

    by_category: dict[str, int] = {}
    by_mechanism: dict[str, int] = {}
    mech_in_facts: set = set()
    mech_in_locator: set = set()
    for d in loss_dossiers:
        by_category[cat_of(d)] = by_category.get(cat_of(d), 0) + 1
        p = d["mechanism"]["primary_mechanism"]
        by_mechanism[p] = by_mechanism.get(p, 0) + 1
        (mech_in_facts if cat_of(d) == "facts/counts" else mech_in_locator).add(p)

    def alias_mean(ds):
        vals = [d["alias_stats"]["alias_count"] for d in ds]
        dens = [d["alias_stats"]["alias_density_per_kchar"] for d in ds]
        return {"alias_count_mean": round(sum(vals) / len(vals), 2) if vals else None,
                "alias_density_mean": round(sum(dens) / len(dens), 4) if dens else None}

    def tok_mean(ds):
        vals = [d["cost"]["encoded_prompt_tokens"] for d in ds if d["cost"]["encoded_prompt_tokens"]]
        return round(sum(vals) / len(vals), 1) if vals else None

    return {
        "source_run": meta.get("run_id"),
        "n_losses": len(loss_dossiers),
        "losses_by_category": by_category,
        "losses_by_primary_mechanism": by_mechanism,
        "mechanisms_in_facts_counts": sorted(mech_in_facts),
        "mechanisms_in_locator": sorted(mech_in_locator),
        "alias_losses_vs_controls": {"losses": alias_mean(loss_dossiers),
                                     "controls": alias_mean(control_dossiers)},
        "encoded_tokens_losses_vs_controls": {"losses": tok_mean(loss_dossiers),
                                              "controls": tok_mean(control_dossiers)},
        "gold_span_share_losses": _fate_share(loss_dossiers),
        "gold_span_share_controls": _fate_share(control_dossiers),
        "integrity": {
            "losses_with_alias_leaks": sum(1 for d in loss_dossiers
                                           if any(a["alias_leaks"] for a in d["answers"] if a["arm"] == "encoded+brief")),
            "losses_with_invalid_identifiers": sum(1 for d in loss_dossiers
                                                   if any(a["invalid_identifiers"] for a in d["answers"] if a["arm"] == "encoded+brief")),
            "losses_with_malformed_encoded": sum(1 for d in loss_dossiers
                                                 if any(a["malformed"] for a in d["answers"] if a["arm"] == "encoded+brief")),
        },
        "conclusion": _conclusion(primaries, all_mechs),
    }
