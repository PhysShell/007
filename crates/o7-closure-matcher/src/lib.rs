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

use o7_closure_canonical::{digest, digest_of_canonical_bytes, CanonicalError, Digest};
use serde_json::Value;

pub mod matchers;
mod schemas;

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

/// An entry in the registry: an identity pair bound to one implementation.
///
/// Two digests, deliberately: [`Self::implementation_digest`] is the identity of
/// `(id, version)`, and [`Self::conformance_digest`] is a behavioural regression
/// witness over a finite vector set. Conflating them is the defect RED-2
/// (782cbaf) recorded.
#[derive(Debug, Clone, Copy)]
pub struct MatcherEntry {
    pub id: &'static str,
    pub version: &'static str,
    /// What the contract's `matcher.parameters` must contain for this matcher.
    pub parameter_keys: &'static [&'static str],
    pub vectors: &'static [ConformanceVector],
    /// The exact bytes of the file that defines `predicate`, embedded at compile
    /// time by `include_str!`.
    ///
    /// `include_str!` and `mod` read the same path in the same build, so this is
    /// the source of the code that actually runs — not a copy that can drift
    /// from it. That is what lets a digest over these bytes be an identity
    /// rather than an observation.
    pub implementation_source: &'static str,
    /// SHA-256 of [`Self::implementation_source`]. **This is the identity of
    /// `(id, version)`.**
    ///
    /// §13.1 requires a new version whenever behaviour changes for ANY input.
    /// No finite vector set discharges `ANY`; RED-2 (782cbaf) is the recorded
    /// proof, where a change gated on a `state` value no vector used passed a
    /// fully green suite. Hashing the implementation instead of a sample of its
    /// behaviour needs no enumeration: every edit moves these bytes.
    ///
    /// What it does NOT cover: a behaviour change that leaves the bytes alone —
    /// a dependency's semantics shifting under it, or a compiler change. The
    /// conformance vectors are the witness for that residual, which is the
    /// division of labour between the two digests.
    pub implementation_digest: &'static str,
    /// Frozen digest over `(id, version, parameterKeys, vectors, the results f
    /// produces now)`.
    ///
    /// A **behavioural regression witness**, not an identity — see
    /// [`Self::implementation_digest`] for why the distinction is load-bearing.
    /// It is kept because the bytes never state what the rule is *supposed* to
    /// do, and each vector's hand-authored `expected` does; and because it
    /// covers the vector set itself, so quietly weakening a vector trips it too.
    pub conformance_digest: &'static str,
    /// The `sourceKind` whose objects this matcher scores.
    ///
    /// Not a validation table: validation is against [`SOURCE_SCHEMAS`], keyed by
    /// what each candidate declares. This names only which surface the predicate
    /// is about.
    ///
    /// Checked by [`recompute_matched`] before the predicate runs, and
    /// deliberately NOT inside the predicate: the predicate's bytes are what
    /// [`MatcherEntry::implementation_digest`] freezes, and a schema precondition
    /// is not part of the selection rule. §13.1 defines `f` over *canonical
    /// source snapshots*; an object that is not one is outside its domain rather
    /// than an input it should answer `false` for.
    ///
    /// This is the whole shape, not merely the fields the predicate reads. A
    /// candidate missing `commitId` is not a canonical source snapshot even
    /// though no matcher reads `commitId`, and scoring it `false` would let an
    /// empty matched set be assembled out of objects that are not evidence.
    pub target_source_kind: &'static str,
    pub predicate: MatcherFn,
}

/// One member of a closed §8 shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub name: &'static str,
    /// REQUIRED members must be present. OPTIONAL-IF-PRESENT members may be
    /// absent — but never `null`: §8 says an absent field is absent, and `null`
    /// is a value that claims the field was observed as empty.
    pub required: bool,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Integer,
    Bool,
    /// An array whose every element is a string — the digest arrays and the
    /// pagination page lists.
    ///
    /// The element type is checked, not merely the container: a digest array
    /// holding a number is not a list of digests, and admitting it would let a
    /// candidate reference survive as something no lookup can resolve.
    TextArray,
    /// A string whose value must be one of a closed set.
    ///
    /// Distinct from [`Self::Text`] because that is the distinction the sixth
    /// P1 turned on: validating a field's type is not validating the value
    /// admissibility turns on. `sourceKind` and `enumeration` are both strings
    /// and neither is admissible at an arbitrary string.
    OneOf(&'static [&'static str]),
    /// An object whose members the contract deliberately does not fix.
    ///
    /// Exactly one member is this: `matcher.parameters`, whose keys are
    /// whichever values the named matcher reads. Its closure is checked against
    /// the resolved matcher's declared `parameter_keys` by [`check_parameters`]
    /// at replay, where the matcher is known — a shape check here would have to
    /// guess. This variant therefore asserts only "an object", and says where
    /// the rest of the obligation is discharged rather than dropping it.
    OpenObject,
    /// A nested object with its own closed shape, e.g. `user`.
    Object(&'static [Member]),
}

