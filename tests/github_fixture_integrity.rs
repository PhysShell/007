//! Integrity of the frozen Step 0B GitHub fixtures (issue #147).
//!
//! The fixtures under `tests/fixtures/github/` were merged as evidence with no
//! consumer (PR #149, fixture commit `eb885924…`, merged into `main` as
//! `b0cc6447…`). This file is their FIRST consumer, introduced afterwards on
//! purpose: with the corpus already immutable on `main`, a failure here is a
//! DISCOVERY ABOUT THE FROZEN CORPUS, not licence to nudge fixture and checker
//! toward each other until they agree.
//!
//! SCOPE — INTEGRITY ONLY. This file checks that the fixtures and their README
//! are internally consistent: the expected files exist, parse, expose only the
//! documented API surfaces, resolve every pointer the README calls decisive,
//! honour the one array the README documents as empty, and satisfy the
//! equality/inequality relations the README states as observed conditions.
//!
//! HARD BOUNDARY — it maps NO fixture condition to a closure outcome. There is
//! deliberately no notion here of a verdict, of accepting or rejecting closure,
//! and no `PASS`/`FINDING`/`OWED`/`CANNOT_CHECK`/`STALE` vocabulary. A stale
//! review SHA is recorded here as "these two pointers differ" and nothing more;
//! deciding what that means about closure belongs to the classifier, which does
//! not exist yet and must not be anticipated from inside a hygiene check.
//!
//! The README stays authoritative for what the fixtures mean. The assertions
//! below MIRROR its decisive observations rather than parsing English prose —
//! a Markdown scraper would be brittle in exactly the way this project keeps
//! finding expensive. `readme_documents_every_asserted_pointer` guards the
//! mirror in one direction only: it proves this file invents no pointer the
//! README does not mention. It cannot prove the converse, and does not claim to.

// Justification for the restriction-lint allowance, per the precedent in
// `tests/a0_candidate_state_e2e.rs`: every panic path below is this test's own
// assertion or its own temporary-copy setup failing loudly. Nothing here runs
// against production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level keys the README's envelope convention documents. Each names the
/// API surface that produced the value beneath it.
const DOCUMENTED_SURFACES: &[&str] = &[
    "pull_request",
    "reviews",
    "review_comments",
    "issue_comments",
    "check_runs",
];

/// A relation the README states as an observed condition. Both sides are JSON
/// Pointers into the same fixture. Deliberately only equality and inequality of
/// observable values — no interpretation of what a difference implies.
#[derive(Clone, Copy)]
enum Relation {
    Equal(&'static str, &'static str),
    NotEqual(&'static str, &'static str),
}

struct FixtureSpec {
    file: &'static str,
    /// Pointers the README lists under "Decisive observations".
    required_pointers: &'static [&'static str],
    /// Pointers the README documents as empty arrays.
    empty_arrays: &'static [&'static str],
    relations: &'static [Relation],
}

const SPECS: &[FixtureSpec] = &[
    FixtureSpec {
        file: "stale-review-wrong-sha.json",
        required_pointers: &[
            "/pull_request/head/sha",
            "/reviews/0/commit_id",
            "/reviews/0/submitted_at",
            "/review_comments/0/commit_id",
            "/check_runs/check_runs/0/head_sha",
        ],
        empty_arrays: &[],
        relations: &[
            // "reviews[0].commit_id != pull_request.head.sha"
            Relation::NotEqual("/reviews/0/commit_id", "/pull_request/head/sha"),
            // "The check runs are bound to the head, the review is bound to a
            // superseded commit."
            Relation::Equal(
                "/check_runs/check_runs/0/head_sha",
                "/pull_request/head/sha",
            ),
            Relation::Equal("/review_comments/0/commit_id", "/reviews/0/commit_id"),
        ],
    },
    FixtureSpec {
        file: "falsification-in-comment.json",
        required_pointers: &[
            "/pull_request/head/sha",
            "/reviews/0/commit_id",
            "/reviews/0/body",
            "/issue_comments/0/body",
            "/issue_comments/0/user/login",
        ],
        empty_arrays: &["/review_comments"],
        relations: &[Relation::Equal(
            "/reviews/0/commit_id",
            "/pull_request/head/sha",
        )],
    },
    FixtureSpec {
        file: "conflicting-review-surfaces.json",
        required_pointers: &[
            "/pull_request/head/sha",
            "/reviews/0/commit_id",
            "/reviews/0/user/login",
            "/review_comments/0/commit_id",
            "/review_comments/0/pull_request_review_id",
            "/issue_comments/0/user/login",
            "/issue_comments/0/body",
            "/issue_comments/0/created_at",
        ],
        empty_arrays: &[],
        relations: &[
            Relation::Equal("/reviews/0/commit_id", "/pull_request/head/sha"),
            // "one vendor": the review and the issue comment share an author.
            Relation::Equal("/reviews/0/user/login", "/issue_comments/0/user/login"),
            // "records a defect at that same commit"
            Relation::Equal("/review_comments/0/commit_id", "/pull_request/head/sha"),
        ],
    },
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/github")
}

