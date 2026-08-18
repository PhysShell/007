//! Preregistration integrity for the closure redaction specimens.
//!
//! The specimens under `tests/fixtures/closure-redaction/` were authored for
//! `docs/architecture/closure-redaction-policy-v1.md` and committed with no
//! consumer. This file is their first consumer, added afterwards on purpose —
//! the same ordering used for the Step 0B corpus and for the provenance
//! specimens, so that a failure here is a discovery about the committed
//! evidence rather than licence to nudge fixture and checker toward agreement.
//!
//! SCOPE — PREREGISTRATION INTEGRITY ONLY.
//!
//! HARD BOUNDARY 1 — this file is NOT a secret scanner. It never decides
//! whether a body contains a secret; it reads the outcome the specimen records
//! and checks the contract's structural consequences of that outcome. Specimen
//! R8 is credential-shaped to a human and carries no blocking finding, so a
//! checker that quietly grew its own detection heuristic fails there. The one
//! place this file does pattern-match (`no_live_credential_shapes`) is hygiene
//! about what was committed to this repository, and feeds into no gate
//! decision.
//!
//! HARD BOUNDARY 2 — it maps no gate outcome to a closure state. There is
//! deliberately no `PASS`/`FINDING`/`OWED`/`CANNOT_CHECK`/`STALE` vocabulary
//! here. Contract §10 derives the closure consequence; anticipating it from
//! inside a hygiene check would recreate the answer-key problem that provenance
//! V1 §12 forbids.

// Same justification as `tests/github_fixture_integrity.rs`: every panic path
// below is this test's own assertion failing loudly. Nothing runs against
// production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The preregistered specimen set. Adding or removing a file without updating
/// this list is itself a finding: the corpus is meant to be a fixed matrix.
const SPECIMENS: &[(&str, &str)] = &[
    ("safe-body-v1.json", "R1"),
    ("explicit-secret-v1.json", "R2"),
    ("whitespace-sensitive-secret-v1.json", "R3"),
    ("detector-failure-v1.json", "R4"),
    ("detector-inconclusive-v1.json", "R5"),
    ("derived-fact-blocked-v1.json", "R6"),
    ("safe-metadata-retained-v1.json", "R7"),
    ("token-shaped-safe-v1.json", "R8"),
];

/// Every gate outcome the contract defines (§5). All three must be exercised.
const OUTCOMES: &[&str] = &["RETAIN", "BLOCK_SECRET", "CANNOT_ASSESS"];

/// `sourceKind` values provenance V1 §8 defines for a complete projection. A
/// blocked source must never produce one, and the blocked-source metadata
/// record must never wear one (redaction contract §7).
const PROVENANCE_SOURCE_KINDS: &[&str] = &[
    "github-pull-request-head",
    "github-actions-check",
    "github-submitted-review",
    "github-review-comment",
    "github-issue-comment",
    "github-query-snapshot",
];

const BLOCKED_METADATA_KIND: &str = "github-blocked-source-metadata";

/// Closure-state vocabulary that must not appear in a redaction specimen.
///
/// Matched as whole words rather than as quoted JSON values. A vacuity probe
/// caught the narrower form: `"note": "this observation is \"PASS\""` escapes to
/// `\\"PASS\\"` in the file, so a verdict smuggled into prose walked straight
/// past a check that only looked for `"PASS"`.
const CLASSIFIER_VOCABULARY: &[&str] = &[
    "PASS",
    "FINDING",
    "OWED",
    "CANNOT_CHECK",
    "STALE",
    "expectedState",
    "headline",
];

/// True when `needle` occurs in `haystack` bounded by non-identifier characters
/// on both sides, so `STALE` does not fire on `INSTALLED`.
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

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure-redaction")
}

fn load(file: &str) -> Value {
    let path = fixtures_dir().join(file);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"))
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected string field {key:?} in {v}"))
}

/// One (outcome, assessment, retention) triple. Most specimens hold exactly
/// one; R3 holds two variants under a single locator.
struct Unit {
    label: String,
    /// The subtree this unit owns. For a single-outcome specimen that is the
    /// whole document; for the two-variant discriminator it is one variant, so
    /// that a blocked variant is not judged against its sibling's legitimate
    /// retained snapshot.
    scope: Value,
    outcome: String,
    assessment: Value,
    retention: Value,
    assessed_body: String,
    blocked_metadata: Option<Value>,
    derived_fact: Option<Value>,
}

fn units(file: &str, doc: &Value) -> Vec<Unit> {
    let build = |label: String, scope: &Value, fallback: &Value| Unit {
        label,
        scope: scope.clone(),
        outcome: str_at(scope, "gateOutcome").to_owned(),
        assessment: scope.get("assessment").expect("assessment").clone(),
        retention: scope.get("retention").expect("retention").clone(),
        assessed_body: str_at(scope, "assessedBody").to_owned(),
        blocked_metadata: scope.get("blockedSourceMetadata").cloned(),
        derived_fact: fallback.get("candidateDerivedFact").cloned(),
    };

    match doc.get("variants").and_then(Value::as_array) {
        Some(variants) => variants
            .iter()
            .map(|v| build(format!("{file}#{}", str_at(v, "variantId")), v, doc))
            .collect(),
        None => vec![build(file.to_owned(), doc, doc)],
    }
}

