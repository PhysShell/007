//! Preregistration integrity for the closure redaction specimens.
//!
//! The specimens under `tests/fixtures/closure-redaction/` were authored for
//! `docs/architecture/closure-redaction-policy-v1.md` and committed with no
//! consumer. This file is their first consumer, added afterwards on purpose —
//! the same ordering used for the Step 0B corpus and the provenance specimens,
//! so a failure here is a discovery about committed evidence rather than
//! licence to nudge fixture and checker toward agreement.
//!
//! SCOPE — PREREGISTRATION INTEGRITY ONLY.
//!
//! HARD BOUNDARY 1 — this file is NOT a secret scanner. It never decides
//! whether a body contains a secret; it reads the outcome a specimen records
//! and checks the contract's structural consequences. Specimen R8 is
//! credential-shaped and carries no blocking finding, so a checker that quietly
//! grew its own heuristic fails there. The one place this file pattern-matches
//! (`no_live_credential_shapes`) is hygiene about what was committed here, and
//! feeds no gate decision.
//!
//! HARD BOUNDARY 2 — it maps no gate outcome to a closure state. There is
//! deliberately no `PASS`/`FINDING`/`OWED`/`CANNOT_CHECK`/`STALE` vocabulary.
//! Contract §10 derives the closure consequence from the surviving decision
//! basis; anticipating it here would rebuild the answer key provenance V1 §12
//! forbids.
//!
//! WHAT IT CANNOT CHECK — digest VALUES. Recomputing them needs JCS + SHA-256,
//! and this slice adds no dependency. Digests are verified out-of-tree by the
//! generator that produced them (rfc8785 0.1.4). Here they are checked only for
//! shape and for referential consistency between a record and its binding.
//!
//! NOTHING LOAD-BEARING IS TAKEN FROM THE SPECIMEN. The required field set
//! comes from `ALWAYS_FIELDS` / `PRESENT_ONLY_FIELDS` below, mirroring §5.3; coverage, the
//! retained/blocked partition and derived-fact admissibility are all computed.
//! Where a specimen declares one of these, the declaration is checked against
//! the computed value rather than used as its source. An earlier revision took
//! the denominator from the specimen, and the positive control then declared a
//! set narrower than the projection it retained.

// Same justification as `tests/github_fixture_integrity.rs`: every panic path
// below is this test's own assertion failing loudly. Nothing runs against
// production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const SPECIMENS: &[(&str, &str)] = &[
    ("safe-body-v1.json", "R1"),
    ("explicit-secret-v1.json", "R2"),
    ("whitespace-sensitive-secret-v1.json", "R3"),
    ("detector-failure-v1.json", "R4"),
    ("detector-inconclusive-v1.json", "R5"),
    ("derived-fact-blocked-v1.json", "R6"),
    ("safe-metadata-retained-v1.json", "R7"),
    ("token-shaped-safe-v1.json", "R8"),
    ("finding-with-incomplete-coverage-v1.json", "R9"),
    ("present-only-field-present-v1.json", "R10"),
    ("present-only-field-absent-v1.json", "R11"),
    ("multiple-findings-v1.json", "R12"),
];

const OUTCOMES: &[&str] = &["RETAIN", "BLOCK_SECRET", "CANNOT_ASSESS"];

/// Contract §5.3, mirrored: the always set and the present-only set per source
/// kind. The authority for "every field the projection would retain" — specimens
/// do not get a vote. All five gated kinds are mirrored; §11 records that the
/// specimens exercise three of them.
const ALWAYS_FIELDS: &[(&str, &[&str])] = &[
    (
        "github-issue-comment",
        &[
            "/author_association",
            "/body",
            "/created_at",
            "/id",
            "/updated_at",
            "/user/id",
            "/user/login",
            "/user/type",
        ],
    ),
    (
        "github-submitted-review",
        &[
            "/author_association",
            "/body",
            "/commit_id",
            "/id",
            "/state",
            "/submitted_at",
            "/user/id",
            "/user/login",
            "/user/type",
        ],
    ),
    (
        "github-review-comment",
        &[
            "/author_association",
            "/body",
            "/commit_id",
            "/created_at",
            "/id",
            "/original_commit_id",
            "/path",
            "/pull_request_review_id",
            "/updated_at",
            "/user/id",
            "/user/login",
            "/user/type",
        ],
    ),
    (
        "github-pull-request-head",
        &["/head/ref", "/head/repo/full_name", "/head/sha", "/number"],
    ),
    (
        "github-actions-check",
        &["/head_sha", "/id", "/name", "/status"],
    ),
];

const PRESENT_ONLY_FIELDS: &[(&str, &[&str])] = &[
    ("github-issue-comment", &[]),
    ("github-submitted-review", &[]),
    (
        "github-review-comment",
        &[
            "/in_reply_to_id",
            "/line",
            "/original_line",
            "/side",
            "/start_line",
        ],
    ),
    ("github-pull-request-head", &["/updated_at"]),
    (
        "github-actions-check",
        &["/completed_at", "/conclusion", "/started_at"],
    ),
];

/// Contract §7.2: pointers whose projected value is a string, not the raw number.
const ID_POINTERS: &[&str] = &[
    "/id",
    "/user/id",
    "/pull_request_review_id",
    "/in_reply_to_id",
];

/// Contract §7.2 via provenance V1 §8: the canonical path each decoded pointer
/// projects to. Used to rebuild the expected projection and compare it whole,
/// rather than hunting for values inside a rendered string.
const PROJECTION_PATH: &[(&str, &[&str])] = &[
    ("/id", &["stableId"]),
    ("/user/id", &["user", "id"]),
    ("/user/login", &["user", "login"]),
    ("/user/type", &["user", "type"]),
    ("/author_association", &["authorAssociation"]),
    ("/body", &["body"]),
    ("/created_at", &["createdAt"]),
    ("/updated_at", &["updatedAt"]),
    ("/state", &["state"]),
    ("/submitted_at", &["submittedAt"]),
    ("/commit_id", &["commitId"]),
    ("/original_commit_id", &["originalCommitId"]),
    ("/path", &["path"]),
    ("/pull_request_review_id", &["pullRequestReviewId"]),
    ("/in_reply_to_id", &["inReplyToId"]),
    ("/line", &["line"]),
    ("/original_line", &["originalLine"]),
    ("/side", &["side"]),
    ("/start_line", &["startLine"]),
];