/// The §8 shape a candidate of one `sourceKind` must have to be admissible input
/// to a matcher.
///
/// §8 shapes are **closed** key sets, so this checks both directions: every
/// required member present with the right type, and no member outside the
/// declared set. A subset check would let a candidate carry an unknown field
/// past replay, and a required-only check would let a truncated projection be
/// scored as a candidate that did not qualify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateSchema {
    /// The `sourceKind` this shape describes.
    pub source_kind: &'static str,
    /// The exact `schemaVersion` this shape describes.
    ///
    /// §8 gives a changed projection a new version, so a candidate declaring a
    /// version this registry does not know is evidence whose shape is unknown —
    /// not evidence that failed to match. Checking that `schemaVersion` is an
    /// integer establishes only that the field is well-typed; admissibility
    /// turns on its value, because that value is what says which key set the
    /// object was built to.
    pub schema_version: i64,
    pub members: &'static [Member],
}

/// The closed §13 shape of a `github-query-snapshot` at one `schemaVersion`.
///
/// A separate type from [`CandidateSchema`] rather than another row in it: a
/// query snapshot is not a candidate. It is the object that *declares* the
/// candidates, and admitting one into the candidate registry would let a
/// snapshot be scored by a matcher.
///
/// §13 makes the two versions closed key sets that "may not borrow from the
/// other", so each version carries its own complete member table. The V2 table
/// is not V1 plus a field at the type level, even though that is what it is on
/// paper — deriving it would make "which members does version 2 have" a
/// computation rather than a transcription, and it is the transcription that
/// `tests/schema_parity.rs` checks against the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuerySnapshotSchema {
    pub schema_version: i64,
    pub members: &'static [Member],
}

/// The `sourceKind` of the object §13 defines.
pub const QUERY_SNAPSHOT_SOURCE_KIND: &str = "github-query-snapshot";

