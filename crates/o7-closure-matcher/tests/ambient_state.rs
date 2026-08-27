//! Probes for the one property §13.1 asserts and no Rust type can enforce:
//! `f(candidate, parameters)` is *total, deterministic and pure*.
//!
//! WHAT IS ACTUALLY GUARANTEED, AND BY WHAT. A [`MatcherFn`] is a bare `fn`
//! pointer, so it has no captured environment — that much is structural, and it
//! is the whole of what the type system contributes. It says nothing about a
//! function body reading the clock, the filesystem, an environment variable or a
//! process-global counter.
//!
//! WHAT THIS FILE IS. Four probes, each of which a specific impurity would fail:
//!
//! | probe | catches |
//! |---|---|
//! | repeat / order / interleave | internal mutable state, call counters |
//! | neighbour independence | a matched set that depends on the other candidates |
//! | perturbed environment (subprocess) | env vars, locale-dependent comparison, TZ |
//! | source-text lint over `matchers.rs` | the direct ambient-access spellings |
//!
//! THAT THESE PROBES DISCRIMINATE WAS MEASURED, NOT ASSUMED. Three impurities
//! were injected, each run against the file, then reverted:
//!
//! | injected | caught by |
//! |---|---|
//! | a `thread_local` call counter returning `false` on the first call | order/count, neighbour, environment — and, after this file was tightened in response, the source lint |
//! | `recompute_matched` skipping a digest it had already emitted | neighbour independence **only** |
//! | `submitted_review_by_author` returning `false` when `GITHUB_ACTOR` is set | environment, source lint |
//!
//! The middle row is why the neighbour probe exists: a deduplicating recompute
//! is deterministic, order-stable and environment-independent, and it silently
//! breaks §13.2's "duplicates preserved". The first row is why the source lint's
//! denylist gained `Cell` and `thread_local` — it had passed the counter through.
//!
//! WHAT THIS FILE IS NOT. It is not a proof of purity, and no test in this
//! repository is. A predicate that branched on a date past 2030, or read a file
//! that happens to be absent here, would pass everything below. Purity is
//! enforced by review of `matchers.rs` — which is why the registry is a flat
//! const slice in one small file rather than anything dynamic. These probes make
//! the cheap and likely violations fail loudly; they do not make the expensive
//! ones impossible.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own construction
// of a candidate from a literal it wrote in the same function. Nothing here runs
// against production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{
    recompute_matched, verify_binding, Candidate, MatcherEntry, RecordedMatcher,
    RecordedQuerySnapshot, REGISTRY,
};
use serde_json::{json, Value};

/// A recorded matcher for an entry already in hand. Purity, not drift.
fn bound(entry: &MatcherEntry, parameters: &Value, candidates: &[Candidate]) -> RecordedMatcher {
    let declared: Vec<String> = candidates
        .iter()
        .map(|c| c.declared_digest.clone())
        .collect();
    snapshot(
        entry.id,
        entry.version,
        parameters,
        &declared,
        &[],
        Some(entry.implementation_digest),
    )
}

/// Build a `github-query-snapshot` and parse it, because that is the only way to
/// obtain a `RecordedMatcher`.
///
/// Deliberately not a shortcut past the parser: a test that could assemble one
/// field-by-field would be testing a value production code can never see.
fn snapshot(
    id: &str,
    version: &str,
    parameters: &Value,
    all_returned: &[String],
    claimed: &[String],
    implementation: Option<&str>,
) -> RecordedMatcher {
    let mut matcher = json!({
        "id": id,
        "version": version,
        "parameters": parameters.clone(),
    });
    let schema_version = match implementation {
        Some(digest) => {
            matcher
                .as_object_mut()
                .expect("the matcher block is an object")
                .insert("implementationDigest".to_owned(), json!(digest));
            2
        }
        None => 1,
    };
    // The digest is computed from the snapshot this helper just built, so it is
    // self-consistent by construction and proves nothing about authority. That is
    // deliberate: these tests are about selection semantics, and the binding's own
    // discriminating witnesses live in `recorded_implementation.rs`, where the
    // expected digest is read out of a frozen fixture computed outside this
    // workspace.
    let document = json!({
        "schemaVersion": schema_version,
        "sourceKind": "github-query-snapshot",
        "matcher": matcher,
        "allReturnedSnapshotDigests": all_returned,
        "matchedSnapshotDigests": claimed,
    });
    let bound = o7_closure_canonical::digest(&document).expect("digest");
    RecordedQuerySnapshot::from_canonical(&document, bound.as_str())
        .expect("a well-formed query snapshot parses")
        .recorded_matcher()
        .clone()
}

/// Set on the subprocess by `vectors_hold_under_a_perturbed_environment`.
const REEXEC_MARKER: &str = "O7_MATCHER_AMBIENT_PROBE_CHILD";

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("a vector is JSON")
}

