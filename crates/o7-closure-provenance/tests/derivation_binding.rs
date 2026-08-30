//! The derivation registry binds an identity pair to exactly one implementation.
//!
//! Same mechanism as `o7-closure-matcher`, same reason, and — stated here rather
//! than discovered in review — the same unclosed half. The expected digest lives
//! in this tree beside the bytes it judges, so it catches drift and not intent.
//! Slice A's answer was to move the expectation into the durable artifact; no
//! artifact carries a derivation digest yet, and §11 of this slice's DOC pass
//! records that rather than letting the check read as stronger than it is.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own parse of a
// JSON literal written in this file. Nothing here runs against production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_provenance::derivations::{resolve, verify_implementation, REGISTRY};
use serde_json::json;

#[test]
fn every_registered_derivation_is_the_code_bound_to_it() {
    assert!(!REGISTRY.is_empty(), "an empty registry binds nothing");
    for entry in REGISTRY {
        verify_implementation(entry).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn identity_pairs_are_unique() {
    for (i, a) in REGISTRY.iter().enumerate() {
        for b in REGISTRY.iter().skip(i + 1) {
            assert!(
                !(a.id == b.id && a.version == b.version),
                "{}/{} is bound twice — an identity pair must resolve to one derivation",
                a.id,
                a.version
            );
        }
    }
}

#[test]
fn resolution_fails_closed() {
    assert!(resolve("review-carries-finding", "1").is_some());
    assert!(resolve("review-carries-finding", "2").is_none());
    assert!(resolve("no-such-derivation", "1").is_none());
}

/// The rule reads two fields and nothing else.
///
/// Asserted behaviourally, because the interesting failure is a derivation that
/// quietly starts consulting the review's `state` — which would make it decide
/// whether a verdict counts, and that is the classifier's job. A derivation that
/// leaked state would give different answers for these two, and it must not.
#[test]
fn the_rule_reads_the_owning_review_id_and_nothing_else() {
    let entry = resolve("review-carries-finding", "1").expect("registered");

    let owning = |state: &str| {
        vec![
            json!({"stableId": "R1", "state": state}),
            json!({"pullRequestReviewId": "R1"}),
        ]
    };
    assert_eq!(
        (entry.derive)(&owning("APPROVED")),
        (entry.derive)(&owning("CHANGES_REQUESTED")),
        "the review's state must not enter a selection rule"
    );
    assert_eq!((entry.derive)(&owning("APPROVED")), Some(json!(true)));
}

/// A source missing the field the rule reads yields `None`, never `false`.
///
/// This is the whole project's oldest rule arriving inside a two-line function:
/// a comment whose `pullRequestReviewId` did not survive projection has not
/// established that the review carries no finding. It has established nothing.
#[test]
fn an_unreadable_source_is_not_a_negative_result() {
    let entry = resolve("review-carries-finding", "1").expect("registered");

    assert_eq!(
        (entry.derive)(&[
            json!({"stableId": "R1"}),
            json!({"body": "no owning review id here"}),
        ]),
        None,
        "a missing input must not read as a false answer"
    );
    assert_eq!(
        (entry.derive)(&[json!({"stableId": "R1"})]),
        None,
        "too few sources is not an answer either"
    );
}

/// A derivation whose bytes are not the ones its version is bound to is refused.
///
/// WHAT THIS ESTABLISHES AND WHAT IT DOES NOT. This is the CHECKER: given an
/// entry whose declared digest does not describe its source, `verify_implementation`
/// reports drift rather than shrugging. It says nothing about whether the
/// decision path consults it — the registry is `const`, so no test can make the
/// real entry drift at runtime.
///
/// The WIRING is established by a source mutation instead, recorded in
/// GREEN-DERIV-BIND: setting the declared digest to a wrong value now turns
/// `witnesses::b6/b7/b7b`, `derivation_source_view` and `correction_b4d` red.
/// Before that commit only this file's `every_registered_derivation_is_the_code_bound_to_it`
/// failed, and `b7b_a_correctly_derived_fact_is_admitted` stayed green — the
/// decision path admitting a fact computed by code whose binding does not hold.
/// Both halves are needed and neither substitutes for the other.
#[test]
fn an_entry_whose_bytes_are_not_its_declared_digest_is_refused() {
    let bound = resolve("review-carries-finding", "1").expect("registered");
    let drifted = o7_closure_provenance::derivations::DerivationEntry {
        implementation_digest: "sha256:\
            0000000000000000000000000000000000000000000000000000000000000000",
        ..*bound
    };
    let drift = verify_implementation(&drifted).expect_err("a wrong digest must be refused");
    assert_eq!(drift.id, "review-carries-finding");
    assert_eq!(drift.version, "1");
    assert_ne!(
        drift.computed, drift.expected,
        "the refusal must report both the bytes' actual digest and the one the version is \
         bound to; a drift report naming one of them tells a reader nothing to act on"
    );
    verify_implementation(bound).expect("the real entry must still verify");
}