/// Every `schemaVersion` of `github-query-snapshot` this crate can read.
///
/// A snapshot declaring a version outside this list is refused. §13 gives a
/// changed shape a new version, so an unregistered one is an object whose shape
/// is unknown — the same law [`check_candidate_shape`] applies to candidates,
/// applied to the object that declares them.
pub const QUERY_SNAPSHOT_SCHEMAS: &[QuerySnapshotSchema] = schemas::QUERY_SNAPSHOTS;

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
    /// The bytes of the file defining this version's predicate are not the bytes
    /// bound to it. §13.1 requires a new version for any behaviour change, and
    /// this is the check that enforces it: an implementation edit under an
    /// unchanged `(id, version)` is refused whether or not any frozen vector
    /// notices. Fixing it means adding a version, never updating this digest.
    ImplementationDigestMismatch {
        id: String,
        version: String,
        expected: String,
        computed: String,
    },
    /// The implementation's behaviour on the frozen vectors no longer matches the
    /// digest bound to this version. A regression witness — weaker than
    /// [`Self::ImplementationDigestMismatch`] and not a substitute for it.
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
    /// The candidates supplied for replay are not the ones the snapshot declared,
    /// in the order it declared them. Refused rather than replayed over what is
    /// present, because a matcher run over a subset answers a different question
    /// than the one the snapshot recorded.
    CandidateSetMismatch {
        declared: Vec<String>,
        supplied: Vec<String>,
    },
    /// A candidate declares a `sourceKind` and does not conform to that kind's
    /// closed §8 shape.
    ///
    /// Refused rather than treated as a non-match. A review whose `user.login`
    /// did not survive projection is evidence that could not be read, and
    /// reporting it as "did not match" turns an unreadable snapshot into a
    /// negative result — the absence claim §13 exists to prevent.
    IncompleteCandidate {
        source_kind: String,
        why: String,
        declared_digest: String,
    },
    /// A durable artifact recorded one implementation digest for this
    /// `(id, version)` and the registry now resolves it to another.
    ///
    /// This is the half of the binding that this source tree cannot rewrite.
    /// [`Self::ImplementationDigestMismatch`] compares two fields that a single
    /// commit can edit together; this compares the running code against a record
    /// written by a different act, at a different time, whose own digest is
    /// covered by whatever retained it. An attestation already emitted is beyond
    /// reach entirely.
    RecordedImplementationDrift {
        id: String,
        version: String,
        recorded: String,
        resolved: String,
    },
    /// The canonical query-snapshot bytes do not hash to the digest supplied for
    /// them.
    ///
    /// This binds the artifact's CONTENT to an expectation held outside it. It
    /// does NOT establish that the expectation is authoritative, that the bytes
    /// are genuine, or that whoever produced them was honest — a forged snapshot
    /// presented with the digest of that same forgery is internally consistent
    /// and passes. The authority of the expected digest comes from the layer
    /// that retained it (§11), which is Slice B's subject, and its authenticity
    /// from attestation, which is Slice D's.
    QuerySnapshotDigestMismatch { expected: String, computed: String },
    /// A query snapshot's matcher block does not match its `schemaVersion`.
    /// Version 1 has no `implementationDigest`; version 2 requires one. The two
    /// shapes are closed, so neither may borrow a field from the other.
    MalformedRecordedMatcher { why: &'static str },
    /// The bytes are digest-bound, and they are not a conforming
    /// `github-query-snapshot`.
    ///
    /// These are different facts and neither implies the other. A digest binds
    /// bytes to an expectation; it says nothing about their shape, because a
    /// malformed snapshot hashes to its own digest exactly as a well-formed one
    /// does. §13 defines the query snapshot as a closed key set at each
    /// `schemaVersion`, so conformance is a property of the whole object — not
    /// of the members some particular reader happens to consult.
    MalformedQuerySnapshot { why: String },
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
            Self::ImplementationDigestMismatch {
                id,
                version,
                expected,
                computed,
            } => write!(
                f,
                "matcher {id:?} version {version:?} is not the implementation bound to \
                 it: the source file hashes to {computed}, the version is bound to \
                 {expected}. §13.1 requires a new version for any behaviour change, so \
                 the fix is a new version entry, never a new digest on this one"
            ),
            Self::RecordedImplementationDrift {
                id,
                version,
                recorded,
                resolved,
            } => write!(
                f,
                "a durable artifact recorded matcher {id:?} version {version:?} as \
                 {recorded}, and this tree resolves that pair to {resolved}. The record \
                 is not editable from here, so the disagreement is real: either the \
                 artifact was produced by different code under the same version, or \
                 this version's implementation changed without taking a new one"
            ),
            Self::CandidateSetMismatch { declared, supplied } => write!(
                f,
                "replay was given a candidate sequence that is not the one the \
                 snapshot declared. Declared {} in observation order, supplied {}. \
                 §13 obliges the matched subsequence to be recomputable from the \
                 complete candidate set, so a partially resolved bundle is refused \
                 rather than replayed over the part that loaded",
                declared.len(),
                supplied.len()
            ),
            Self::IncompleteCandidate {
                source_kind,
                why,
                declared_digest,
            } => write!(
                f,
                "candidate {declared_digest} declares sourceKind {source_kind:?} and does \
                 not conform to that kind's closed §8 shape: {why}. This is refused \
                 rather than reported as a non-match: evidence that could not be read is \
                 not evidence of absence"
            ),
            Self::QuerySnapshotDigestMismatch { expected, computed } => write!(
                f,
                "the canonical query snapshot hashes to {computed}, and the digest \
                 supplied for it is {expected}. The bytes are not the ones that \
                 digest names, so nothing is extracted from them"
            ),
            Self::MalformedRecordedMatcher { why } => {
                write!(f, "the recorded matcher block is malformed: {why}")
            }
            Self::MalformedQuerySnapshot { why } => write!(
                f,
                "the bytes hash to the digest supplied for them and are not a conforming \
                 §13 github-query-snapshot: {why}. A matching digest establishes only that \
                 these are the bytes that digest names; a malformed snapshot hashes to its \
                 own digest, so nothing downstream will catch this"
            ),
            Self::ConformanceDigestMismatch {
                id,
                version,
                expected,
                computed,
            } => write!(
                f,
                "matcher {id:?} version {version:?} no longer produces the results \
                 bound to it: expected {expected}, computed {computed}. This is the \
                 behavioural witness; the implementation digest is the identity"
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
/// Every §8 source-snapshot shape, keyed by the `sourceKind` it describes.
pub const SOURCE_SCHEMAS: &[CandidateSchema] = schemas::ALL;

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

/// The whole obligation: implementation identity first, then behaviour.
///
/// Returns the **conformance** digest, so existing callers are unchanged. The
/// order matters — a matcher whose implementation is not the bound one must be
/// refused before any of its answers are consulted, because those answers are
/// then answers from something else.
pub fn verify_binding(entry: &MatcherEntry) -> Result<Digest, MatchError> {
    verify_implementation(entry)?;
    verify_conformance(entry)
}

/// Check the implementation binding: the bytes of the file defining this
/// version's predicate against the digest the version is bound to.
///
/// This is the check §13.1 actually needs, and the one RED-2 (782cbaf) showed
/// was missing. It is separate from [`verify_binding`] so the two failures never
/// masquerade as each other: this one says *the implementation moved*, the other
/// says *a result moved*.
pub fn verify_implementation(entry: &MatcherEntry) -> Result<Digest, MatchError> {
    let computed = digest_of_canonical_bytes(entry.implementation_source.as_bytes());
    if computed.as_str() != entry.implementation_digest {
        return Err(MatchError::ImplementationDigestMismatch {
            id: entry.id.to_owned(),
            version: entry.version.to_owned(),
            expected: entry.implementation_digest.to_owned(),
            computed: computed.as_str().to_owned(),
        });
    }
    Ok(computed)
}

/// Run the frozen vectors and check the behavioural half of the binding.
///
/// Callers wanting the whole obligation should use [`verify_binding`], which
/// checks the implementation identity first.
pub fn verify_conformance(entry: &MatcherEntry) -> Result<Digest, MatchError> {
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

/// What a durable artifact recorded about the implementation that produced it.
///
/// The distinction this type exists to keep is between *no drift* and *no
/// check*. A snapshot written before `implementationDigest` existed cannot
/// witness anything about the code that ran; collapsing that into "fine" is how
/// an unchecked axis starts reading as a passed one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedImplementation {
    /// Query-snapshot `schemaVersion` 2: the artifact names the implementation.
    Bound(String),
    /// Query-snapshot `schemaVersion` 1: the field did not exist when this was
    /// written. Replay CANNOT check implementation identity, and says so.
    Unrecorded,
}

/// The outcome of comparing the running implementation against the recorded one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an unread implementation check is an unchecked axis reported as a passed one"]
pub enum ImplementationCheck {
    /// The artifact's digest is the resolved implementation's.
    Bound { digest: String },
    /// The artifact predates the field. Nothing about the implementation was
    /// established — not that it drifted, and not that it did not.
    CannotCheck,
}

/// A canonical `github-query-snapshot` whose bytes hash to a digest supplied
/// from outside it.
///
/// This is the only route to a [`RecordedMatcher`]. §11 retains snapshots BY
/// digest, so the mapping `digest -> retained bytes` is where the authority
/// lives; the bytes alone are just JSON a caller happens to hold. Candidates
/// were already bound this way — each candidate's digest is recomputed and
/// checked against what the query snapshot declared — and the query snapshot
/// itself was bound to nothing, so the chain of custody terminated on an
/// unbound object one step above the part that was careful.
///
/// WHAT THIS ESTABLISHES, EXACTLY:
///
/// ```text
/// bytes + expected digest, mismatched   ->  REFUSE
/// bytes + expected digest, matching     ->  these are the bytes that digest names
/// ```
///
/// WHAT IT DOES NOT ESTABLISH. A forged snapshot presented together with the
/// digest of that same forgery is internally consistent and passes. This is
/// content binding relative to an expectation, not authentication: the
/// expectation's *authority* comes from the layer that retained it (Slice B
/// decides which digest is in the decision basis), its *production* from
/// acquisition (Slice C), and its *authenticity* from attestation (Slice D).
/// The name deliberately carries no adjective — nothing here is "trusted" or
/// "authenticated", and a later reader must not be able to infer that it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedQuerySnapshot {
    digest: String,
    matcher: RecordedMatcher,
}

impl RecordedQuerySnapshot {
    /// Bind canonical query-snapshot bytes to the digest supplied for them.
    ///
    /// The whole snapshot is canonicalized and hashed BEFORE any recorded value
    /// is read out of it. Checking a subset would leave every unchecked member
    /// free to differ from the artifact the digest names — including the members
    /// replay is checked against.
    ///
    /// Then the whole snapshot is checked against §13's closed shape, before any
    /// recorded value is read out of it. **The digest is not that check and
    /// cannot stand in for it**: it binds bytes to an expectation held outside
    /// them, and a snapshot malformed anywhere hashes to its own digest exactly
    /// as a well-formed one does. Reading seven members and constructing from
    /// them left the other ten free to be absent — including `enumeration`, whose
    /// presence §13 makes the precondition for a legal absence claim.
    pub fn from_canonical(
        snapshot: &Value,
        expected_query_digest: &str,
    ) -> Result<Self, MatchError> {
        let computed = digest(snapshot)?;
        if computed.as_str() != expected_query_digest {
            return Err(MatchError::QuerySnapshotDigestMismatch {
                expected: expected_query_digest.to_owned(),
                computed: computed.as_str().to_owned(),
            });
        }
        check_query_snapshot_shape(snapshot)?;
        Ok(Self {
            digest: computed.as_str().to_owned(),
            matcher: RecordedMatcher::from_query_snapshot(snapshot)?,
        })
    }

    /// The digest these bytes were bound to.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The matcher block, now known to be the one those bytes carry.
    pub fn recorded_matcher(&self) -> &RecordedMatcher {
        &self.matcher
    }
}

/// A matcher block as some durable artifact recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedMatcher {
    id: String,
    version: String,
    /// `matchedSnapshotDigests` exactly as the snapshot claimed it.
    ///
    /// Carried rather than passed in, for the same reason as the field below and
    /// as `implementation`: a verifier handed the claim as a loose argument is a
    /// verifier whose caller chooses what it is checking against. Passing the
    /// recomputed value in place of the artifact's own would report `reproduced`
    /// for a snapshot that contradicts itself, which is the one thing §13 asks
    /// replay to surface.
    matched_snapshot_digests: Vec<String>,
    /// `allReturnedSnapshotDigests` exactly as the snapshot declared it, in §13.2
    /// observation order.
    ///
    /// Replay is bound to this list, not to whatever a caller happens to hold.
    /// §13 obliges `matchedSnapshotDigests` to be recomputable from *the complete
    /// candidate set*; a caller that resolved only some of it — a retained blob
    /// that would not load, a truncated bundle — must not be able to reproduce an
    /// absence claim over the part that did load. An empty slice reproducing an
    /// empty claim is precisely "partial success is not success" (AGENTS.md).
    all_returned_snapshot_digests: Vec<String>,
    parameters: Value,
    implementation: RecordedImplementation,
}