/// One vector, evaluated on its own, with nothing before it in this process.
fn run(entry: &MatcherEntry, index: usize) -> bool {
    let vector = entry.vectors.get(index).expect("vector index");
    (entry.predicate)(&parse(vector.candidate), &parse(vector.parameters))
        .expect("a frozen vector evaluates")
}

/// Every (matcher, vector) pair, flattened, so probes can permute across
/// matchers as well as within one.
fn all_pairs() -> Vec<(&'static MatcherEntry, usize)> {
    REGISTRY
        .iter()
        .flat_map(|e| (0..e.vectors.len()).map(move |i| (e, i)))
        .collect()
}

/// A predicate with a call counter, a cached first answer, or any other internal
/// state would disagree with itself here. Forward, reverse, interleaved across
/// matchers, and repeated — every evaluation must return the vector's frozen
/// expectation, whatever ran before it.
#[test]
fn results_do_not_depend_on_call_order_or_call_count() {
    let pairs = all_pairs();
    assert!(pairs.len() >= 10, "too few vectors to permute meaningfully");

    let baseline: Vec<bool> = pairs.iter().map(|(e, i)| run(e, *i)).collect();
    for ((entry, index), got) in pairs.iter().zip(&baseline) {
        let expected = entry.vectors.get(*index).expect("vector").expected;
        assert_eq!(
            *got, expected,
            "{}/{} vector {index} does not hold standalone",
            entry.id, entry.version
        );
    }

    // Reverse: the last vector is now the first call this process makes into
    // that predicate.
    for (offset, (entry, index)) in pairs.iter().enumerate().rev() {
        assert_eq!(
            run(entry, *index),
            *baseline.get(offset).expect("baseline"),
            "{}/{} vector {index} moved when evaluated in reverse order",
            entry.id,
            entry.version
        );
    }

    // Interleaved across matchers, then the whole sequence again: a stride walk
    // visits the registry's entries in an order no single-matcher run produces.
    for stride in [3usize, 5, 7] {
        for start in 0..stride {
            let mut offset = start;
            while offset < pairs.len() {
                let (entry, index) = pairs.get(offset).expect("pair");
                assert_eq!(
                    run(entry, *index),
                    *baseline.get(offset).expect("baseline"),
                    "{}/{} vector {index} moved under stride {stride}",
                    entry.id,
                    entry.version
                );
                offset += stride;
            }
        }
    }

    // And the binding as a whole, twice, since `verify_binding` is the entry
    // point everything else calls.
    for entry in REGISTRY {
        let first = verify_binding(entry).expect("binding");
        let second = verify_binding(entry).expect("binding");
        assert_eq!(first.as_str(), second.as_str());
    }
}

/// §13.2 makes the matched list a subsequence of the candidates in observation
/// order. That is only reproducible if each candidate is judged on its own: a
/// predicate that consulted its neighbours — deduplicating, or taking the first
/// match only — would still emit a subsequence, and would emit a different one
/// for a differently-ordered acquisition of the same evidence.
///
/// So the strong statement, asserted here: the matched list is exactly the
/// candidates whose standalone evaluation is true, in position order.
#[test]
fn a_candidate_is_judged_without_reference_to_its_neighbours() {
    for entry in REGISTRY {
        let parameters = parse(entry.vectors.first().expect("a vector").parameters);
        // Only the vectors sharing the first vector's parameters can go in one
        // list; every matcher's vectors do, today, and this asserts it rather
        // than assuming it.
        let usable: Vec<usize> = (0..entry.vectors.len())
            .filter(|i| parse(entry.vectors.get(*i).expect("vector").parameters) == parameters)
            .collect();
        assert!(
            usable.len() >= 3,
            "{}/{}: fewer than three vectors share parameters; \
             neighbour independence is not being probed",
            entry.id,
            entry.version
        );

        let candidates: Vec<Candidate> = usable
            .iter()
            .map(|i| {
                let snapshot = parse(entry.vectors.get(*i).expect("vector").candidate);
                let declared_digest = o7_closure_canonical::digest(&snapshot)
                    .expect("canonical")
                    .as_str()
                    .to_owned();
                Candidate {
                    declared_digest,
                    snapshot,
                }
            })
            .collect();

        let expected: Vec<String> = usable
            .iter()
            .zip(&candidates)
            .filter(|(i, _)| run(entry, **i))
            .map(|(_, c)| c.declared_digest.clone())
            .collect();

        let matched = recompute_matched(&bound(entry, &parameters, &candidates), &candidates)
            .expect("recompute");
        assert_eq!(
            matched.matched, expected,
            "{}/{}: the matched list is not the standalone-true candidates in order",
            entry.id, entry.version
        );

        // Duplicating every candidate must duplicate every match and nothing
        // else — a deduplicating or first-match-wins predicate fails here.
        let doubled: Vec<Candidate> = candidates.iter().chain(&candidates).cloned().collect();
        let doubled_expected: Vec<String> = expected.iter().chain(&expected).cloned().collect();
        assert_eq!(
            recompute_matched(&bound(entry, &parameters, &doubled), &doubled)
                .expect("recompute")
                .matched,
            doubled_expected,
            "{}/{}: duplicated candidates do not duplicate the matches",
            entry.id,
            entry.version
        );

        // An empty prefix and a non-matching prefix must leave the tail's
        // verdicts untouched.
        let mut prefixed = vec![candidates.last().expect("a candidate").clone()];
        prefixed.extend(candidates.iter().cloned());
        let prefixed_expected: Vec<String> = usable
            .last()
            .filter(|i| run(entry, **i))
            .map(|_| {
                vec![candidates
                    .last()
                    .expect("a candidate")
                    .declared_digest
                    .clone()]
            })
            .unwrap_or_default()
            .into_iter()
            .chain(expected.iter().cloned())
            .collect();
        assert_eq!(
            recompute_matched(&bound(entry, &parameters, &prefixed), &prefixed)
                .expect("recompute")
                .matched,
            prefixed_expected,
            "{}/{}: a prefixed candidate changed the verdicts after it",
            entry.id,
            entry.version
        );
    }
}

