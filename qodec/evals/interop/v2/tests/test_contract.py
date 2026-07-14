"""Tests for the interop benchmark v2 contract validator.

These build a fully synthetic — but quota-complete — manifest in a temp
directory and drive ``validate_contract.py`` over it. No model, tokenizer
cache, qodec binary or RTK binary is required.

The synthetic generator is itself a proof that the frozen quotas in
``coverage-matrix.json`` are simultaneously satisfiable: 48 cases across 12
families and 240 rule-scored questions that meet every split, outcome, size,
origin, hazard, category and axis quota at once.
"""

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

V2_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = V2_DIR.parents[3]
sys.path.insert(0, str(V2_DIR))

import validate_contract as VC  # noqa: E402

FAMILIES = [
    "compiler-build", "test-runner", "lint-static-analysis", "search-listing",
    "git-diff-history", "exception-stacktrace", "application-ci-log",
    "dependency-package", "structured-data-query", "container-orchestrator",
    "network-api", "code-exploration-callgraph",
]
ECOS = ["dotnet", "rust", "python", "javascript-typescript", "go", "jvm", "language-neutral"]

CRITICAL = VC.CRITICAL_CATEGORIES
FAILMIX = VC.FAILURE_MIXED_OUTCOMES

MATCH = {
    "exact-retrieval": "exact", "locator": "ordered-path", "count": "numeric",
    "exact-set": "exact-set", "relation": "relation-set", "ordering": "ordered-path",
    "comparison": "boolean", "negative-evidence": "boolean", "causality": "contains-all",
    "actionability": "contains-all", "cross-section-synthesis": "contains-all",
}

FILL_ORDER = [
    "count", "exact-set", "relation", "ordering", "comparison",
    "negative-evidence", "causality", "actionability", "cross-section-synthesis",
]


def _hex(seed: str) -> str:
    return hashlib.sha256(seed.encode()).hexdigest()


def _outcome(f: int, slot: int) -> str:
    if slot == 0:
        return "success-clean" if f <= 7 else "warning-only"
    if slot == 1:
        return "single-failure" if f <= 7 else "multi-failure"
    if slot == 2:
        if f in (0, 1):
            return "warning-only"
        if f in (2, 3, 4, 5):
            return "multi-failure"
        return "mixed-warning-failure"
    # slot 3
    return "empty-or-no-match" if f <= 3 else "timeout-cancel-truncated-or-malformed"


def _size(f: int, slot: int) -> str:
    if slot == 0:
        return "xl" if f <= 7 else "medium"
    if slot == 1:
        return "tiny" if f <= 5 else "small"
    if slot == 2:
        return "medium" if f <= 7 else "small"
    return "large"


def _hazards(n: int) -> list[str]:
    table = [
        (range(0, 4), "duplicate-basename"),
        (range(4, 8), "windows-path"),
        (range(8, 12), "unicode"),
        (range(12, 16), "ansi"),
        (range(16, 20), "crlf"),
        (range(20, 24), "hostile-qodec-markers"),
        (range(24, 28), "conflicting-old-and-new-facts"),
        (range(28, 32), "sanitized-secret-like-values"),
        (range(32, 36), "nested-repetition"),
    ]
    for rng, hz in table:
        if n in rng:
            return [hz]
    rotation = [
        "posix-path", "spaces-or-quotes-in-path", "combining-characters",
        "alias-like-glyphs", "long-identifiers", "empty-result", "truncated-output",
        "binary-file-notice", "generated-or-minified-content", "carriage-return-progress",
        "posix-path", "long-identifiers",
    ]
    return [rotation[n - 36]]