impl RecordedMatcher {
    /// Every field is private and there is exactly one constructor, which reads
    /// an artifact.
    ///
    /// The fields carry the values replay is checked against, and §13.1 says
    /// nothing being checked may arrive from the party being checked. Public
    /// fields leave that as a convention: a caller could parse a snapshot whose
    /// claim is false, overwrite `matched_snapshot_digests` with the recomputed
    /// list, and get `reproduced: true` — reintroducing, through assignment, the
    /// exact bypass that removing the `claimed` parameter closed. Private fields
    /// make a `RecordedMatcher` unforgeable evidence of what an artifact said,
    /// so the rule is enforced by construction rather than by discipline.
    ///
    /// Tests build a snapshot and parse it like anything else. That costs a few
    /// lines per case and buys the guarantee that the only path into this type
    /// is the one production uses.
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn parameters(&self) -> &Value {
        &self.parameters
    }
    /// `matchedSnapshotDigests` as the snapshot claimed it.
    pub fn matched_snapshot_digests(&self) -> &[String] {
        &self.matched_snapshot_digests
    }
    /// `allReturnedSnapshotDigests` as the snapshot declared it, in §13.2 order.
    pub fn all_returned_snapshot_digests(&self) -> &[String] {
        &self.all_returned_snapshot_digests
    }
    pub fn implementation(&self) -> &RecordedImplementation {
        &self.implementation
    }