/// Contract §7.3: which locator fields identify each kind.
const LOCATOR_SHAPE: &[(&str, &[&str])] = &[
    (
        "github-issue-comment",
        &["pullRequest", "repository", "stableId"],
    ),
    (
        "github-submitted-review",
        &["pullRequest", "repository", "stableId"],
    ),
    (
        "github-review-comment",
        &["pullRequest", "repository", "stableId"],
    ),
    ("github-actions-check", &["repository", "stableId"]),
    ("github-pull-request-head", &["pullRequest", "repository"]),
];

/// Contract §5.4 / §9.3 closed vocabulary.
const COVERAGE_FAILURE_CODES: &[&str] = &[
    "DETECTOR_UNAVAILABLE",
    "DETECTOR_FAILED",
    "INCOMPLETE_COVERAGE",
    "INVALID_RESULT",
];

/// Contract §9: the assessment schema is closed. Exactly these keys, and §9.4's
/// security argument is that none of them is free text.
const ASSESSMENT_ALWAYS: &[&str] = &[
    "schemaVersion",
    "sourceKind",
    "redactionPolicyVersion",
    "detector",
    "representation",
    "assessedFields",
    "coverageComplete",
    "outcome",
    "observedAt",
];
const ASSESSMENT_CONDITIONAL: &[&str] = &["findings", "coverageFailureCode"];

/// The rule ids covered by the specimens' bound detector configuration.
/// The specimens' synthetic detector configuration, and the rule ids it covers.
/// Keyed by digest so a record cannot claim one configuration and use another's
/// rules. The production form of this binding is OWED — contract §11 — but a
/// global list would not even hold the synthetic case.
const SYNTHETIC_CONFIG_DIGEST: &str =
    "sha256:191198e7a0b1017c1bba28dda161fc26973e7810333a323beeba8a7df12b9d7d";
const CONFIGURED_RULES: &[(&str, &[&str])] =
    &[(SYNTHETIC_CONFIG_DIGEST, &["SYN-TOKEN-1", "SYN-TOKEN-2"])];

fn rules_for(config_digest: &str) -> &'static [&'static str] {
    CONFIGURED_RULES
        .iter()
        .find(|(d, _)| *d == config_digest)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("no rule set is bound to detector configuration {config_digest}"))
}

const PROVENANCE_SOURCE_KINDS: &[&str] = &[
    "github-pull-request-head",
    "github-actions-check",
    "github-submitted-review",
    "github-review-comment",
    "github-issue-comment",
    "github-query-snapshot",
];

const REDUCED_KIND: &str = "github-reduced-source-record";
const ASSESSMENT_KIND: &str = "closure-retention-assessment";
const BINDING_KIND: &str = "closure-retention-binding";

/// Contract §9.5: nested objects are exact key sets too. A schema closed only at
/// its top level is not closed.
const DETECTOR_KEYS: &[&str] = &["configDigest", "id", "version"];
const FINDING_KEYS: &[&str] = &["field", "findingId"];
const REDUCED_RECORD_KEYS: &[&str] = &[
    "blockedFields",
    "coverageComplete",
    "locator",
    "locatorKind",
    "outcome",
    "redactionPolicyVersion",
    "retainedFields",
    "schemaVersion",
    "sourceKind",
];
const BINDING_KEYS: &[&str] = &[
    "assessmentDigest",
    "recordDigest",
    "schemaVersion",
    "sourceKind",
];

fn assert_exact_keys(label: &str, what: &str, object: &Value, expected: &[&str]) {
    let got: BTreeSet<&str> = object
        .as_object()
        .unwrap_or_else(|| panic!("{label}: {what} is not an object"))
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(
        got, want,
        "{label}: {what} is not the exact key set §9.5 freezes — a schema closed \
         only at its top level is not closed"
    );
}

const CLASSIFIER_VOCABULARY: &[&str] = &[
    "PASS",
    "FINDING",
    "OWED",
    "CANNOT_CHECK",
    "STALE",
    "expectedState",
    "headline",
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure-redaction")
}

fn load(file: &str) -> Value {
    let path = fixtures_dir().join(file);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"))
}

fn all_docs() -> Vec<(&'static str, Value)> {
    SPECIMENS.iter().map(|(f, _)| (*f, load(f))).collect()
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected string field {key:?} in {v}"))
}

fn strings(v: Option<&Value>) -> BTreeSet<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn table<'a>(t: &'a [(&str, &[&str])], kind: &str) -> &'a [&'a str] {
    t.iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, f)| *f)
        .unwrap_or_else(|| panic!("contract §5.3 has no entry for {kind:?}"))
}

/// Contract §5.3: always ∪ the present-only fields actually present. Computed
/// from the decoded source, never read from the specimen.
fn required_for(kind: &str, src: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = table(ALWAYS_FIELDS, kind)
        .iter()
        .map(|p| (*p).to_owned())
        .collect();
    for p in table(PRESENT_ONLY_FIELDS, kind) {
        if src.get(*p).is_some_and(|v| !v.is_null()) {
            out.insert((*p).to_owned());
        }
    }
    out
}

/// Contract §7.2: the value the complete projection would carry for `pointer`.
fn projected_value(pointer: &str, raw: &Value) -> Value {
    if ID_POINTERS.contains(&pointer) {
        let n = raw
            .as_i64()
            .unwrap_or_else(|| panic!("{pointer} should be a raw numeric id, got {raw}"));
        Value::String(n.to_string())
    } else {
        raw.clone()
    }
}

