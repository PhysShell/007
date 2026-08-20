//! Matcher implementation binding — `closure-source-provenance-v1.md` §13.1.
//!
//! The contract froze what a matcher is:
//!
//! ```text
//! matcher.id          names a deterministic, total, pure predicate
//!                     f(candidate canonical snapshot, parameters) -> bool
//! matcher.version     changes whenever f's behaviour changes for ANY input
//! matcher.parameters  every value f reads that is not the candidate snapshot
//! ```
//!
//! and recorded, as the residual blocking acquisition, that `id` + `version`
//! resolve to exactly one predicate by no stated mechanism. An adapter told to
//! compute `matchedSnapshotDigests` therefore had to pick an implementation
//! itself — §13.1's prohibition defeated by a missing prerequisite. This crate is
//! that prerequisite and nothing more.
//!
//! PURITY IS STRUCTURAL, NOT PROMISED. A matcher is a bare `fn` pointer, not a
//! closure and not a trait object, so it cannot capture state at all. What it
//! reads is what it is handed. That does not stop a function body from calling
//! the clock, and no Rust type can; `ambient_state.rs` probes for it behaviourally
//! and says plainly that a probe is not a proof.
//!
//! THE CANDIDATE BINDING IS DERIVED, NOT DECLARED. [`recompute_matched`] takes
//! candidate snapshots and recomputes each digest from the snapshot's own bytes,
//! rejecting any pair whose declared digest does not hold. A recomputation that
//! trusted a supplied digest-to-snapshot map would be re-running the selection
//! over whatever the producer said the candidates were.

use o7_closure_canonical::{digest, CanonicalError, Digest};
use serde_json::Value;

pub mod matchers;

/// The predicate §13.1 describes. A function pointer rather than a closure or a
/// trait object: it has no captured environment, so its inputs are exactly the
/// two the contract allows.
pub type MatcherFn = fn(candidate: &Value, parameters: &Value) -> Result<bool, MatchError>;

/// One frozen input/outcome pair. Together these are the finite witness set the
/// conformance digest covers.
#[derive(Debug, Clone, Copy)]
pub struct ConformanceVector {
    pub name: &'static str,
    /// JSON text so a vector is a literal in the source, reviewable as bytes.
    pub parameters: &'static str,
    pub candidate: &'static str,
    pub expected: bool,
}

/// An entry in the registry: an identity pair bound to one implementation, and a
/// digest over that implementation's behaviour on the frozen vectors.
#[derive(Debug, Clone, Copy)]
pub struct MatcherEntry {
    pub id: &'static str,
    pub version: &'static str,
    /// What the contract's `matcher.parameters` must contain for this matcher.
    pub parameter_keys: &'static [&'static str],
    pub vectors: &'static [ConformanceVector],
    /// Frozen digest over `(id, version, vectors, the results f produces now)`.
    ///
    /// This is change-detection, not correctness: correctness is asserted by each
    /// vector's `expected`, which is hand-authored. The digest additionally covers
    /// the vector set itself, so quietly weakening a vector trips it too.
    pub conformance_digest: &'static str,
    pub predicate: MatcherFn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchError {
    /// Fails closed: an unregistered id resolves to nothing, never to a default.
    UnknownMatcherId { id: String },
    /// Fails closed separately from an unknown id, because "this matcher exists
    /// but not at that version" and "no such matcher" are different facts.
    UnknownMatcherVersion {
        id: String,
        version: String,
        known: Vec<String>,
    },
    /// The implementation's behaviour on the frozen vectors no longer matches the
    /// digest bound to this version. §13.1 requires a new version for any
    /// behaviour change; this is the mechanism that notices one was not taken.
    ConformanceDigestMismatch {
        id: String,
        version: String,
        expected: String,
        computed: String,
    },
    /// A vector's hand-authored expectation is not what the predicate returns.
    ConformanceVectorFailed {
        id: String,
        version: String,
        vector: &'static str,
        expected: bool,
        got: bool,
    },
    /// `matcher.parameters` is missing a key this matcher reads, or carries one it
    /// does not. Silently defaulting a missing parameter is how a selection rule
    /// stops being reproducible from its record.
    ParameterMismatch {
        id: String,
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// A candidate's declared digest is not the digest of the snapshot beside it.
    CandidateDigestMismatch { declared: String, computed: String },
    /// A candidate snapshot cannot be canonicalized at all.
    Canonical { message: String },
    /// The candidate is not shaped like a canonical source snapshot.
    MalformedCandidate { why: &'static str },
}

impl std::fmt::Display for MatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMatcherId { id } => write!(f, "no matcher is bound to id {id:?}"),
            Self::UnknownMatcherVersion { id, version, known } => write!(
                f,
                "matcher {id:?} has no version {version:?}; bound versions: {known:?}"
            ),
            Self::ConformanceDigestMismatch {
                id,
                version,
                expected,
                computed,
            } => write!(
                f,
                "matcher {id:?} version {version:?} no longer behaves as the version \
                 bound to it: expected {expected}, computed {computed}. §13.1 requires a \
                 new version whenever behaviour changes"
            ),
            Self::ConformanceVectorFailed {
                id,
                version,
                vector,
                expected,
                got,
            } => write!(
                f,
                "matcher {id:?} version {version:?} failed conformance vector {vector:?}: \
                 expected {expected}, got {got}"
            ),
            Self::ParameterMismatch {
                id,
                expected,
                found,
            } => write!(
                f,
                "matcher {id:?} reads parameters {expected:?} but was given {found:?}"
            ),
            Self::CandidateDigestMismatch { declared, computed } => write!(
                f,
                "a candidate declares digest {declared} but its snapshot canonicalizes \
                 to {computed}"
            ),
            Self::Canonical { message } => write!(f, "canonicalizing a candidate: {message}"),
            Self::MalformedCandidate { why } => write!(f, "malformed candidate snapshot: {why}"),
        }
    }
}