/// Every `canonical` object anywhere in the document, paired with the
/// `canonicalDigest` sitting beside it when there is one.
fn canonical_objects(v: &Value, out: &mut Vec<(Value, Option<String>)>) {
    match v {
        Value::Object(map) => {
            if let Some(canonical) = map.get("canonical") {
                out.push((
                    canonical.clone(),
                    map.get("canonicalDigest")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                ));
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

fn all_docs() -> Vec<(&'static str, Value)> {
    SPECIMENS.iter().map(|(f, _)| (*f, load(f))).collect()
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
            doc.get("specimens").and_then(Value::as_array).map(Vec::len),
            Some(1),
            "{file}: expected exactly one specimen id"
        );
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
    let mut seen: BTreeSet<String> = BTreeSet::new();
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
            let a = &unit.assessment;
            let l = &unit.label;
            for key in ["id", "version", "configDigest"] {
                let value = a
                    .pointer(&format!("/detector/{key}"))
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{l}: detector.{key} missing"));
                assert!(!value.is_empty(), "{l}: detector.{key} is empty");
            }
            assert!(
                a.get("schemaVersion").and_then(Value::as_u64).is_some(),
                "{l}: assessment without a schemaVersion"
            );
        }
    }
}

#[test]
fn assessment_records_what_was_assessed() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let a = &unit.assessment;
            let l = &unit.label;
            assert_eq!(
                str_at(a, "representation"),
                "decoded-source-field-values",
                "{l}: the contract defines exactly one legal representation"
            );
            let assessed = a
                .get("assessedFields")
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{l}: assessedFields missing"));
            assert!(!assessed.is_empty(), "{l}: assessedFields is empty");
            assert!(
                a.get("observedAt")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
                "{l}: observedAt missing"
            );
            assert_eq!(
                a.get("redactionPolicyVersion").and_then(Value::as_u64),
                Some(1),
                "{l}: redactionPolicyVersion missing or wrong"
            );
        }
    }
}

#[test]
fn a_failed_assessment_cannot_look_safe() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let a = &unit.assessment;
            let l = &unit.label;
            match unit.outcome.as_str() {
                "CANNOT_ASSESS" => {
                    let reason = a
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or_else(|| panic!("{l}: CANNOT_ASSESS without a reason"));
                    assert!(!reason.trim().is_empty(), "{l}: empty reason");
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
                        "{l}: BLOCK_SECRET with an empty findings list would be \
                         indistinguishable from a clean assessment"
                    );
                    for f in findings {
                        assert!(
                            f.get("findingId")
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty()),
                            "{l}: finding without an identifier"
                        );
                        assert!(
                            f.get("field")
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty()),
                            "{l}: finding without a field pointer"
                        );
                        // Contract §9: a finding must not quote the secret.
                        for key in ["match", "matched", "excerpt", "sample", "value", "digest"] {
                            assert!(
                                f.get(key).is_none(),
                                "{l}: finding carries {key:?} — a findings list must not \
                                 reproduce the matched bytes in any form"
                            );
                        }
                    }
                }
                "RETAIN" => {
                    assert_eq!(
                        a.get("findings").and_then(Value::as_array).map(Vec::len),
                        Some(0),
                        "{l}: RETAIN must record a completed assessment with no findings"
                    );
                    assert!(
                        a.get("reason").is_none(),
                        "{l}: RETAIN must not carry a failure reason"
                    );
                }
                other => panic!("{l}: unknown outcome {other:?}"),
            }
        }
    }
}

#[test]
fn retain_requires_coverage_of_every_retained_field() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let assessed: BTreeSet<String> = unit
                .assessment
                .get("assessedFields")
                .and_then(Value::as_array)
                .expect("assessedFields")
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            if let Some(required) = doc
                .get("fieldsProjectionWouldRetain")
                .and_then(Value::as_array)
            {
                let required: BTreeSet<String> = required
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                let covered = required.is_subset(&assessed);
                assert_eq!(
                    covered,
                    unit.outcome == "RETAIN",
                    "{}: coverage and outcome disagree — partial assessment must not RETAIN",
                    unit.label
                );
            }
        }
    }
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
                assert!(
                    PROVENANCE_SOURCE_KINDS.contains(&str_at(canonical, "sourceKind")),
                    "{l}: a retained snapshot must use a provenance V1 sourceKind"
                );
                assert!(
                    unit.retention
                        .get("canonicalDigest")
                        .and_then(Value::as_str)
                        .is_some_and(|d| d.starts_with("sha256:") && d.len() == 71),
                    "{l}: retained snapshot without a well-formed digest"
                );
            } else {
                assert!(
                    unit.retention.get("canonical").is_none()
                        && unit.retention.get("canonicalDigest").is_none(),
                    "{l}: a blocked source must not carry a snapshot or its digest"
                );
            }
        }
    }
}