/// Rebuild the complete §8 projection the contract says these bytes produce.
fn expected_projection(kind: &str, src: &Value, required: &BTreeSet<String>) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("schemaVersion".into(), Value::from(1));
    out.insert("sourceKind".into(), Value::String(kind.to_owned()));
    for pointer in required {
        let path = PROJECTION_PATH
            .iter()
            .find(|(p, _)| p == pointer)
            .map(|(_, path)| *path)
            .unwrap_or_else(|| panic!("no projection path for {pointer}"));
        let raw = src
            .get(pointer)
            .unwrap_or_else(|| panic!("{pointer} required but absent from the source"));
        let value = projected_value(pointer, raw);
        match path {
            [key] => {
                out.insert((*key).to_owned(), value);
            }
            [parent, key] => {
                out.entry((*parent).to_owned())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()))
                    .as_object_mut()
                    .expect("nested projection object")
                    .insert((*key).to_owned(), value);
            }
            other => panic!("unsupported projection path {other:?}"),
        }
    }
    Value::Object(out)
}

/// Contract §5.5: unique, ascending lexical order.
fn assert_canonical_pointer_array(label: &str, field: &str, v: Option<&Value>) {
    let items: Vec<&str> = v
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{label}: {field} is not an array"))
        .iter()
        .map(|x| {
            x.as_str()
                .unwrap_or_else(|| panic!("{label}: {field} holds a non-string"))
        })
        .collect();
    let mut sorted = items.clone();
    sorted.sort_unstable();
    assert_eq!(
        items, sorted,
        "{label}: {field} is not in ascending order (§5.5)"
    );
    let unique: BTreeSet<&&str> = items.iter().collect();
    assert_eq!(
        unique.len(),
        items.len(),
        "{label}: {field} contains duplicates (§5.5)"
    );
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    let bounded = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    haystack.match_indices(needle).any(|(at, _)| {
        let before = haystack.get(..at).and_then(|s| s.chars().next_back());
        let after = haystack
            .get(at + needle.len()..)
            .and_then(|s| s.chars().next());
        bounded(before) && bounded(after)
    })
}

fn well_formed_digest(d: &str) -> bool {
    d.strip_prefix("sha256:").is_some_and(|h| {
        h.len() == 64
            && h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    })
}

/// One gate evaluation. Most specimens hold exactly one; R3 holds two variants
/// under a single locator, so a blocked variant is never judged against its
/// sibling's legitimate retained snapshot.
struct Unit {
    label: String,
    kind: String,
    scope: Value,
    outcome: String,
    assessment: Value,
    retention: Value,
    src: Value,
    reduced: Option<Value>,
    binding: Value,
    assessment_digest: String,
    derived_fact: Option<Value>,
}

fn units(file: &str, doc: &Value) -> Vec<Unit> {
    let kind = doc
        .get("sourceKind")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{file}: sourceKind missing"))
        .to_owned();
    let build = |label: String, scope: &Value, outer: &Value| Unit {
        label,
        kind: kind.clone(),
        scope: scope.clone(),
        outcome: str_at(scope, "gateOutcome").to_owned(),
        assessment: scope.get("assessment").expect("assessment").clone(),
        retention: scope.get("retention").expect("retention").clone(),
        src: scope.get("decodedSource").expect("decodedSource").clone(),
        reduced: scope.get("reducedSourceRecord").cloned(),
        binding: scope
            .get("retentionBinding")
            .expect("retentionBinding")
            .clone(),
        assessment_digest: str_at(scope, "assessmentDigest").to_owned(),
        derived_fact: scope
            .get("candidateDerivedFact")
            .or_else(|| outer.get("candidateDerivedFact"))
            .cloned(),
    };
    match doc.get("variants").and_then(Value::as_array) {
        Some(variants) => variants
            .iter()
            .map(|v| build(format!("{file}#{}", str_at(v, "variantId")), v, doc))
            .collect(),
        None => vec![build(file.to_owned(), doc, doc)],
    }
}

impl Unit {
    fn assessed(&self) -> BTreeSet<String> {
        strings(self.assessment.get("assessedFields"))
    }

    fn flagged(&self) -> BTreeSet<String> {
        self.assessment
            .get("findings")
            .and_then(Value::as_array)
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| f.get("field").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn required(&self) -> BTreeSet<String> {
        required_for(&self.kind, &self.src)
    }

    fn body(&self) -> String {
        self.src
            .get("/body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    /// Contract §7.1, computed here rather than read from the record.
    fn expected_partition(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let required = self.required();
        let assessed = self.assessed();
        let flagged = self.flagged();
        let blocked: BTreeSet<String> = required
            .iter()
            .filter(|p| flagged.contains(*p) || !assessed.contains(*p))
            .cloned()
            .collect();
        let retained = required.difference(&blocked).cloned().collect();
        (retained, blocked)
    }

    /// The pointers that still resolve to a retained record for this locator.
    fn resolvable(&self) -> BTreeSet<String> {
        match &self.reduced {
            None => self.required(),
            Some(r) => r
                .pointer("/canonical/retainedFields")
                .and_then(Value::as_object)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
        }
    }

    fn retained_digest(&self) -> String {
        match &self.reduced {
            None => str_at(&self.retention, "canonicalDigest").to_owned(),
            Some(r) => str_at(r, "canonicalDigest").to_owned(),
        }
    }
}

fn canonical_objects(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Object(map) => {
            if let Some(c) = map.get("canonical") {
                out.push(c.clone());
            }
            for value in map.values() {
                canonical_objects(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                canonical_objects(item, out);
            }
        }
        _ => {}
    }
}

#[test]
fn specimen_set_matches_the_preregistered_matrix() {
    let on_disk: BTreeSet<String> = fs::read_dir(fixtures_dir())
        .expect("reading the specimen directory")
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.ends_with(".json"))
        .collect();
    let preregistered: BTreeSet<String> = SPECIMENS.iter().map(|(f, _)| (*f).to_owned()).collect();
    assert_eq!(
        on_disk, preregistered,
        "the specimen directory drifted from the preregistered matrix"
    );
    assert!(
        fixtures_dir().join("README.md").is_file(),
        "README.md missing"
    );

    for (file, id) in SPECIMENS {
        let doc = load(file);
        assert_eq!(
            doc.pointer("/specimens/0").and_then(Value::as_str),
            Some(*id),
            "{file}: specimen id disagrees with the matrix"
        );
        assert_eq!(
            doc.get("synthetic").and_then(Value::as_bool),
            Some(true),
            "{file}: not marked synthetic"
        );
    }
}

#[test]
fn every_gate_outcome_is_exercised() {
    let mut seen = BTreeSet::new();
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            assert!(
                OUTCOMES.contains(&unit.outcome.as_str()),
                "{}: unknown gate outcome {:?}",
                unit.label,
                unit.outcome
            );
            seen.insert(unit.outcome);
        }
    }
    let expected: BTreeSet<String> = OUTCOMES.iter().map(|o| (*o).to_owned()).collect();
    assert_eq!(seen, expected, "not every gate outcome has a specimen");
}