impl std::error::Error for MatchError {}

impl From<CanonicalError> for MatchError {
    fn from(e: CanonicalError) -> Self {
        Self::Canonical {
            message: e.to_string(),
        }
    }
}

/// Every matcher this repository binds. A flat const slice on purpose: the
/// smallest mechanism that makes `id` + `version` resolve to one implementation.
/// Not a plugin system, not a registry file, nothing dynamic — a matcher that is
/// not in this array does not exist, and adding one is a reviewable diff.
pub const REGISTRY: &[MatcherEntry] = matchers::ALL;

/// Resolve an identity pair, failing closed on both halves.
pub fn resolve(id: &str, version: &str) -> Result<&'static MatcherEntry, MatchError> {
    let by_id: Vec<&MatcherEntry> = REGISTRY.iter().filter(|e| e.id == id).collect();
    if by_id.is_empty() {
        return Err(MatchError::UnknownMatcherId { id: id.to_owned() });
    }
    by_id
        .iter()
        .find(|e| e.version == version)
        .copied()
        .ok_or_else(|| MatchError::UnknownMatcherVersion {
            id: id.to_owned(),
            version: version.to_owned(),
            known: by_id.iter().map(|e| e.version.to_owned()).collect(),
        })
}

/// Run the frozen vectors and check both halves of the binding: each vector's
/// hand-authored expectation, and the digest over the whole behaviour.
pub fn verify_binding(entry: &MatcherEntry) -> Result<Digest, MatchError> {
    let mut results = Vec::with_capacity(entry.vectors.len());
    for vector in entry.vectors {
        let parameters: Value = serde_json::from_str(vector.parameters).map_err(|_| {
            MatchError::MalformedCandidate {
                why: "vector parameters are not JSON",
            }
        })?;
        let candidate: Value =
            serde_json::from_str(vector.candidate).map_err(|_| MatchError::MalformedCandidate {
                why: "vector candidate is not JSON",
            })?;
        let got = (entry.predicate)(&candidate, &parameters)?;
        if got != vector.expected {
            return Err(MatchError::ConformanceVectorFailed {
                id: entry.id.to_owned(),
                version: entry.version.to_owned(),
                vector: vector.name,
                expected: vector.expected,
                got,
            });
        }
        results.push(serde_json::json!({
            "name": vector.name,
            "parameters": parameters,
            "candidate": candidate,
            "result": got,
        }));
    }

    let statement = serde_json::json!({
        "schemaVersion": 1,
        "sourceKind": "closure-matcher-conformance",
        "matcher": { "id": entry.id, "version": entry.version },
        "parameterKeys": entry.parameter_keys,
        "vectors": results,
    });
    let computed = digest(&statement)?;
    if computed.as_str() != entry.conformance_digest {
        return Err(MatchError::ConformanceDigestMismatch {
            id: entry.id.to_owned(),
            version: entry.version.to_owned(),
            expected: entry.conformance_digest.to_owned(),
            computed: computed.as_str().to_owned(),
        });
    }
    Ok(computed)
}

/// One retained candidate: the digest a query snapshot listed, and the snapshot
/// the evidence bundle resolved it to.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub declared_digest: String,
    pub snapshot: Value,
}

/// §13.1's conformance obligation, executed.
///
/// Given the identity pair, the parameters and the candidates in
/// `allReturnedSnapshotDigests` order, produce the matched subsequence — same
/// members, same relative order, duplicates preserved, per §13.2.
///
/// Every candidate's digest is recomputed from its own snapshot first. A
/// selection re-run over a producer-declared digest-to-snapshot map would
/// reproduce the producer's opinion of what the candidates were.
pub fn recompute_matched(
    id: &str,
    version: &str,
    parameters: &Value,
    candidates: &[Candidate],
) -> Result<Vec<String>, MatchError> {
    let entry = resolve(id, version)?;
    verify_binding(entry)?;
    check_parameters(entry, parameters)?;

    let mut matched = Vec::new();
    for candidate in candidates {
        let computed = digest(&candidate.snapshot)?;
        if computed.as_str() != candidate.declared_digest {
            return Err(MatchError::CandidateDigestMismatch {
                declared: candidate.declared_digest.clone(),
                computed: computed.as_str().to_owned(),
            });
        }
        if (entry.predicate)(&candidate.snapshot, parameters)? {
            matched.push(candidate.declared_digest.clone());
        }
    }
    Ok(matched)
}

/// The obligation as a verdict: recompute, and compare against what a query
/// snapshot claimed.
pub fn verify_matched(
    id: &str,
    version: &str,
    parameters: &Value,
    candidates: &[Candidate],
    claimed: &[String],
) -> Result<bool, MatchError> {
    Ok(recompute_matched(id, version, parameters, candidates)? == claimed)
}

/// §13.1: parameters are exactly what the matcher reads. Not a superset, so a
/// stray key cannot ride along unnoticed; not a subset, so nothing is defaulted.
fn check_parameters(entry: &MatcherEntry, parameters: &Value) -> Result<(), MatchError> {
    let object = parameters
        .as_object()
        .ok_or(MatchError::MalformedCandidate {
            why: "matcher.parameters is not an object",
        })?;
    let mut found: Vec<String> = object.keys().cloned().collect();
    found.sort();
    let mut expected: Vec<String> = entry
        .parameter_keys
        .iter()
        .map(|k| (*k).to_owned())
        .collect();
    expected.sort();
    if found != expected {
        return Err(MatchError::ParameterMismatch {
            id: entry.id.to_owned(),
            expected,
            found,
        });
    }
    Ok(())
}