def build_manifest(base_dir: Path) -> dict:
    """Construct a valid, quota-complete synthetic manifest under base_dir."""
    tok = {"name": "qwen2.5-coder-tok", "sha256": _hex("tok")}
    cases = []
    payload_root = base_dir / "payloads"
    payload_root.mkdir(parents=True, exist_ok=True)

    for f in range(12):
        for c in range(4):
            n = f * 4 + c
            fam = FAMILIES[f]
            cid = f"{fam}-{c}"
            split = ("public-development", "public-development", "public-validation", "sealed-heldout")[c]
            kind = "synthetic" if c == 2 else "real"
            hazards = _hazards(n)
            secret = "sanitized-secret-like-values" in hazards
            case = {
                "case_id": cid,
                "family": fam,
                "ecosystem": ECOS[n % 7],
                "tool": f"{fam}-tool",
                "outcome": _outcome(f, c),
                "size_bucket": _size(f, c),
                "structure": "sectioned-report",
                "origin": {
                    "kind": kind,
                    "source_description": f"capture for {cid}",
                    "sanitization": "synthetic-secrets-only" if secret else "none",
                    "source_sha256": _hex(f"src-{cid}"),
                    "generator_version": "gen-1.0.0",
                },
                "hazards": hazards,
                "tags": ["adversarial"] if kind == "synthetic" else ["primary"],
                "split": split,
                "payload_sha256": _hex(f"pl-{cid}"),
                "question_set_sha256": _hex(f"qs-{cid}"),
                "tokenizer_identity": tok,
            }
            if split != "sealed-heldout":
                rel = f"payloads/{cid}.txt"
                (payload_root / f"{cid}.txt").write_text(f"payload for {cid}\n")
                case["payload_path"] = rel
            cases.append(case)

    # ----- questions: budgeted category allocation ----- #
    budget = {
        "exact-retrieval": 12, "locator": 30, "count": 20, "exact-set": 20,
        "relation": 40, "ordering": 24, "comparison": 20, "negative-evidence": 30,
        "causality": 12, "actionability": 16, "cross-section-synthesis": 16,
    }
    per_case: dict[str, list[str]] = {c["case_id"]: [] for c in cases}

    def take(options):
        for opt in options:
            if budget[opt] > 0:
                budget[opt] -= 1
                return opt
        raise AssertionError(f"budget exhausted for {options}")

    # Phase A: mandatory picks for non-tiny cases.
    for case in cases:
        if case["size_bucket"] == "tiny":
            continue
        lst = per_case[case["case_id"]]
        lst.append(take(["locator", "exact-retrieval"]))          # exact/locator
        lst.append(take(["relation", "count", "comparison"]))      # relation/agg/comparison
        if case["outcome"] in FAILMIX:
            lst.append(take(["negative-evidence", "causality", "actionability"]))

    # Phase B: fill pool from remaining budget (er/loc already fully consumed).
    fill_pool: list[str] = []
    for cat in FILL_ORDER:
        fill_pool.extend([cat] * budget[cat])
        budget[cat] = 0
    assert sum(budget.values()) == 0

    # Phase C: pad every case to exactly 5 questions.
    fp = iter(fill_pool)
    for case in cases:
        lst = per_case[case["case_id"]]
        while len(lst) < 5:
            lst.append(next(fp))
    assert next(fp, None) is None, "fill pool not fully consumed"

    questions = []
    case_by_id = {c["case_id"]: c for c in cases}
    for cid, cats in per_case.items():
        sealed = case_by_id[cid]["split"] == "sealed-heldout"
        for i, cat in enumerate(cats):
            q = {
                "question_id": f"{cid}-q{i}",
                "case_id": cid,
                "category": cat,
                "field": "target-field",
                "match": MATCH[cat],
                "critical": cat in CRITICAL,
                "cross_section": i >= 3,               # 2 per case -> 96 total
                "disambiguation": i == 0,              # 1 per case  -> 48 total
                "absence_required": cat == "negative-evidence",
                "evidence_span_count": 1,
            }
            if not sealed:
                q["gold"] = f"gold-{cid}-{i}"
            questions.append(q)

    return {
        "contract_version": "interop-benchmark-v2",
        "cases": cases,
        "questions": questions,
    }


class ContractTestBase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.coverage = VC.load_json(V2_DIR / "coverage-matrix.json")
        self.manifest = build_manifest(self.base)

    def tearDown(self):
        self.tmp.cleanup()

    def run_validate(self, manifest=None, coverage=None):
        return VC.validate(manifest or self.manifest, coverage or self.coverage, self.base)


class TestValid(ContractTestBase):
    def test_valid_complete_contract_passes(self):
        violations = self.run_validate()
        self.assertEqual(violations, [], f"expected no violations, got:\n" + "\n".join(violations))

    def test_counts_are_exactly_as_designed(self):
        self.assertEqual(len(self.manifest["cases"]), 48)
        self.assertEqual(len(self.manifest["questions"]), 240)