#[test]
fn every_assessment_carries_detector_provenance() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let (a, l) = (&unit.assessment, &unit.label);
            for key in ["id", "version", "configDigest"] {
                let v = a
                    .pointer(&format!("/detector/{key}"))
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{l}: detector.{key} missing"));
                assert!(!v.is_empty(), "{l}: detector.{key} is empty");
            }
            assert!(
                well_formed_digest(
                    a.pointer("/detector/configDigest")
                        .and_then(Value::as_str)
                        .expect("configDigest")
                ),
                "{l}: detector.configDigest is not a well-formed digest"
            );
            assert_eq!(
                str_at(a, "representation"),
                "decoded-source-field-values",
                "{l}: the contract defines exactly one legal representation"
            );
            assert_eq!(
                a.get("redactionPolicyVersion").and_then(Value::as_u64),
                Some(1),
                "{l}: redactionPolicyVersion missing or wrong"
            );
            assert!(
                a.get("observedAt")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
                "{l}: observedAt missing"
            );
        }
    }
}

/// Contract §5.2 and §5.3: the denominator comes from the contract. A specimen
/// may declare it, and the declaration is checked, never trusted.
#[test]
fn coverage_is_computed_from_the_normative_field_set() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            let required = unit.required();
            let assessed = unit.assessed();
            assert!(
                assessed.is_subset(&required),
                "{l}: assessed a field the projection does not retain"
            );
            let computed = required.is_subset(&assessed);
            assert_eq!(
                unit.assessment
                    .get("coverageComplete")
                    .and_then(Value::as_bool),
                Some(computed),
                "{l}: coverageComplete disagrees with the normative field set"
            );
            assert_eq!(
                strings(unit.scope.get("requiredFields")),
                required,
                "{l}: the specimen's requiredFields disagrees with contract §5.3"
            );
            assert_canonical_pointer_array(
                l,
                "assessedFields",
                unit.assessment.get("assessedFields"),
            );
        }
    }
}

/// Contract §5.1: a blocking finding wins over incomplete coverage.
/// Contract §5.5: findings are unique on (field, findingId) and sorted by it.
/// The other two arrays are pointer lists; this one is a list of pairs.
#[test]
fn findings_are_canonically_ordered() {
    let mut multi = 0;
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let Some(findings) = unit.assessment.get("findings").and_then(Value::as_array) else {
                continue;
            };
            let keys: Vec<(&str, &str)> = findings
                .iter()
                .map(|f| {
                    (
                        f.get("field").and_then(Value::as_str).expect("field"),
                        f.get("findingId")
                            .and_then(Value::as_str)
                            .expect("findingId"),
                    )
                })
                .collect();
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            assert_eq!(
                keys, sorted,
                "{}: findings are not in (field, findingId) order (§5.5)",
                unit.label
            );
            let unique: BTreeSet<&(&str, &str)> = keys.iter().collect();
            assert_eq!(
                unique.len(),
                keys.len(),
                "{}: findings repeat a (field, findingId) pair (§5.5)",
                unit.label
            );
            if keys.len() > 1 {
                multi += 1;
            }
        }
    }
    assert!(
        multi > 0,
        "no specimen carries more than one finding, so the corpus cannot tell \
         canonical order from whatever a detector happened to emit"
    );
}

/// Contract §9.6: the retained assessment is the authority on its own outcome.
/// Anything outside it is an expectation, checked against it.
#[test]
fn the_retained_assessment_owns_the_outcome() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            assert_eq!(
                str_at(&unit.assessment, "outcome"),
                unit.outcome,
                "{l}: the retained assessment and the specimen disagree about the \
                 outcome — the retained bytes must be what the path followed"
            );
        }
    }
}

#[test]
fn outcome_follows_the_frozen_precedence() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            let complete = unit
                .assessment
                .get("coverageComplete")
                .and_then(Value::as_bool)
                .expect("coverageComplete");
            let expected = if !unit.flagged().is_empty() {
                "BLOCK_SECRET"
            } else if !complete {
                "CANNOT_ASSESS"
            } else {
                "RETAIN"
            };
            assert_eq!(
                str_at(&unit.assessment, "outcome"),
                expected,
                "{l}: the retained assessment's outcome disagrees with the §5.1 \
                 computation over its own findings and coverage"
            );
        }
    }
}

