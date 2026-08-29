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

use o7_closure_canonical::digest_of_canonical_bytes;
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
        "sha256:9990c9990875b746b0afcc3bda59538b34aeaaef87f9efeb98a03c5d9d7e902e",
    derive: carries_finding_v1::derive,
}];

/// Resolve an identity pair to exactly one derivation. Fails closed.
#[must_use]
pub fn resolve(id: &str, version: &str) -> Option<&'static DerivationEntry> {
    REGISTRY
        .iter()
        .find(|entry| entry.id == id && entry.version == version)
}

/// The bytes of the file defining a derivation are the ones bound to its
/// `(id, version)`.
///
/// The same mechanism as `o7-closure-matcher`'s implementation binding, and the
/// same reason: `matcher.version` changes whenever behaviour changes for ANY
/// input, no finite vector set discharges "any", and every edit moves the file's
/// bytes. `include_str!` and `mod` read the same path in the same build, so the
/// hashed bytes are the code that runs.
///
/// WHAT THIS DOES NOT ESTABLISH, and it is the same residual Slice A closed for
/// matchers and this slice leaves open for derivations: the expected value lives
/// in this tree, two lines from the `include_str!` that supplies the bytes it
/// judges, so one commit can edit both. Slice A's answer was to record the
/// digest in the durable artifact instead, and no artifact carries a derivation
/// digest yet. Until one does, this check catches drift and not intent.
pub fn verify_implementation(entry: &DerivationEntry) -> Result<(), DerivationDrift> {
    let computed = digest_of_canonical_bytes(entry.implementation_source.as_bytes());
    if computed.as_str() == entry.implementation_digest {
        return Ok(());
    }
    Err(DerivationDrift {
        id: entry.id,
        version: entry.version,
        expected: entry.implementation_digest,
        computed: computed.as_str().to_owned(),
    })
}

/// A registered derivation is not the code bound to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationDrift {
    pub id: &'static str,
    pub version: &'static str,
    pub expected: &'static str,
    pub computed: String,
}

impl std::fmt::Display for DerivationDrift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "derivation {:?} version {:?} is not the implementation bound to it: the source \
             file hashes to {}, the version is bound to {}. A behaviour change needs a new \
             version, never a new digest on this one",
            self.id, self.version, self.computed, self.expected
        )
    }
}