/// Integrity violations for one already-parsed fixture. Returns descriptions,
/// never a judgement about what any observed value implies.
fn check_document(spec: &FixtureSpec, doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let file = spec.file;

    let Some(obj) = doc.as_object() else {
        out.push(format!("{file}: top level is not a JSON object"));
        return out;
    };

    for key in obj.keys() {
        if !DOCUMENTED_SURFACES.contains(&key.as_str()) {
            out.push(format!("{file}: undocumented top-level surface `{key}`"));
        }
    }

    for ptr in spec.required_pointers {
        if doc.pointer(ptr).is_none() {
            out.push(format!("{file}: decisive pointer `{ptr}` does not resolve"));
        }
    }

    for ptr in spec.empty_arrays {
        match doc.pointer(ptr) {
            None => out.push(format!(
                "{file}: documented-empty pointer `{ptr}` does not resolve"
            )),
            Some(Value::Array(items)) if items.is_empty() => {}
            Some(Value::Array(items)) => out.push(format!(
                "{file}: `{ptr}` is documented as empty but holds {} element(s)",
                items.len()
            )),
            Some(_) => out.push(format!(
                "{file}: `{ptr}` is documented as an empty array but is not an array"
            )),
        }
    }

    for relation in spec.relations {
        let (left, right, want_equal) = match *relation {
            Relation::Equal(l, r) => (l, r, true),
            Relation::NotEqual(l, r) => (l, r, false),
        };
        match (doc.pointer(left), doc.pointer(right)) {
            (Some(lv), Some(rv)) => {
                let equal = lv == rv;
                if equal != want_equal {
                    let op = if want_equal { "==" } else { "!=" };
                    out.push(format!(
                        "{file}: stated relation `{left}` {op} `{right}` does not hold ({lv} vs {rv})"
                    ));
                }
            }
            (None, _) => out.push(format!(
                "{file}: relation operand `{left}` does not resolve"
            )),
            (_, None) => out.push(format!(
                "{file}: relation operand `{right}` does not resolve"
            )),
        }
    }

    out
}

/// Integrity violations for a directory holding the three fixtures. Fails
/// closed: a missing file or unparseable JSON is a violation, never a skip.
fn check_dir(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for spec in SPECS {
        let path = dir.join(spec.file);
        let Ok(text) = fs::read_to_string(&path) else {
            out.push(format!(
                "{}: expected fixture is missing or unreadable",
                spec.file
            ));
            continue;
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(doc) => out.extend(check_document(spec, &doc)),
            Err(err) => out.push(format!("{}: malformed JSON ({err})", spec.file)),
        }
    }
    out
}

/// Copy the frozen fixtures into a scratch directory so negative cases can
/// mutate a COPY. The committed corpus is never written to by this test.
fn copy_fixtures(dir: &Path) {
    for spec in SPECS {
        let from = fixtures_dir().join(spec.file);
        let to = dir.join(spec.file);
        fs::copy(&from, &to).expect("copying a frozen fixture into a scratch dir");
    }
}

fn load_copy(dir: &Path, file: &str) -> Value {
    let text = fs::read_to_string(dir.join(file)).expect("reading a scratch copy");
    serde_json::from_str(&text).expect("parsing a scratch copy")
}

fn store_copy(dir: &Path, file: &str, doc: &Value) {
    let text = serde_json::to_string_pretty(doc).expect("serializing a scratch copy");
    fs::write(dir.join(file), text).expect("writing a scratch copy");
}

// ---------------------------------------------------------------- positive --