#[test]
fn a_failed_assessment_cannot_look_safe() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let (a, l) = (&unit.assessment, &unit.label);
            match unit.outcome.as_str() {
                "CANNOT_ASSESS" => {
                    assert!(
                        a.get("findings").is_none(),
                        "{l}: CANNOT_ASSESS must not carry findings — it measured nothing"
                    );
                }
                "BLOCK_SECRET" => {
                    let findings = a
                        .get("findings")
                        .and_then(Value::as_array)
                        .unwrap_or_else(|| panic!("{l}: BLOCK_SECRET without findings"));
                    assert!(
                        !findings.is_empty(),
                        "{l}: an empty findings list is indistinguishable from a clean assessment"
                    );
                    for f in findings {
                        for key in ["findingId", "field"] {
                            assert!(
                                f.get(key)
                                    .and_then(Value::as_str)
                                    .is_some_and(|s| !s.is_empty()),
                                "{l}: finding without {key}"
                            );
                        }
                        let id = f
                            .get("findingId")
                            .and_then(Value::as_str)
                            .expect("findingId");
                        let config = a
                            .pointer("/detector/configDigest")
                            .and_then(Value::as_str)
                            .expect("configDigest");
                        assert!(
                            rules_for(config).contains(&id),
                            "{l}: findingId {id:?} is not a rule of the configuration this \
                             assessment claims — an arbitrary string is not a closed value \
                             (§9, §9.4)"
                        );

                        // §9: a detector can only find something in a field it
                        // looked at. A finding naming an unassessed or
                        // non-required pointer blocks nothing while making the
                        // record read BLOCK_SECRET, and the genuinely dangerous
                        // field stays retainable.
                        let field = f.get("field").and_then(Value::as_str).expect("field");
                        assert!(
                            unit.required().contains(field),
                            "{l}: finding names {field:?}, which is not in the §5.3 required \
                             set — it can block nothing"
                        );
                        assert!(
                            unit.assessed().contains(field),
                            "{l}: finding names {field:?}, which this assessment never \
                             successfully assessed"
                        );
                        for key in [
                            "match", "matched", "excerpt", "sample", "value", "digest", "length",
                        ] {
                            assert!(
                                f.get(key).is_none(),
                                "{l}: finding carries {key:?} — findings must not reproduce the \
                                 matched bytes in any form"
                            );
                        }
                    }
                }
                "RETAIN" => {
                    assert!(
                        a.get("findings").is_none(),
                        "{l}: findings is present iff BLOCK_SECRET — an empty list and an \
                         absent key would be two encodings of one fact"
                    );
                    assert!(
                        a.get("coverageFailureCode").is_none(),
                        "{l}: RETAIN must not carry a coverage failure code"
                    );
                }
                other => panic!("{l}: unknown outcome {other:?}"),
            }
        }
    }
}

/// Contract §9 and §9.4: the assessment schema is closed, so there is nowhere
/// for a secret to be written. This replaces an earlier substring heuristic,
/// which admitted every secret shorter than its threshold and could not see a
/// field nobody had thought of.
#[test]
fn the_assessment_schema_is_closed() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            let keys: BTreeSet<&str> = unit
                .assessment
                .as_object()
                .expect("assessment object")
                .keys()
                .map(String::as_str)
                .collect();
            let always: BTreeSet<&str> = ASSESSMENT_ALWAYS.iter().copied().collect();
            let allowed: BTreeSet<&str> = always
                .union(&ASSESSMENT_CONDITIONAL.iter().copied().collect())
                .copied()
                .collect();
            assert!(
                always.is_subset(&keys),
                "{l}: assessment is missing a required field"
            );
            assert!(
                keys.is_subset(&allowed),
                "{l}: assessment carries {:?}, outside the closed schema — a field \
                 nobody constrained is where the secret ends up",
                keys.difference(&allowed).collect::<Vec<_>>()
            );

            // §9: domain separation, per provenance V1 §7.
            assert_eq!(
                str_at(&unit.assessment, "sourceKind"),
                ASSESSMENT_KIND,
                "{l}: the assessment must be domain-separated by its own sourceKind"
            );
            assert_eq!(
                str_at(&unit.binding, "sourceKind"),
                BINDING_KIND,
                "{l}: the binding must be domain-separated by its own sourceKind"
            );

            // §9.5: nested objects are exact key sets.
            assert_exact_keys(
                l,
                "detector",
                unit.assessment.get("detector").expect("detector"),
                DETECTOR_KEYS,
            );
            assert_exact_keys(l, "retentionBinding", &unit.binding, BINDING_KEYS);
            if let Some(findings) = unit.assessment.get("findings").and_then(Value::as_array) {
                for f in findings {
                    assert_exact_keys(l, "finding", f, FINDING_KEYS);
                }
            }
            if let Some(reduced) = &unit.reduced {
                let canonical = reduced.get("canonical").expect("canonical");
                // The ROOT of the reduced record, not only its nested objects.
                // An unknown root property is a way to carry a blocked value into
                // a durable canonical object without touching retainedFields.
                assert_exact_keys(l, "reduced source record", canonical, REDUCED_RECORD_KEYS);
                assert_exact_keys(
                    l,
                    "locator",
                    canonical.get("locator").expect("locator"),
                    table(LOCATOR_SHAPE, &unit.kind),
                );
            }

            // §9.5: the version fields are related, not merely present.
            for (what, object) in [
                ("assessment", &unit.assessment),
                ("retentionBinding", &unit.binding),
            ] {
                assert_eq!(
                    object.get("schemaVersion").and_then(Value::as_u64),
                    Some(1),
                    "{l}: {what} schemaVersion is not 1"
                );
            }
            if let Some(reduced) = &unit.reduced {
                let canonical = reduced.get("canonical").expect("canonical");
                assert_eq!(
                    canonical.get("schemaVersion").and_then(Value::as_u64),
                    Some(1),
                    "{l}: reduced record schemaVersion is not 1"
                );
                assert_eq!(
                    canonical.get("redactionPolicyVersion"),
                    unit.assessment.get("redactionPolicyVersion"),
                    "{l}: the record and its authorising assessment disagree on the \
                     policy version"
                );
            }
        }
    }
}

