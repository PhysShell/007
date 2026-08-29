//! Registered derivations — provenance V1 §18, made re-executable.
//!
//! §18 requires a derived fact to *name* the source digests it came from, and
//! stops there. Naming is necessary and not sufficient: a citation nobody
//! follows is indistinguishable from a citation nobody could follow. So a
//! derivation here is the same shape as a matcher in `o7-closure-matcher` — an
//! identity pair resolving to exactly one pure function, whose bytes are hashed
//! so the pair cannot quietly come to mean something else.
//!
//! DELIBERATELY ONE ENTRY. `carries_finding` is the fact §18 names and the only
//! one any consumer needs today. This is a registry because that is the shape
//! the problem has, not a framework for derivations in general; a second entry
//! arrives when a second consumer does.

use serde_json::Value;

/// A derivation: total, pure, and a function of the named sources only.
///
/// A bare `fn` pointer for the reason `MatcherFn` is one — it cannot capture an
/// environment, so its inputs are exactly what it is handed.
pub type DerivationFn = fn(sources: &[Value]) -> Option<Value>;

/// One registered derivation.
#[derive(Debug, Clone, Copy)]
pub struct DerivationEntry {
    pub id: &'static str,
    pub version: &'static str,
    /// How many source snapshots the derivation consumes, in order.
    pub arity: usize,
    /// The exact bytes of the file defining `derive`, embedded at compile time.
    pub implementation_source: &'static str,
    /// SHA-256 of [`Self::implementation_source`], the identity of
    /// `(id, version)`.
    pub implementation_digest: &'static str,
    pub derive: DerivationFn,
}

/// `carries_finding`: a submitted review carries a finding when a review comment
/// belongs to it.
///
/// Provenance V1 §18's own example, and the reason the rule exists: this value
/// is not a GitHub field, so an adapter that supplies it is asserting rather
/// than reporting.
mod carries_finding_v1;

pub const REGISTRY: &[DerivationEntry] = &[DerivationEntry {
    id: "review-carries-finding",
    version: "1",
    arity: 2,
    implementation_source: include_str!("derivations/carries_finding_v1.rs"),
    implementation_digest:
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    derive: carries_finding_v1::derive,
}];

/// Resolve an identity pair to exactly one derivation. Fails closed.
#[must_use]
pub fn resolve(id: &str, version: &str) -> Option<&'static DerivationEntry> {
    REGISTRY
        .iter()
        .find(|entry| entry.id == id && entry.version == version)
}