class TestFailures(ContractTestBase):
    def _assert_code(self, violations, code):
        self.assertTrue(any(v.startswith(f"[{code}]") for v in violations),
                        f"expected a [{code}] violation, got:\n" + "\n".join(violations))

    def test_missing_family_fails(self):
        m = copy.deepcopy(self.manifest)
        m["cases"] = [c for c in m["cases"] if c["family"] != "network-api"]
        self._assert_code(self.run_validate(m), "family")

    def test_underfilled_heldout_split_fails(self):
        m = copy.deepcopy(self.manifest)
        # flip one sealed case to development: sealed=11, dev=25 -> both wrong
        for c in m["cases"]:
            if c["split"] == "sealed-heldout":
                c["split"] = "public-development"
                break
        self._assert_code(self.run_validate(m), "split")

    def test_duplicate_case_id_fails(self):
        m = copy.deepcopy(self.manifest)
        m["cases"][1]["case_id"] = m["cases"][0]["case_id"]
        self._assert_code(self.run_validate(m), "dup-case-id")

    def test_duplicate_question_id_fails(self):
        m = copy.deepcopy(self.manifest)
        m["questions"][1]["question_id"] = m["questions"][0]["question_id"]
        self._assert_code(self.run_validate(m), "dup-question-id")

    def test_bad_sha_fails(self):
        m = copy.deepcopy(self.manifest)
        m["cases"][0]["payload_sha256"] = "not-a-sha"
        self._assert_code(self.run_validate(m), "bad-sha")

    def test_sealed_gold_leakage_fails(self):
        m = copy.deepcopy(self.manifest)
        sealed_ids = {c["case_id"] for c in m["cases"] if c["split"] == "sealed-heldout"}
        for q in m["questions"]:
            if q["case_id"] in sealed_ids:
                q["gold"] = "leaked-secret-answer"
                break
        self._assert_code(self.run_validate(m), "sealed-gold")

    def test_sealed_payload_path_leakage_fails(self):
        m = copy.deepcopy(self.manifest)
        for c in m["cases"]:
            if c["split"] == "sealed-heldout":
                c["payload_path"] = "payloads/leaked.txt"
                break
        self._assert_code(self.run_validate(m), "sealed-leak")

    def test_question_category_quota_failure_is_reported(self):
        m = copy.deepcopy(self.manifest)
        # relabel every relation question -> comparison: relation quota (30) fails
        for q in m["questions"]:
            if q["category"] == "relation":
                q["category"] = "comparison"
                q["match"] = MATCH["comparison"]
                q["critical"] = False
        self._assert_code(self.run_validate(m), "question-category")

    def test_hazard_quota_failure_is_reported(self):
        m = copy.deepcopy(self.manifest)
        for c in m["cases"]:
            if "crlf" in c["hazards"]:
                c["hazards"] = ["posix-path"]
        self._assert_code(self.run_validate(m), "hazard")

    def test_gate_mutation_without_version_bump_fails(self):
        cov = copy.deepcopy(self.coverage)
        # record a prior result under the same contract_version with the CURRENT digest
        cov["results_ledger"] = [{
            "contract_version": cov["contract_version"],
            "gates_digest": VC.gates_digest(cov),
            "results_id": "l2-first-v2-run",
        }]
        # now move a numeric gate and re-stamp the self-consistency digest,
        # but WITHOUT bumping contract_version
        cov["quotas"]["outcome"]["success-clean"] = 9
        cov["gates_digest"] = VC.gates_digest(cov)
        self._assert_code(self.run_validate(coverage=cov), "gate-mutation")

    def test_gate_immutability_holds_when_version_bumped(self):
        cov = copy.deepcopy(self.coverage)
        old_digest = VC.gates_digest(cov)
        cov["results_ledger"] = [{
            "contract_version": "interop-benchmark-v2",
            "gates_digest": old_digest,
        }]
        # bump version AND change a gate -> old ledger entry no longer matches version
        cov["contract_version"] = "interop-benchmark-v2.1"
        cov["quotas"]["outcome"]["success-clean"] = 9
        cov["gates_digest"] = VC.gates_digest(cov)
        m = copy.deepcopy(self.manifest)
        m["contract_version"] = "interop-benchmark-v2.1"
        violations = self.run_validate(m, cov)
        self.assertFalse(any(v.startswith("[gate-mutation]") for v in violations),
                         "version bump should clear the gate-mutation violation")

    def test_orphan_question_fails(self):
        m = copy.deepcopy(self.manifest)
        m["questions"][0]["case_id"] = "does-not-exist"
        self._assert_code(self.run_validate(m), "orphan-question")

    def test_secret_hazard_requires_sanitization(self):
        m = copy.deepcopy(self.manifest)
        for c in m["cases"]:
            if "sanitized-secret-like-values" in c["hazards"]:
                c["origin"]["sanitization"] = "none"
        self._assert_code(self.run_validate(m), "origin")

    def test_missing_public_payload_path_fails(self):
        m = copy.deepcopy(self.manifest)
        for c in m["cases"]:
            if c["split"] != "sealed-heldout":
                c["payload_path"] = "payloads/nope-missing.txt"
                break
        self._assert_code(self.run_validate(m), "missing-path")


class TestGitignore(unittest.TestCase):
    def test_private_path_is_gitignored(self):
        gi = (REPO_ROOT / ".gitignore").read_text()
        self.assertIn("qodec/evals/interop/v2/private/", gi,
                      "private held-out path must be gitignored")


if __name__ == "__main__":
    unittest.main(verbosity=2)