/// Contract §5.4: incomplete coverage names its reason, whatever the outcome.
#[test]
fn incomplete_coverage_names_its_reason() {
    let mut incomplete_with_finding = 0;
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let (a, l) = (&unit.assessment, &unit.label);
            let complete = a
                .get("coverageComplete")
                .and_then(Value::as_bool)
                .expect("coverageComplete");
            if complete {
                assert!(
                    a.get("coverageFailureCode").is_none(),
                    "{l}: complete coverage must not carry a failure code"
                );
                continue;
            }
            let code = a
                .get("coverageFailureCode")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{l}: incomplete coverage without a failure code"));
            assert!(
                COVERAGE_FAILURE_CODES.contains(&code),
                "{l}: coverageFailureCode {code:?} is outside the §5.4 vocabulary"
            );
            if !unit.flagged().is_empty() {
                incomplete_with_finding += 1;
            }
        }
    }
    assert!(
        incomplete_with_finding > 0,
        "no specimen witnesses §5.1's overlap — a blocking finding together with \
         incomplete coverage. Without one the corpus cannot tell the frozen \
         precedence from its inverse."
    );
}

/// Contract §5.3: the present-only rule is a computation over the decoded
/// source, and the corpus witnesses it in both directions.
#[test]
fn present_only_fields_join_the_set_only_when_present() {
    let present = load("present-only-field-present-v1.json");
    let absent = load("present-only-field-absent-v1.json");
    let (pu, au) = (
        units("present-only-field-present-v1.json", &present),
        units("present-only-field-absent-v1.json", &absent),
    );
    let (p, a) = (pu.first().expect("R10"), au.first().expect("R11"));
    assert_eq!(p.kind, a.kind, "the pair must share a source kind");

    let optional = "/in_reply_to_id";
    assert!(
        p.src.get(optional).is_some() && a.src.get(optional).is_none(),
        "the pair must differ by exactly the presence of {optional}"
    );
    assert!(
        p.required().contains(optional),
        "a present present-only field must join the required set"
    );
    assert!(
        !a.required().contains(optional),
        "an absent present-only field must stay out of the required set"
    );
    assert_eq!(
        p.required().len(),
        a.required().len() + 1,
        "the required sets must differ by exactly that one field"
    );
}

#[test]
fn a_blocked_source_produces_no_provenance_snapshot() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            let permitted = unit
                .retention
                .get("permitted")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| panic!("{l}: retention.permitted missing"));
            assert_eq!(
                permitted,
                unit.outcome == "RETAIN",
                "{l}: retention permission disagrees with the gate outcome"
            );

            if permitted {
                let canonical = unit.retention.get("canonical").expect("canonical");
                assert_eq!(
                    str_at(canonical, "sourceKind"),
                    unit.kind,
                    "{l}: a retained snapshot must use the locator's provenance kind"
                );
                assert!(
                    well_formed_digest(str_at(&unit.retention, "canonicalDigest")),
                    "{l}: retained snapshot without a well-formed digest"
                );
                assert!(
                    unit.reduced.is_none(),
                    "{l}: RETAIN needs no reduced record"
                );
                // §7.2, whole-object: the projection is exactly what the decoded
                // source produces under the frozen mapping. Checking the key set
                // alone would let a correct pointer name incorrect bytes.
                assert_eq!(
                    canonical,
                    &expected_projection(&unit.kind, &unit.src, &unit.required()),
                    "{l}: the retained projection is not what §7.2 says these bytes produce"
                );
            } else {
                assert!(
                    unit.retention.get("canonical").is_none()
                        && unit.retention.get("canonicalDigest").is_none(),
                    "{l}: a blocked source must not carry a snapshot or its digest"
                );
                assert!(
                    unit.reduced.is_some(),
                    "{l}: a non-RETAIN outcome needs a reduced source record"
                );
            }

            let mut in_scope = Vec::new();
            canonical_objects(&unit.scope, &mut in_scope);
            for canonical in &in_scope {
                if let Some(k) = canonical.get("sourceKind").and_then(Value::as_str) {
                    let legal = if permitted {
                        unit.kind.as_str()
                    } else {
                        REDUCED_KIND
                    };
                    assert_eq!(k, legal, "{l}: unexpected canonical sourceKind");
                }
            }
        }
    }
}