#[test]
fn blocked_bytes_never_reach_a_snapshot_or_a_digest() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let l = &unit.label;
            // Containment is checked across the WHOLE document: blocked bytes
            // must not surface anywhere in the file, including inside a
            // sibling variant. The sourceKind rule below is per-unit, because
            // a sibling variant may legitimately hold a retained snapshot.
            let mut everywhere = Vec::new();
            canonical_objects(&doc, &mut everywhere);
            let mut in_scope = Vec::new();
            canonical_objects(&unit.scope, &mut in_scope);

            if unit.outcome == "RETAIN" {
                // The no-normalization invariant: what was assessed is what was kept.
                let canonical = unit.retention.get("canonical").expect("canonical");
                assert_eq!(
                    str_at(canonical, "body"),
                    unit.assessed_body,
                    "{l}: the retained body differs from the assessed body — something \
                     normalized it between the gate and the projection"
                );
                continue;
            }

            for (canonical, _) in &everywhere {
                assert!(
                    !canonical.to_string().contains(&unit.assessed_body),
                    "{l}: the assessed body appears inside a canonical object"
                );
            }
            for (canonical, _) in &in_scope {
                if let Some(kind) = canonical.get("sourceKind").and_then(Value::as_str) {
                    assert_eq!(
                        kind, BLOCKED_METADATA_KIND,
                        "{l}: a blocked source may only canonicalize a blocked-source \
                         metadata record"
                    );
                }
            }
        }
    }
}

#[test]
fn blocked_source_metadata_is_a_separate_representation() {
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let Some(wrapper) = unit.blocked_metadata else {
                assert_eq!(
                    unit.outcome, "RETAIN",
                    "{}: a non-RETAIN specimen without a metadata record",
                    unit.label
                );
                continue;
            };
            let l = &unit.label;
            let m = wrapper.get("canonical").expect("canonical");

            assert_eq!(
                str_at(m, "sourceKind"),
                BLOCKED_METADATA_KIND,
                "{l}: the reduced record must not wear a complete projection's kind"
            );
            assert!(
                PROVENANCE_SOURCE_KINDS.contains(&str_at(m, "locatorKind")),
                "{l}: locatorKind must name the provenance kind that was refused"
            );
            assert_eq!(
                str_at(m, "outcome"),
                unit.outcome,
                "{l}: metadata outcome disagrees with the gate"
            );

            let blocked: Vec<&str> = m
                .get("blockedFields")
                .and_then(Value::as_array)
                .expect("blockedFields")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            assert!(
                !blocked.is_empty(),
                "{l}: a record that blocked nothing is not a blocked-source record"
            );

            let retained = m
                .get("retainedFields")
                .and_then(Value::as_object)
                .expect("retainedFields");
            for pointer in &blocked {
                let name = pointer.trim_start_matches('/');
                assert!(
                    !retained.contains_key(name),
                    "{l}: {pointer:?} is listed as blocked and retained at once"
                );
            }
            assert!(
                wrapper
                    .get("canonicalDigest")
                    .and_then(Value::as_str)
                    .is_some_and(|d| d.starts_with("sha256:")),
                "{l}: the reduced record is retained evidence and needs its digest"
            );
        }
    }
}

#[test]
fn a_derived_fact_needs_every_input_retained() {
    let mut admissible = 0;
    let mut inadmissible = 0;
    for (file, doc) in all_docs() {
        for unit in units(file, &doc) {
            let Some(fact) = unit.derived_fact else {
                continue;
            };
            let l = &unit.label;
            let every_input_retained = fact
                .get("everyInputFieldRetained")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| panic!("{l}: everyInputFieldRetained missing"));
            let claimed = fact
                .get("admissible")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| panic!("{l}: admissible missing"));
            assert_eq!(
                claimed, every_input_retained,
                "{l}: admissibility must follow from whether every input is retained"
            );

            if claimed {
                admissible += 1;
                let bindings = fact
                    .get("derivedFrom")
                    .and_then(Value::as_array)
                    .unwrap_or_else(|| panic!("{l}: an admissible fact must name its sources"));
                assert!(!bindings.is_empty(), "{l}: empty derivedFrom");
                let mut found = Vec::new();
                canonical_objects(&doc, &mut found);
                let retained: BTreeSet<String> =
                    found.iter().filter_map(|(_, d)| d.clone()).collect();
                for b in bindings {
                    let d = b.as_str().expect("digest string");
                    assert!(
                        retained.contains(d),
                        "{l}: derivedFrom names {d}, which is not a retained snapshot here"
                    );
                }
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

    let bodies: Vec<&str> = variants.iter().map(|v| str_at(v, "assessedBody")).collect();
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

    // Everything about the assessment except its result must match, or the pair
    // would differ for a second reason and discriminate nothing.
    for key in [
        "representation",
        "assessedFields",
        "detector",
        "redactionPolicyVersion",
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
/// to one: no specimen may carry a string shaped like a real provider's
/// credential, even a revoked one.
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
    for (file, id) in SPECIMENS {
        assert!(readme.contains(file), "README does not mention {file}");
        assert!(readme.contains(id), "README does not mention specimen {id}");
    }
    for outcome in OUTCOMES {
        assert!(
            readme.contains(outcome),
            "README does not mention the {outcome} outcome"
        );
    }
}