    /// Read the matcher block out of a canonical `github-query-snapshot`.
    ///
    /// `schemaVersion` 1 yields [`RecordedImplementation::Unrecorded`];
    /// `schemaVersion` 2 REQUIRES `implementationDigest`. A version-2 snapshot
    /// missing the field and a version-1 snapshot carrying it are both refused:
    /// §13's schemas are closed key sets, and a shape that can borrow a field
    /// from its neighbour is not closed.
    ///
    /// Those refusals are now unreachable in practice, because
    /// [`check_query_snapshot_shape`] has already checked the whole closed shape
    /// by the time this runs, and the version tables express the same rule. They
    /// are kept rather than deleted: this function must branch on the version
    /// anyway in order to *extract*, and an extraction that assumed its caller
    /// had validated would be one refactor away from being wrong silently. The
    /// duplication is a fail-closed default, not a second opinion — both arms
    /// refuse, so they cannot disagree about what to admit.
    /// NOT public. The only way to obtain a `RecordedMatcher` is through
    /// [`RecordedQuerySnapshot`], which binds the snapshot's bytes to a digest
    /// supplied from outside before anything is extracted from it. A public
    /// `&Value -> RecordedMatcher` constructor would let a caller mutate a
    /// retained snapshot and parse the result, which is the bypass one level
    /// earlier than the one private fields closed.
    fn from_query_snapshot(snapshot: &Value) -> Result<Self, MatchError> {
        let schema_version = snapshot
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .ok_or(MatchError::MalformedRecordedMatcher {
                why: "schemaVersion is not an integer",
            })?;
        let matcher = snapshot
            .get("matcher")
            .ok_or(MatchError::MalformedRecordedMatcher {
                why: "no matcher block",
            })?;
        let string = |key: &'static str| -> Result<String, MatchError> {
            matcher
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(MatchError::MalformedRecordedMatcher {
                    why: "matcher.id or matcher.version is not a string",
                })
        };
        let declared = matcher.get("implementationDigest");
        let implementation = match (schema_version, declared) {
            (1, None) => RecordedImplementation::Unrecorded,
            (2, Some(Value::String(digest))) => RecordedImplementation::Bound(digest.clone()),
            (1, Some(_)) => {
                return Err(MatchError::MalformedRecordedMatcher {
                    why: "a version-1 query snapshot carries implementationDigest, which \
                          version 1 does not define",
                })
            }
            (2, _) => {
                return Err(MatchError::MalformedRecordedMatcher {
                    why: "a version-2 query snapshot has no implementationDigest string, \
                          which version 2 requires",
                })
            }
            _ => {
                return Err(MatchError::MalformedRecordedMatcher {
                    why: "unknown query-snapshot schemaVersion",
                })
            }
        };
        Ok(Self {
            id: string("id")?,
            version: string("version")?,
            parameters: matcher.get("parameters").cloned().ok_or(
                MatchError::MalformedRecordedMatcher {
                    why: "no matcher.parameters",
                },
            )?,
            implementation,
            matched_snapshot_digests: digest_array(snapshot, "matchedSnapshotDigests")?,
            all_returned_snapshot_digests: digest_array(snapshot, "allReturnedSnapshotDigests")?,
        })
    }
}