/// Contract §7.1: the split is computed, and the two sets exhaustively
/// partition the normative required set.
#[test]
fn the_retained_blocked_split_is_computed_not_nominated() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let Some(reduced) = &unit.reduced else {
                continue;
            };
            let l = &unit.label;
            let canonical = reduced.get("canonical").expect("canonical");

            assert_eq!(
                str_at(canonical, "sourceKind"),
                REDUCED_KIND,
                "{l}: the reduced record must not wear a complete projection's kind"
            );
            assert!(
                PROVENANCE_SOURCE_KINDS.contains(&str_at(canonical, "locatorKind")),
                "{l}: locatorKind must name a provenance kind"
            );
            assert_eq!(
                str_at(canonical, "locatorKind"),
                unit.kind,
                "{l}: locatorKind must be the source kind that was actually gated — \
                 shape alone is not identity (§7.3)"
            );
            assert_eq!(
                canonical.get("locator"),
                doc.get("locator"),
                "{l}: the record's locator is not the acquisition locator of this \
                 source — a well-formed pointer at the wrong object resolves, which \
                 is worse than a missing one (§7.3)"
            );
            assert_eq!(
                str_at(canonical, "outcome"),
                unit.outcome,
                "{l}: reduced record outcome disagrees with the gate"
            );
            assert_eq!(
                canonical.get("coverageComplete"),
                unit.assessment.get("coverageComplete"),
                "{l}: reduced record coverage disagrees with the assessment"
            );

            let (want_retained, want_blocked) = unit.expected_partition();
            let got_blocked = strings(canonical.get("blockedFields"));
            let got_retained: BTreeSet<String> = canonical
                .pointer("/retainedFields")
                .and_then(Value::as_object)
                .expect("retainedFields")
                .keys()
                .cloned()
                .collect();

            assert_eq!(
                got_blocked, want_blocked,
                "{l}: blockedFields is not (findings ∪ unassessed)"
            );
            assert_eq!(
                got_retained, want_retained,
                "{l}: retainedFields is not (required \\ blocked)"
            );
            assert!(
                !got_blocked.is_empty(),
                "{l}: a record that blocked nothing is not a reduced record"
            );

            let required = unit.required();
            let union: BTreeSet<String> = got_retained.union(&got_blocked).cloned().collect();
            assert_eq!(
                union, required,
                "{l}: the split does not cover the required set"
            );
            assert!(
                got_retained.is_disjoint(&got_blocked),
                "{l}: a field is retained and blocked at once"
            );
            for p in &got_retained {
                assert!(
                    p.starts_with('/'),
                    "{l}: retained key {p:?} is not a JSON pointer"
                );
            }
            assert!(
                well_formed_digest(str_at(reduced, "canonicalDigest")),
                "{l}: the reduced record is retained evidence and needs its digest"
            );
            assert_canonical_pointer_array(l, "blockedFields", canonical.get("blockedFields"));

            // §7.2: the right pointer is not enough — the value under it is frozen too.
            let retained_map = canonical
                .pointer("/retainedFields")
                .and_then(Value::as_object)
                .expect("retainedFields");
            for (pointer, held) in retained_map {
                let raw = unit.src.get(pointer).unwrap_or_else(|| {
                    panic!("{l}: {pointer} retained but absent from the source")
                });
                assert_eq!(
                    held,
                    &projected_value(pointer, raw),
                    "{l}: {pointer} holds a value the complete projection would not carry"
                );
            }

            // §7.3: identity lives in the locator, and the locator is not evidence.
            let loc = canonical
                .get("locator")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{l}: reduced record without a locator"));
            let shape: BTreeSet<&str> = table(LOCATOR_SHAPE, &unit.kind).iter().copied().collect();
            let got: BTreeSet<&str> = loc.keys().map(String::as_str).collect();
            assert_eq!(got, shape, "{l}: locator shape disagrees with §7.3");
            assert!(
                canonical.get("stableId").is_none(),
                "{l}: stableId must live in the locator, not beside the retained fields — \
                 otherwise a blocked /id walks back in under an alias"
            );
        }
    }
}

/// Contract §9.2: the assessment is retained, and every retained record is
/// bound to the assessment that authorised it.
#[test]
fn every_retained_record_is_bound_to_its_assessment() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            assert!(
                well_formed_digest(&unit.assessment_digest),
                "{l}: assessmentDigest is not well formed"
            );
            assert_eq!(
                str_at(&unit.binding, "assessmentDigest"),
                unit.assessment_digest,
                "{l}: the binding names a different assessment"
            );
            assert_eq!(
                str_at(&unit.binding, "recordDigest"),
                unit.retained_digest(),
                "{l}: the binding names a record this unit did not retain"
            );
        }
    }
}

#[test]
fn blocked_bytes_never_reach_a_snapshot() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            let body = unit.body();
            let body_survives = unit.resolvable().contains("/body");

            if body_survives {
                // §4 and §9: what was assessed is what was kept, byte for byte.
                let kept = match &unit.reduced {
                    None => str_at(unit.retention.get("canonical").expect("canonical"), "body")
                        .to_owned(),
                    Some(r) => r
                        .pointer("/canonical/retainedFields/~1body")
                        .and_then(Value::as_str)
                        .unwrap_or_else(|| panic!("{l}: /body retained but absent"))
                        .to_owned(),
                };
                assert_eq!(
                    kept, body,
                    "{l}: the retained body differs from the assessed body — something \
                     normalized it between the gate and retention"
                );
                continue;
            }

            let mut everywhere = Vec::new();
            canonical_objects(&doc, &mut everywhere);
            for canonical in &everywhere {
                assert!(
                    !body.is_empty() && !canonical.to_string().contains(&body),
                    "{l}: a blocked body appears inside a canonical object"
                );
            }
        }
    }
}

/// Contract §8: admissibility is resolved against the retained records, never
/// read from the fixture's own claim.
#[test]
fn a_derived_fact_needs_every_input_resolved() {
    let (mut admissible, mut inadmissible) = (0, 0);
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let Some(fact) = &unit.derived_fact else {
                continue;
            };
            let l = &unit.label;
            let inputs = strings(fact.get("wouldBeDerivedFrom"));
            assert!(!inputs.is_empty(), "{l}: a derived fact with no inputs");
            let resolvable = unit.resolvable();
            let computed = inputs.is_subset(&resolvable);

            assert_eq!(
                fact.get("admissible").and_then(Value::as_bool),
                Some(computed),
                "{l}: admissibility disagrees with what actually resolves"
            );
            if let Some(claim) = fact.get("everyInputFieldRetained").and_then(Value::as_bool) {
                assert_eq!(
                    claim, computed,
                    "{l}: the fixture's retention claim disagrees with the retained record"
                );
            }

            if let Some(reduced) = &unit.reduced {
                let loc = reduced
                    .pointer("/canonical/locator")
                    .and_then(Value::as_object)
                    .expect("locator");
                for input in &inputs {
                    if !resolvable.contains(input) {
                        let leaf = input.rsplit('/').next().unwrap_or(input);
                        assert!(
                            !loc.contains_key(leaf),
                            "{l}: {input} is blocked, and the locator must not satisfy it (§7.3)"
                        );
                    }
                }
            }

            if computed {
                admissible += 1;
                let bindings = strings(fact.get("derivedFrom"));
                assert!(
                    !bindings.is_empty(),
                    "{l}: an admissible fact must name its sources"
                );
                let mut retained = BTreeSet::new();
                retained.insert(unit.retained_digest());
                assert!(
                    bindings.is_subset(&retained),
                    "{l}: derivedFrom names a digest this unit did not retain"
                );
            } else {
                inadmissible += 1;
                assert!(
                    fact.get("derivedFrom").is_none(),
                    "{l}: an inadmissible fact must not claim a source binding"
                );
            }
        }
    }
    assert!(
        admissible > 0 && inadmissible > 0,
        "the derived-fact pair must witness both directions"
    );
}