#[test]
fn frozen_fixtures_pass_integrity_checks() {
    let violations = check_dir(&fixtures_dir());
    assert!(
        violations.is_empty(),
        "frozen Step 0B corpus violates its own documented integrity:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn readme_documents_every_asserted_pointer() {
    let readme =
        fs::read_to_string(fixtures_dir().join("README.md")).expect("reading the fixture README");
    let mut missing = Vec::new();
    for spec in SPECS {
        let mut pointers: Vec<&str> = spec.required_pointers.to_vec();
        pointers.extend(spec.empty_arrays.iter().copied());
        for relation in spec.relations {
            let (l, r) = match *relation {
                Relation::Equal(l, r) | Relation::NotEqual(l, r) => (l, r),
            };
            pointers.push(l);
            pointers.push(r);
        }
        for ptr in pointers {
            if !readme.contains(ptr) {
                missing.push(format!(
                    "{}: `{ptr}` is asserted here but absent from README.md",
                    spec.file
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "this test asserts pointers the frozen README does not document:\n  {}",
        missing.join("\n  ")
    );
}

// ---------------------------------------------------------------- negative --

#[test]
fn missing_fixture_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    fs::remove_file(scratch.path().join("stale-review-wrong-sha.json"))
        .expect("removing a scratch copy");

    let violations = check_dir(scratch.path());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing or unreadable")),
        "a missing fixture must fail closed, got: {violations:?}"
    );
}

#[test]
fn malformed_json_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    fs::write(
        scratch.path().join("falsification-in-comment.json"),
        "{ this is not json",
    )
    .expect("writing");

    let violations = check_dir(scratch.path());
    assert!(
        violations.iter().any(|v| v.contains("malformed JSON")),
        "malformed JSON must fail closed, got: {violations:?}"
    );
}

#[test]
fn unresolved_decisive_pointer_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    let mut doc = load_copy(scratch.path(), "stale-review-wrong-sha.json");
    let review = doc.pointer_mut("/reviews/0").expect("the review object");
    review
        .as_object_mut()
        .expect("review is an object")
        .remove("submitted_at");
    store_copy(scratch.path(), "stale-review-wrong-sha.json", &doc);

    let violations = check_dir(scratch.path());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("/reviews/0/submitted_at") && v.contains("does not resolve")),
        "a decisive pointer that stops resolving must be reported, got: {violations:?}"
    );
}

#[test]
fn undocumented_top_level_surface_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    let mut doc = load_copy(scratch.path(), "conflicting-review-surfaces.json");
    doc.as_object_mut()
        .expect("top level is an object")
        .insert("labels".to_owned(), json!([]));
    store_copy(scratch.path(), "conflicting-review-surfaces.json", &doc);

    let violations = check_dir(scratch.path());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("undocumented top-level surface `labels`")),
        "an undocumented API surface must be reported, got: {violations:?}"
    );
}

#[test]
fn violated_inequality_relation_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    let mut doc = load_copy(scratch.path(), "stale-review-wrong-sha.json");
    let head = doc
        .pointer("/pull_request/head/sha")
        .expect("head sha")
        .clone();
    *doc.pointer_mut("/reviews/0/commit_id")
        .expect("review commit id") = head;
    store_copy(scratch.path(), "stale-review-wrong-sha.json", &doc);

    let violations = check_dir(scratch.path());
    assert!(
        violations.iter().any(|v| v.contains("does not hold")),
        "a stated inequality that stops holding must be reported, got: {violations:?}"
    );
}

#[test]
fn violated_equality_relation_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    let mut doc = load_copy(scratch.path(), "conflicting-review-surfaces.json");
    *doc.pointer_mut("/issue_comments/0/user/login")
        .expect("issue comment author") = json!("someone-else[bot]");
    store_copy(scratch.path(), "conflicting-review-surfaces.json", &doc);

    let violations = check_dir(scratch.path());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("/reviews/0/user/login") && v.contains("does not hold")),
        "a stated equality that stops holding must be reported, got: {violations:?}"
    );
}

#[test]
fn documented_empty_array_becoming_non_empty_is_a_violation() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    copy_fixtures(scratch.path());
    let mut doc = load_copy(scratch.path(), "falsification-in-comment.json");
    let comments = doc
        .pointer_mut("/review_comments")
        .expect("review comments array");
    comments
        .as_array_mut()
        .expect("it is an array")
        .push(json!({"id": 1}));
    store_copy(scratch.path(), "falsification-in-comment.json", &doc);

    let violations = check_dir(scratch.path());
    assert!(
        violations.iter().any(|v| v.contains("documented as empty")),
        "a documented-empty array gaining an element must be reported, got: {violations:?}"
    );
}