/// One of the snapshot's ordered digest arrays. Both are REQUIRED by §13 even
/// when empty: an empty array is a fact about the enumeration, an absent one is
/// a fact about the adapter.
fn digest_array(snapshot: &Value, field: &'static str) -> Result<Vec<String>, MatchError> {
    snapshot
        .get(field)
        .and_then(Value::as_array)
        .ok_or(MatchError::MalformedRecordedMatcher {
            why: "a required digest array is absent or not an array",
        })?
        .iter()
        .map(|d| {
            d.as_str()
                .map(str::to_owned)
                .ok_or(MatchError::MalformedRecordedMatcher {
                    why: "a candidate digest is not a string",
                })
        })
        .collect()
}

/// Compare the implementation the registry resolves against the one an artifact
/// recorded.
pub fn check_recorded_implementation(
    entry: &MatcherEntry,
    recorded: &RecordedImplementation,
) -> Result<ImplementationCheck, MatchError> {
    let resolved = verify_implementation(entry)?;
    match recorded {
        RecordedImplementation::Unrecorded => Ok(ImplementationCheck::CannotCheck),
        RecordedImplementation::Bound(digest) => {
            if digest == resolved.as_str() {
                Ok(ImplementationCheck::Bound {
                    digest: digest.clone(),
                })
            } else {
                Err(MatchError::RecordedImplementationDrift {
                    id: entry.id.to_owned(),
                    version: entry.version.to_owned(),
                    recorded: digest.clone(),
                    resolved: resolved.as_str().to_owned(),
                })
            }
        }
    }
}

/// A replay of a recorded selection: what the matcher produces now, and whether
/// the code that produced it is the code the artifact named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay {
    pub matched: Vec<String>,
    pub implementation: ImplementationCheck,
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
/// Given a matcher block as some durable artifact recorded it, and the
/// candidates in `allReturnedSnapshotDigests` order, produce the matched
/// subsequence — same members, same relative order, duplicates preserved, per
/// §13.2 — together with the verdict on whether the implementation that just ran
/// is the one that artifact named.
///
/// The recorded matcher is taken whole rather than as loose id/version/parameter
/// arguments, because a caller who can supply the identity pair without the
/// recorded implementation is a caller who can skip the drift check. There is no
/// lower-level entry point that omits it.
///
/// Every candidate's digest is recomputed from its own snapshot first. A
/// selection re-run over a producer-declared digest-to-snapshot map would
/// reproduce the producer's opinion of what the candidates were.
pub fn recompute_matched(
    recorded: &RecordedMatcher,
    candidates: &[Candidate],
) -> Result<Replay, MatchError> {
    let entry = resolve(&recorded.id, &recorded.version)?;
    verify_binding(entry)?;
    let implementation = check_recorded_implementation(entry, &recorded.implementation)?;
    check_parameters(entry, &recorded.parameters)?;

    // The candidate sequence is the snapshot's, not the caller's. Length and
    // order both, since §13.2 puts observation order inside the query digest.
    let supplied: Vec<String> = candidates
        .iter()
        .map(|c| c.declared_digest.clone())
        .collect();
    if supplied != recorded.all_returned_snapshot_digests {
        return Err(MatchError::CandidateSetMismatch {
            declared: recorded.all_returned_snapshot_digests.clone(),
            supplied,
        });
    }

    let mut matched = Vec::new();
    for candidate in candidates {
        let computed = digest(&candidate.snapshot)?;
        if computed.as_str() != candidate.declared_digest {
            return Err(MatchError::CandidateDigestMismatch {
                declared: candidate.declared_digest.clone(),
                computed: computed.as_str().to_owned(),
            });
        }
        check_candidate_shape(candidate)?;
        if (entry.predicate)(&candidate.snapshot, &recorded.parameters)? {
            matched.push(candidate.declared_digest.clone());
        }
    }
    Ok(Replay {
        matched,
        implementation,
    })
}