#[test]
fn the_byte_discriminator_isolates_one_field() {
    let doc = load("whitespace-sensitive-secret-v1.json");
    let variants = doc
        .get("variants")
        .and_then(Value::as_array)
        .expect("variants");
    assert_eq!(
        variants.len(),
        2,
        "the discriminator needs exactly two variants"
    );

    let bodies: Vec<&str> = variants
        .iter()
        .map(|v| {
            v.pointer("/decodedSource/~1body")
                .and_then(Value::as_str)
                .expect("decoded body")
        })
        .collect();
    let (a, b) = (bodies.first().expect("a"), bodies.last().expect("b"));
    assert_ne!(a, b, "the two variants must differ");
    assert_eq!(
        a.len() + 1,
        b.len(),
        "the variants must differ by exactly one byte"
    );
    assert_eq!(
        b.replacen(' ', "", 1).len(),
        a.len(),
        "the differing byte must be the single inserted space"
    );

    let outcomes: BTreeSet<&str> = variants.iter().map(|v| str_at(v, "gateOutcome")).collect();
    assert_eq!(
        outcomes.len(),
        2,
        "one byte must flip the gate outcome — otherwise the pair proves nothing \
         about normalization before assessment"
    );

    for key in [
        "representation",
        "assessedFields",
        "detector",
        "redactionPolicyVersion",
        "coverageComplete",
    ] {
        let first = variants
            .first()
            .and_then(|v| v.pointer(&format!("/assessment/{key}")));
        let last = variants
            .last()
            .and_then(|v| v.pointer(&format!("/assessment/{key}")));
        assert_eq!(first, last, "the two variants disagree on assessment.{key}");
    }
}

#[test]
fn specimens_state_no_closure_verdict() {
    for (file, _) in SPECIMENS {
        let raw = fs::read_to_string(fixtures_dir().join(file)).expect("reading specimen");
        for token in CLASSIFIER_VOCABULARY {
            assert!(
                !contains_word(&raw, token),
                "{file}: carries closure-state vocabulary {token} — a redaction specimen \
                 records its gate outcome and nothing about closure"
            );
        }
    }
}

/// Hygiene about what was committed here, not a gate decision and not an input
/// to one.
#[test]
fn no_live_credential_shapes() {
    const REAL_SHAPES: &[&str] = &[
        "ghp_",
        "gho_",
        "github_pat_",
        "AKIA",
        "xoxb-",
        "xoxp-",
        "sk-",
        "AIza",
        "PRIVATE KEY-----",
    ];
    for (file, _) in SPECIMENS {
        let raw = fs::read_to_string(fixtures_dir().join(file)).expect("reading specimen");
        for shape in REAL_SHAPES {
            assert!(
                !raw.contains(shape),
                "{file}: contains {shape:?}, which looks like real credential material"
            );
        }
    }
    let readme = fs::read_to_string(fixtures_dir().join("README.md")).expect("reading README");
    assert!(
        readme.contains("SYNTHETIC_FAKE_TOKEN_"),
        "the README must name the synthetic constant it uses"
    );
}

#[test]
fn readme_documents_every_specimen() {
    let readme = fs::read_to_string(fixtures_dir().join("README.md")).expect("reading README");

    // The stale-description defect, mechanically. Round 4 added a second rule
    // and this section kept describing one, with the old digest — a stale
    // account of the very thing configDigest exists to pin down.
    assert!(
        readme.contains(SYNTHETIC_CONFIG_DIGEST),
        "README does not name the detector configuration digest the specimens carry"
    );
    for rule in rules_for(SYNTHETIC_CONFIG_DIGEST) {
        assert!(
            readme.contains(rule),
            "README does not describe rule {rule} of that configuration"
        );
    }
    for (file, id) in SPECIMENS {
        assert!(readme.contains(file), "README does not mention {file}");
        assert!(readme.contains(id), "README does not mention specimen {id}");
    }
    for outcome in OUTCOMES {
        assert!(
            readme.contains(outcome),
            "README omits the {outcome} outcome"
        );
    }
}

/// The mirror guard: this file must invent no required field the contract does
/// not state. It proves one direction only, exactly as its Step 0B predecessor
/// does, and does not claim the converse.
#[test]
fn contract_states_every_required_field_this_file_asserts() {
    let contract = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/architecture/closure-redaction-policy-v1.md"),
    )
    .expect("reading the contract");

    let mut per_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for (table_name, t) in [
        ("always", ALWAYS_FIELDS),
        ("present-only", PRESENT_ONLY_FIELDS),
    ] {
        for (kind, fields) in t {
            assert!(contract.contains(kind), "contract does not mention {kind}");
            for f in *fields {
                assert!(
                    contract.contains(f),
                    "contract §5.3 does not state {table_name} field {f} for {kind}"
                );
            }
            *per_kind.entry(kind).or_default() += 1;
        }
    }
    assert_eq!(
        per_kind.len(),
        5,
        "the mirror must cover every gated kind §5.3 freezes"
    );
    for code in COVERAGE_FAILURE_CODES {
        assert!(
            contract.contains(code),
            "contract does not state coverageFailureCode {code}"
        );
    }
    for key in ASSESSMENT_ALWAYS.iter().chain(ASSESSMENT_CONDITIONAL) {
        assert!(
            contract.contains(key),
            "contract §9 does not list assessment field {key}"
        );
    }
    assert!(
        contract.contains("no field of an assessment is free text"),
        "contract must state that V1 has no free text"
    );
    for (kind, fields) in LOCATOR_SHAPE {
        for f in *fields {
            assert!(
                contract.contains(f),
                "contract §7.3 does not state locator field {f} for {kind}"
            );
        }
    }
    assert!(
        contract.contains(REDUCED_KIND),
        "contract does not name the reduced record kind"
    );
}
