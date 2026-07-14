#!/usr/bin/env python3
"""Interop Benchmark v2 contract validator.

Validates a coverage manifest (cases + questions + optional tracked sealed
manifest) against the frozen contract in ``coverage-matrix.json`` and the JSON
schemas under ``schemas/``. It performs **no model calls**, needs no tokenizer
cache, no qodec binary and no RTK binary — it is pure structural / quota
validation over a manifest that already exists.

The frozen numeric gates, taxonomy and split policy live in
``coverage-matrix.json`` and are digest-protected: once v2 results appear, any
change to those numbers without a new ``contract_version`` is a violation
(section 2 of the contract — you do not move the goalposts after the ball is
already next to them).

Exit code is non-zero on any violation.

Usage::

    python validate_contract.py MANIFEST.json
    python validate_contract.py MANIFEST.json --coverage coverage-matrix.json --base-dir DIR
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

# Categories that may carry critical=true (section 9 "Critical categories").
CRITICAL_CATEGORIES = {
    "locator",
    "relation",
    "ordering",
    "negative-evidence",
    "actionability",
}
SUCCESS_OUTCOMES = {"success-clean", "warning-only"}
FAILURE_MIXED_OUTCOMES = {"single-failure", "multi-failure", "mixed-warning-failure"}
# Fields a tracked sealed case/question must never carry (content leakage).
SEALED_FORBIDDEN_CASE_FIELDS = ("payload", "payload_path")
SEALED_FORBIDDEN_QUESTION_FIELDS = ("gold", "text", "prompt", "question_text")


# --------------------------------------------------------------------------- #
# Tiny JSON-Schema subset validator (no external dependency).
# --------------------------------------------------------------------------- #
def _type_ok(value, typ: str) -> bool:
    if typ == "object":
        return isinstance(value, dict)
    if typ == "array":
        return isinstance(value, list)
    if typ == "string":
        return isinstance(value, str)
    if typ == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if typ == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if typ == "boolean":
        return isinstance(value, bool)
    if typ == "null":
        return value is None
    return True


def validate_schema(instance, schema: dict, path: str = "") -> list[str]:
    """Validate ``instance`` against the supported JSON-Schema subset."""
    errs: list[str] = []

    if "type" in schema and not _type_ok(instance, schema["type"]):
        errs.append(f"{path or '<root>'}: expected type {schema['type']}")
        return errs

    if "enum" in schema and instance not in schema["enum"]:
        errs.append(f"{path or '<root>'}: {instance!r} not in enum")

    if isinstance(instance, str):
        if "pattern" in schema and not re.search(schema["pattern"], instance):
            errs.append(f"{path or '<root>'}: {instance!r} fails pattern {schema['pattern']}")
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errs.append(f"{path or '<root>'}: shorter than minLength {schema['minLength']}")

    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errs.append(f"{path or '<root>'}: below minimum {schema['minimum']}")

    if isinstance(instance, dict):
        for req in schema.get("required", []):
            if req not in instance:
                errs.append(f"{path or '<root>'}: missing required field '{req}'")
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            for key in instance:
                if key not in props:
                    errs.append(f"{path or '<root>'}: additional property '{key}' not allowed")
        for key, subschema in props.items():
            if key in instance:
                errs.extend(validate_schema(instance[key], subschema, f"{path}.{key}" if path else key))

    if isinstance(instance, list):
        if schema.get("uniqueItems") and len(instance) != len({json.dumps(x, sort_keys=True) for x in instance}):
            errs.append(f"{path or '<root>'}: items not unique")
        item_schema = schema.get("items")
        if item_schema:
            for i, item in enumerate(instance):
                errs.extend(validate_schema(item, item_schema, f"{path}[{i}]"))

    return errs


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #
def canonical(obj) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def sha256_hex(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def load_json(path: Path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def gates_digest(coverage: dict) -> str:
    """Digest over the frozen gate/quota block only."""
    block = {k: coverage[k] for k in ("corpus", "quotas", "gates") if k in coverage}
    return sha256_hex(canonical(block))


# --------------------------------------------------------------------------- #
# Contract checks
# --------------------------------------------------------------------------- #
class Validator:
    def __init__(self, manifest: dict, coverage: dict, base_dir: Path):
        self.m = manifest
        self.cov = coverage
        self.base_dir = base_dir
        self.violations: list[str] = []
        self.cases = manifest.get("cases", [])
        self.questions = manifest.get("questions", [])
        self.case_by_id = {c.get("case_id"): c for c in self.cases}

    def fail(self, code: str, msg: str):
        self.violations.append(f"[{code}] {msg}")

    # -- schema ------------------------------------------------------------- #
    def check_schema(self):
        case_schema = load_json(SCRIPT_DIR / "schemas" / "case.schema.json")
        q_schema = load_json(SCRIPT_DIR / "schemas" / "question.schema.json")
        for c in self.cases:
            for e in validate_schema(c, case_schema, f"case {c.get('case_id','?')}"):
                self.fail("schema", e)
        for q in self.questions:
            for e in validate_schema(q, q_schema, f"question {q.get('question_id','?')}"):
                self.fail("schema", e)
        sm = self.m.get("sealed_manifest")
        if sm is not None:
            sm_schema = load_json(SCRIPT_DIR / "schemas" / "sealed-manifest.schema.json")
            for e in validate_schema(sm, sm_schema, "sealed_manifest"):
                self.fail("schema", e)

    # -- unique ids / sha --------------------------------------------------- #
    def check_unique_ids(self):
        seen = set()
        for c in self.cases:
            cid = c.get("case_id")
            if cid in seen:
                self.fail("dup-case-id", f"duplicate case_id {cid!r}")
            seen.add(cid)
        seenq = set()
        for q in self.questions:
            qid = q.get("question_id")
            if qid in seenq:
                self.fail("dup-question-id", f"duplicate question_id {qid!r}")
            seenq.add(qid)

    def check_sha_format(self):
        for c in self.cases:
            for f in ("payload_sha256", "question_set_sha256"):
                v = c.get(f, "")
                if not SHA256_RE.match(str(v)):
                    self.fail("bad-sha", f"case {c.get('case_id')} {f} not a sha256")
            src = c.get("origin", {}).get("source_sha256", "")
            if not SHA256_RE.match(str(src)):
                self.fail("bad-sha", f"case {c.get('case_id')} origin.source_sha256 not a sha256")
            tk = c.get("tokenizer_identity", {}).get("sha256", "")
            if not SHA256_RE.match(str(tk)):
                self.fail("bad-sha", f"case {c.get('case_id')} tokenizer_identity.sha256 not a sha256")

    # -- corpus / split ----------------------------------------------------- #
    def check_corpus_totals(self):
        corpus = self.cov["corpus"]
        if len(self.cases) < corpus["base_cases"]:
            self.fail("corpus", f"{len(self.cases)} cases < required {corpus['base_cases']}")
        if len(self.questions) < corpus["questions"]:
            self.fail("corpus", f"{len(self.questions)} questions < required {corpus['questions']}")

    def check_splits(self):
        corpus = self.cov["corpus"]
        counts = {}
        for c in self.cases:
            counts[c.get("split")] = counts.get(c.get("split"), 0) + 1
        for split, want in corpus["split"].items():
            got = counts.get(split, 0)
            if got != want:
                self.fail("split", f"split {split}: {got} cases, want exactly {want}")

    def check_family_balance(self):
        corpus = self.cov["corpus"]
        families = corpus["families"]
        per_family: dict[str, list] = {f: [] for f in families}
        for c in self.cases:
            fam = c.get("family")
            if fam not in per_family:
                self.fail("family", f"case {c.get('case_id')} unknown family {fam!r}")
                continue
            per_family[fam].append(c)
        for fam, cs in per_family.items():
            if len(cs) != corpus["cases_per_family"]:
                self.fail("family", f"family {fam}: {len(cs)} cases, want {corpus['cases_per_family']}")
            # per-family split shape
            sp = {}
            for c in cs:
                sp[c.get("split")] = sp.get(c.get("split"), 0) + 1
            for split, want in corpus["per_family_split"].items():
                if sp.get(split, 0) != want:
                    self.fail("family", f"family {fam}: split {split} has {sp.get(split,0)}, want {want}")
            # per-family composition guarantees
            outcomes = {c.get("outcome") for c in cs}
            if not (outcomes & SUCCESS_OUTCOMES):
                self.fail("family", f"family {fam}: no success case")
            if not (outcomes & FAILURE_MIXED_OUTCOMES):
                self.fail("family", f"family {fam}: no failure/mixed case")
            reals = [c for c in cs if c.get("origin", {}).get("kind") == "real"]
            if len(reals) < 2:
                self.fail("family", f"family {fam}: {len(reals)} real captures, want >= 2")
            adv_syn = [
                c for c in cs
                if c.get("origin", {}).get("kind") == "synthetic" and "adversarial" in c.get("tags", [])
            ]
            if not adv_syn:
                self.fail("family", f"family {fam}: no adversarial synthetic case")
            sizes = {c.get("size_bucket") for c in cs}
            if not (sizes & {"medium", "large", "xl"}):
                self.fail("family", f"family {fam}: no payload >= medium")
            if not (sizes & {"large", "xl"}):
                self.fail("family", f"family {fam}: no large/xl payload")

    # -- ecosystem ---------------------------------------------------------- #
    def check_ecosystem(self):
        eq = self.cov["quotas"]["ecosystem"]
        case_counts: dict[str, int] = {}
        for c in self.cases:
            eco = c.get("ecosystem")
            case_counts[eco] = case_counts.get(eco, 0) + 1
        used = set(case_counts)
        if len(used) < eq["min_ecosystems"]:
            self.fail("ecosystem", f"{len(used)} ecosystems used, want >= {eq['min_ecosystems']}")
        for mand in eq["mandatory"]:
            if case_counts.get(mand, 0) == 0:
                self.fail("ecosystem", f"mandatory ecosystem {mand} absent")
        for eco, n in case_counts.items():
            if eco != "language-neutral" and n < eq["min_per_non_neutral"]:
                self.fail("ecosystem", f"ecosystem {eco}: {n} cases < {eq['min_per_non_neutral']}")
        # share caps
        ncases = len(self.cases)
        for eco, n in case_counts.items():
            if ncases and n / ncases > eq["max_share"] + 1e-9:
                self.fail("ecosystem", f"ecosystem {eco}: {n}/{ncases} cases > {eq['max_share']:.0%}")
        q_counts: dict[str, int] = {}
        for q in self.questions:
            c = self.case_by_id.get(q.get("case_id"))
            if c:
                eco = c.get("ecosystem")
                q_counts[eco] = q_counts.get(eco, 0) + 1
        nq = len(self.questions)
        for eco, n in q_counts.items():
            if nq and n / nq > eq["max_share"] + 1e-9:
                self.fail("ecosystem", f"ecosystem {eco}: {n}/{nq} questions > {eq['max_share']:.0%}")

    # -- outcome / size quotas --------------------------------------------- #
    def _count_field(self, field: str) -> dict[str, int]:
        out: dict[str, int] = {}
        for c in self.cases:
            v = c.get(field)
            out[v] = out.get(v, 0) + 1
        return out

    def check_outcome_quota(self):
        got = self._count_field("outcome")
        for k, want in self.cov["quotas"]["outcome"].items():
            if got.get(k, 0) < want:
                self.fail("outcome", f"outcome {k}: {got.get(k,0)} < {want}")

    def check_size_quota(self):
        got = self._count_field("size_bucket")
        for k, want in self.cov["quotas"]["size"].items():
            if got.get(k, 0) < want:
                self.fail("size", f"size {k}: {got.get(k,0)} < {want}")

    def check_origin_quota(self):
        # Global origin sanity in addition to per-family (checked in family balance).
        real = sum(1 for c in self.cases if c.get("origin", {}).get("kind") == "real")
        syn = sum(1 for c in self.cases if c.get("origin", {}).get("kind") == "synthetic")
        if real < 2 * len(self.cov["corpus"]["families"]):
            self.fail("origin", f"only {real} real captures total")
        if syn < len(self.cov["corpus"]["families"]):
            self.fail("origin", f"only {syn} synthetic cases total")
        # secret sanitization
        for c in self.cases:
            if "sanitized-secret-like-values" in c.get("hazards", []):
                san = c.get("origin", {}).get("sanitization", "none")
                if san == "none" or not san:
                    self.fail("origin", f"case {c.get('case_id')} carries secret-like hazard but sanitization=none")

    # -- hazards ------------------------------------------------------------ #
    def check_hazard_quota(self):
        hq = self.cov["quotas"]["hazards"]

        def count_any(tags: set[str]) -> int:
            return sum(1 for c in self.cases if tags & set(c.get("hazards", [])))

        groups = {
            "duplicate-basename": {"duplicate-basename"},
            "windows-path": {"windows-path"},
            "unicode-or-combining": {"unicode", "combining-characters"},
            "ansi-or-progress-output": {"ansi", "carriage-return-progress"},
            "crlf": {"crlf"},
            "hostile-qodec-markers": {"hostile-qodec-markers"},
            "conflicting-old-and-new-facts": {"conflicting-old-and-new-facts"},
            "sanitized-secret-like-values": {"sanitized-secret-like-values"},
            "nested-repetition": {"nested-repetition"},
        }
        for key, want in hq.items():
            got = count_any(groups[key])
            if got < want:
                self.fail("hazard", f"hazard {key}: {got} cases < {want}")

    # -- question categories & axes ---------------------------------------- #
    def check_question_categories(self):
        qq = self.cov["quotas"]["question_category"]
        cat_counts: dict[str, int] = {}
        for q in self.questions:
            cat = q.get("category")
            cat_counts[cat] = cat_counts.get(cat, 0) + 1
        for key, want in qq.items():
            if "+" in key:
                got = sum(cat_counts.get(part, 0) for part in key.split("+"))
            else:
                got = cat_counts.get(key, 0)
            if got < want:
                self.fail("question-category", f"category {key}: {got} < {want}")

    def check_question_axes(self):
        ax = self.cov["quotas"]["question_axes"]
        cs = sum(1 for q in self.questions if q.get("cross_section"))
        dis = sum(1 for q in self.questions if q.get("disambiguation"))
        ab = sum(1 for q in self.questions if q.get("absence_required"))
        crit = sum(1 for q in self.questions if q.get("critical"))
        if cs < ax["cross_section"]:
            self.fail("axis", f"cross_section {cs} < {ax['cross_section']}")
        if dis < ax["disambiguation"]:
            self.fail("axis", f"disambiguation {dis} < {ax['disambiguation']}")
        if ab < ax["absence_required"]:
            self.fail("axis", f"absence_required {ab} < {ax['absence_required']}")
        if crit < ax["critical"]:
            self.fail("axis", f"critical {crit} < {ax['critical']}")
        for q in self.questions:
            if q.get("critical") and q.get("category") not in CRITICAL_CATEGORIES:
                self.fail("axis", f"question {q.get('question_id')} critical but category {q.get('category')} not critical-eligible")

    # -- per-case question requirements ------------------------------------ #
    def check_per_case_questions(self):
        by_case: dict[str, list] = {}
        for q in self.questions:
            by_case.setdefault(q.get("case_id"), []).append(q)
        for q in self.questions:
            if q.get("case_id") not in self.case_by_id:
                self.fail("orphan-question", f"question {q.get('question_id')} references unknown case {q.get('case_id')}")
        for c in self.cases:
            if c.get("size_bucket") == "tiny":
                continue  # tiny cases are exempt from the per-case minimums
            qs = by_case.get(c.get("case_id"), [])
            cid = c.get("case_id")
            if len(qs) < 5:
                self.fail("per-case", f"case {cid}: {len(qs)} questions < 5")
            cats = {q.get("category") for q in qs}
            if not (cats & {"exact-retrieval", "locator"}):
                self.fail("per-case", f"case {cid}: no exact-retrieval/locator question")
            if not (cats & {"relation", "count", "comparison"}):
                self.fail("per-case", f"case {cid}: no relation/aggregation/comparison question")
            if not any(q.get("cross_section") for q in qs):
                self.fail("per-case", f"case {cid}: no cross-section question")
            if c.get("outcome") in FAILURE_MIXED_OUTCOMES:
                if not (cats & {"negative-evidence", "causality", "actionability"}):
                    self.fail("per-case", f"case {cid}: failure/mixed case lacks negative-evidence/causality/actionability question")

    # -- sealed leakage ----------------------------------------------------- #
    def check_sealed_leakage(self):
        for c in self.cases:
            if c.get("split") != "sealed-heldout":
                continue
            for f in SEALED_FORBIDDEN_CASE_FIELDS:
                if f in c:
                    self.fail("sealed-leak", f"sealed case {c.get('case_id')} carries forbidden field '{f}'")
        for q in self.questions:
            c = self.case_by_id.get(q.get("case_id"))
            if c and c.get("split") == "sealed-heldout":
                for f in SEALED_FORBIDDEN_QUESTION_FIELDS:
                    if f in q:
                        self.fail("sealed-leak", f"sealed question {q.get('question_id')} carries forbidden field '{f}'")

    # -- public paths ------------------------------------------------------- #
    def check_public_paths(self):
        for c in self.cases:
            if c.get("split") == "sealed-heldout":
                continue
            p = c.get("payload_path")
            if p is None:
                continue
            target = (self.base_dir / p).resolve()
            if not target.exists():
                self.fail("missing-path", f"public case {c.get('case_id')} payload_path {p} does not exist")

    # -- gold presence ------------------------------------------------------ #
    def check_gold(self):
        for q in self.questions:
            c = self.case_by_id.get(q.get("case_id"))
            if c is None:
                continue
            if c.get("split") == "sealed-heldout":
                if "gold" in q:
                    self.fail("sealed-gold", f"sealed question {q.get('question_id')} exposes gold")
            else:
                if "gold" not in q:
                    self.fail("public-gold", f"public question {q.get('question_id')} missing gold")
                elif q.get("evidence_span_count", 0) < 1:
                    self.fail("ungrounded", f"public question {q.get('question_id')} has no evidence span")

    # -- gate immutability -------------------------------------------------- #
    def check_gate_immutability(self):
        cov = self.cov
        computed = gates_digest(cov)
        stored = cov.get("gates_digest")
        if stored is not None and stored != computed:
            self.fail("gate-digest", "coverage-matrix gates_digest does not match its own gate block")
        cv = cov.get("contract_version")
        for entry in cov.get("results_ledger", []):
            if entry.get("contract_version") == cv and entry.get("gates_digest") != computed:
                self.fail(
                    "gate-mutation",
                    f"gates changed under contract_version {cv!r} without a version bump "
                    f"(results already recorded digest {entry.get('gates_digest')})",
                )
        mcv = self.m.get("contract_version")
        if mcv is not None and mcv != cv:
            self.fail("contract-version", f"manifest contract_version {mcv!r} != coverage {cv!r}")

    # -- run all ------------------------------------------------------------ #
    def run(self) -> list[str]:
        self.check_schema()
        self.check_unique_ids()
        self.check_sha_format()
        self.check_corpus_totals()
        self.check_splits()
        self.check_family_balance()
        self.check_ecosystem()
        self.check_outcome_quota()
        self.check_size_quota()
        self.check_origin_quota()
        self.check_hazard_quota()
        self.check_question_categories()
        self.check_question_axes()
        self.check_per_case_questions()
        self.check_sealed_leakage()
        self.check_public_paths()
        self.check_gold()
        self.check_gate_immutability()
        return self.violations


def validate(manifest: dict, coverage: dict, base_dir: Path) -> list[str]:
    return Validator(manifest, coverage, base_dir).run()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Validate an interop benchmark v2 coverage manifest.")
    ap.add_argument("manifest", help="Path to the coverage manifest JSON.")
    ap.add_argument("--coverage", default=str(SCRIPT_DIR / "coverage-matrix.json"),
                    help="Path to coverage-matrix.json (frozen quotas).")
    ap.add_argument("--base-dir", default=None,
                    help="Base directory for resolving public payload_path (default: manifest dir).")
    args = ap.parse_args(argv)

    manifest_path = Path(args.manifest).resolve()
    manifest = load_json(manifest_path)
    coverage = load_json(Path(args.coverage).resolve())
    base_dir = Path(args.base_dir).resolve() if args.base_dir else manifest_path.parent

    violations = validate(manifest, coverage, base_dir)
    if violations:
        print(f"CONTRACT INVALID — {len(violations)} violation(s):", file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1
    print("CONTRACT VALID")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