/// A replay compared against what the artifact claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedVerdict {
    pub replay: Replay,
    /// Whether the recomputed subsequence equals the claimed one. False is a
    /// finding about the artifact, not an error — unlike drift, which is refused
    /// outright, because a claim that does not reproduce is exactly what §13
    /// exists to surface.
    pub reproduced: bool,
}

/// The obligation as a verdict: recompute, and compare against what the query
/// snapshot itself claimed.
///
/// There is no `claimed` parameter. The claim is a field of the recorded matcher
/// because a caller who supplies it chooses what the verifier is checking
/// against, and the failure that hides is exactly the one §13 exists to surface:
/// a snapshot whose `matchedSnapshotDigests` contradicts its own candidate set.
pub fn verify_matched(
    recorded: &RecordedMatcher,
    candidates: &[Candidate],
) -> Result<MatchedVerdict, MatchError> {
    let replay = recompute_matched(recorded, candidates)?;
    let reproduced = replay.matched == recorded.matched_snapshot_digests;
    Ok(MatchedVerdict { replay, reproduced })
}

/// Every candidate must conform to the closed §8 shape of the kind **it
/// declares** — not of the kind the running matcher scores.
///
/// A candidate of another surface is still an ordinary non-match, which is the
/// delivery-surface law. But it has to be a well-formed object of that surface
/// first: a malformed foreign object scored `false` joins an empty absence claim
/// exactly as a malformed same-kind one would, just wearing a different
/// `sourceKind`.
fn check_candidate_shape(candidate: &Candidate) -> Result<(), MatchError> {
    let refuse = |why: String| MatchError::IncompleteCandidate {
        source_kind: candidate
            .snapshot
            .pointer("/sourceKind")
            .and_then(Value::as_str)
            .unwrap_or("<absent>")
            .to_owned(),
        why,
        declared_digest: candidate.declared_digest.clone(),
    };

    // §7 first, and for every candidate regardless of kind: a canonical object
    // carries its own schemaVersion and sourceKind. An object without them is not
    // a canonical object at all, so it is not "a candidate of another kind" — the
    // delivery-surface path is for objects that legitimately declare a DIFFERENT
    // kind, not for objects that declare none. Skipping that distinction lets a
    // truncated snapshot that still holds the expected login be scored `false`.
    let Some(declared_kind) = candidate
        .snapshot
        .pointer("/sourceKind")
        .and_then(Value::as_str)
    else {
        return Err(refuse(
            "/sourceKind is absent or not a string; §7 requires every canonical \
             object to carry it"
                .to_owned(),
        ));
    };
    let Some(declared_version) = candidate.snapshot.pointer("/schemaVersion").and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
    }) else {
        return Err(refuse(
            "/schemaVersion is absent or not an integer; §7 requires every \
             canonical object to carry it"
                .to_owned(),
        ));
    };

    // The shape is chosen by what the candidate DECLARES. A kind §8 does not
    // define is unreadable evidence: a canonical source snapshot comes from an
    // enumerated surface, so an object claiming another one is not one.
    let Some(schema) = SOURCE_SCHEMAS
        .iter()
        .find(|s| s.source_kind == declared_kind)
    else {
        return Err(refuse(format!(
            "§8 defines no surface {declared_kind:?}; a canonical source snapshot \
             comes from an enumerated surface, so an object claiming another one is \
             evidence whose shape is unknown"
        )));
    };

    // §8 gives a changed projection a new version, so an unregistered version is
    // evidence whose shape is unknown rather than a candidate that failed to
    // qualify. Applying the V1 key set to a V2 projection scores an object this
    // registry has never been taught to read.
    if declared_version != schema.schema_version {
        return Err(refuse(format!(
            "/schemaVersion is {declared_version}, and this registry knows only \
             version {} of {}; §8 gives a changed projection a new version, so an \
             unregistered one is evidence whose shape is unknown",
            schema.schema_version, schema.source_kind
        )));
    }

    check_shape(&candidate.snapshot, schema.members, "").map_err(refuse)
}