/// The probe that needs a different process, because a matcher that read
/// `std::env` or compared strings through a locale would be perfectly
/// deterministic within one.
///
/// `LANG=tr_TR.UTF-8` is deliberate: Turkish case folding maps `I` to a dotless
/// `i`, so a predicate doing locale-aware case-insensitive comparison would
/// silently change its answer on the `Expected-Reviewer` vector. `TZ`,
/// `SOURCE_DATE_EPOCH` and the GitHub variables cover the other ambient reads an
/// acquisition-adjacent function is most likely to reach for.
#[test]
fn vectors_hold_under_a_perturbed_environment() {
    if std::env::var_os(REEXEC_MARKER).is_some() {
        // We are the child. Just run the bindings; the parent reads our status.
        for entry in REGISTRY {
            verify_binding(entry).expect("binding holds in the perturbed child");
        }
        return;
    }

    let exe = std::env::current_exe().expect("test binary path");
    let output = std::process::Command::new(exe)
        .args(["--exact", "vectors_hold_under_a_perturbed_environment"])
        .env(REEXEC_MARKER, "1")
        .env("TZ", "Pacific/Kiritimati")
        .env("LANG", "tr_TR.UTF-8")
        .env("LC_ALL", "tr_TR.UTF-8")
        .env("LC_COLLATE", "tr_TR.UTF-8")
        .env("SOURCE_DATE_EPOCH", "0")
        .env("CI", "true")
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_ACTOR", "expected-reviewer")
        .env("GITHUB_REPOSITORY", "PhysShell/007")
        .env("GITHUB_TOKEN", "not-a-token")
        .env("HOME", "/nonexistent")
        .output()
        .expect("re-executing the test binary");

    assert!(
        output.status.success(),
        "the conformance vectors do not hold under a perturbed environment.\n\
         stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Guard against the child silently filtering to zero tests and exiting 0.
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "the child did not actually run the probe:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// A source-text lint over each predicate's own file.
///
/// It reads `implementation_source` — the exact bytes `implementation_digest`
/// binds and `include_str!` compiles — so what is linted is what runs. Scoping
/// it to the predicate files is why the registry may use `include_str!` while a
/// predicate may not.
///
/// It is a denylist of exact spellings, so it is defeated by an alias or a macro.
/// It is a tripwire on the obvious route, not a sandbox.
#[test]
fn no_predicate_reaches_for_ambient_state_by_name() {
    for entry in REGISTRY {
        let source = entry.implementation_source;
        for forbidden in [
            "std::env",
            "env::var",
            "std::fs",
            "std::time",
            "SystemTime",
            "Instant",
            "static mut",
            "OnceLock",
            "lazy_static",
            "Cell",
            "thread_local",
            "Mutex",
            "RwLock",
            "Atomic",
            "thread_rng",
            "random",
            "std::process",
            "std::net",
            "include_str",
            "include_bytes",
        ] {
            assert!(
                !source.contains(forbidden),
                "{}/{}'s implementation mentions {forbidden:?}. A matcher reads its \
                 two arguments and nothing else (§13.1); if this is a false positive, \
                 the fix is a new matcher version, not a weaker list",
                entry.id,
                entry.version
            );
        }

        // The file's whole import surface. Two imports, fixed: anything else is
        // implementation this file's digest does not cover, which is the RED-2
        // hole reopened one level down.
        let imports: Vec<&str> = source
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("use "))
            .collect();
        assert_eq!(
            imports,
            vec!["use serde_json::Value;", "use crate::MatchError;"],
            "{}/{} imports something new; ambient state arrives through imports, and \
             a shared helper is implementation the digest does not bind",
            entry.id,
            entry.version
        );

        // Exactly one function, so the bound bytes define exactly one predicate.
        assert_eq!(
            source.matches("fn ").count(),
            1,
            "{}/{}'s file must define exactly one function",
            entry.id,
            entry.version
        );
    }
}
