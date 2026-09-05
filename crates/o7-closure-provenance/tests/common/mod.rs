//! The one fact this crate's witnesses need from outside their own fixtures.
//!
//! Twelve witness files used to carry their own copy of the lookup below, each
//! under an allowance justified on "JSON literals written in this file". A
//! registry lookup is not a literal written in a fixture, and no fixture file's
//! invariant was ever weighed against it. N1 records that; this module is where
//! the pair now lives so that each of those twelve justifications became true
//! by losing the sites it did not describe, rather than by being reworded to
//! cover them after the fact.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// three `expect` sites below are this workspace's OWN matcher registry failing to
// answer for an entry the provenance contract's §13.1 requires. The invariant
// that makes them sound is not the fixture invariant and must not borrow it: a
// witness naming a matcher the registry does not bind cannot be constructed at
// all, and the only alternative to failing loudly here is to substitute a
// digest the registry never produced — the vacuous green these files exist to
// prevent. Both sites are in the single function below and nothing else in this
// module may take the allowance without restating the invariant over it.
// Extent (checked by N1): 3 `expect` sites.
#![allow(clippy::expect_used)]
// Not a restriction lint and so outside rule 4, but stated for the same reason:
// each test binary compiles this whole module and uses the part it needs, so a
// helper unused by one binary is expected rather than dead.
#![allow(dead_code)]

/// The implementation digest §13.1 binds `id`/`version` to, read from the
/// registry that binds it rather than copied as a literal that can go stale.
pub(crate) fn bound_matcher_digest(id: &str, version: &str) -> String {
    let entry = o7_closure_matcher::resolve(id, version).expect("the matcher is registered");
    o7_closure_matcher::verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}

/// The canonical digest of a fixture literal, as the string every store keys on.
///
/// Every witness store used to carry this line itself, under that file's own
/// allowance. It is one invariant and it is stated here.
pub(crate) fn digest_of(value: &serde_json::Value) -> String {
    o7_closure_canonical::digest(value)
        .expect("a fixture must be able to canonicalize the literal it just wrote")
        .as_str()
        .to_owned()
}