/// The whole §13 query snapshot against the closed shape of the version it
/// declares.
///
/// Ordered exactly as [`check_candidate_shape`] is, and for the same reasons:
/// the universals first, because an object that does not declare what it is
/// cannot be looked up; then the version, because §13 gives a changed shape a
/// new version and applying the V1 table to a V2 object reads a projection this
/// crate was never taught; then the whole closed key set.
///
/// WHAT THIS DOES NOT DECIDE. Conformance is not admissibility. That
/// `enumeration` is present and carries a value §13 defines is a fact about the
/// object's shape. Whether a particular value is sufficient input for a
/// particular decision — §13's `NotProduced` is legal ONLY when
/// `enumeration = COMPLETE` — is a fact about that decision, and belongs to
/// whoever makes it. Refusing `INCOMPLETE` here would be this crate deciding a
/// classifier question, and would make specimen D, whose whole purpose is to
/// keep the non-authoritative empty result representable and distinguishable
/// from the authoritative one, unrepresentable.
fn check_query_snapshot_shape(snapshot: &Value) -> Result<(), MatchError> {
    let refuse = |why: String| MatchError::MalformedQuerySnapshot { why };

    let Some(declared_kind) = snapshot.pointer("/sourceKind").and_then(Value::as_str) else {
        return Err(refuse(
            "/sourceKind is absent or not a string; §7 requires every canonical object \
             to carry it, and without it any object holding a matcher block reads as a \
             query snapshot"
                .to_owned(),
        ));
    };
    if declared_kind != QUERY_SNAPSHOT_SOURCE_KIND {
        return Err(refuse(format!(
            "/sourceKind is {declared_kind:?}, not {QUERY_SNAPSHOT_SOURCE_KIND:?}; an \
             object of another kind that happens to carry a matcher block is not the \
             record §13 defines"
        )));
    }

    let Some(declared_version) = snapshot.pointer("/schemaVersion").and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
    }) else {
        return Err(refuse(
            "/schemaVersion is absent or not an integer; §7 requires every canonical \
             object to carry it"
                .to_owned(),
        ));
    };

    let Some(schema) = QUERY_SNAPSHOT_SCHEMAS
        .iter()
        .find(|s| s.schema_version == declared_version)
    else {
        let known: Vec<i64> = QUERY_SNAPSHOT_SCHEMAS
            .iter()
            .map(|s| s.schema_version)
            .collect();
        return Err(refuse(format!(
            "/schemaVersion is {declared_version}, and this registry knows only {known:?} \
             of {QUERY_SNAPSHOT_SOURCE_KIND}; §13 gives a changed shape a new version, so \
             an unregistered one is an object whose shape is unknown"
        )));
    };

    check_shape(snapshot, schema.members, "").map_err(refuse)
}

/// Both directions of a closed shape: nothing required missing, nothing outside
/// the declared set present.
fn check_shape(value: &Value, members: &[Member], path: &str) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}/ is not an object"))?;

    for member in members {
        let at = format!("{path}/{}", member.name);
        match object.get(member.name) {
            None => {
                if member.required {
                    return Err(format!("{at} is REQUIRED and absent"));
                }
            }
            Some(Value::Null) => {
                return Err(format!(
                    "{at} is null; §8 says an absent field is absent, not null"
                ))
            }
            Some(found) => match member.kind {
                ValueKind::Text if found.as_str().is_none() => {
                    return Err(format!("{at} is not a string"))
                }
                ValueKind::Integer if !found.is_i64() && !found.is_u64() => {
                    return Err(format!("{at} is not an integer"))
                }
                ValueKind::Bool if !found.is_boolean() => {
                    return Err(format!("{at} is not a boolean"))
                }
                ValueKind::TextArray => {
                    let items = found
                        .as_array()
                        .ok_or_else(|| format!("{at} is not an array"))?;
                    if let Some(position) = items.iter().position(|i| i.as_str().is_none()) {
                        return Err(format!("{at}[{position}] is not a string"));
                    }
                }
                ValueKind::OneOf(admissible) => {
                    let text = found
                        .as_str()
                        .ok_or_else(|| format!("{at} is not a string"))?;
                    if !admissible.contains(&text) {
                        return Err(format!(
                            "{at} is {text:?}, and the contract defines only {admissible:?}. \
                             A value outside the set is not an unrecognised state to be \
                             treated leniently — it is an object whose meaning this \
                             reader cannot establish"
                        ));
                    }
                }
                ValueKind::OpenObject if !found.is_object() => {
                    return Err(format!("{at} is not an object"))
                }
                ValueKind::Object(nested) => check_shape(found, nested, &at)?,
                _ => {}
            },
        }
    }

    for key in object.keys() {
        if !members.iter().any(|m| m.name == key.as_str()) {
            return Err(format!(
                "{path}/{key} is outside the closed §8 key set for this kind"
            ));
        }
    }
    Ok(())
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
