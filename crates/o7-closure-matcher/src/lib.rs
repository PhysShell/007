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
    /// The closed §8 shape a candidate must have for this matcher to be allowed
    /// to score it.
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
    pub candidate_schema: CandidateSchema,
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
    /// Only candidates declaring this `sourceKind` are checked. A candidate of a
    /// different kind is a genuine non-match, not a malformed one — that is the
    /// delivery-surface law, and it must keep returning `false`.
    pub source_kind: &'static str,
    pub members: &'static [Member],
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
    /// A query snapshot's matcher block does not match its `schemaVersion`.
    /// Version 1 has no `implementationDigest`; version 2 requires one. The two
    /// shapes are closed, so neither may borrow a field from the other.
    MalformedRecordedMatcher { why: &'static str },
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
            Self::MalformedRecordedMatcher { why } => {
                write!(f, "the recorded matcher block is malformed: {why}")
            }
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

/// A matcher block as some durable artifact recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedMatcher {
    pub id: String,
    pub version: String,
    /// `allReturnedSnapshotDigests` exactly as the snapshot declared it, in §13.2
    /// observation order.
    ///
    /// Replay is bound to this list, not to whatever a caller happens to hold.
    /// §13 obliges `matchedSnapshotDigests` to be recomputable from *the complete
    /// candidate set*; a caller that resolved only some of it — a retained blob
    /// that would not load, a truncated bundle — must not be able to reproduce an
    /// absence claim over the part that did load. An empty slice reproducing an
    /// empty claim is precisely "partial success is not success" (AGENTS.md).
    pub all_returned_snapshot_digests: Vec<String>,
    pub parameters: Value,
    pub implementation: RecordedImplementation,
}

impl RecordedMatcher {
    /// Read the matcher block out of a canonical `github-query-snapshot`.
    ///
    /// `schemaVersion` 1 yields [`RecordedImplementation::Unrecorded`];
    /// `schemaVersion` 2 REQUIRES `implementationDigest`. A version-2 snapshot
    /// missing the field and a version-1 snapshot carrying it are both refused:
    /// §8 schemas are closed key sets, and a shape that can borrow a field from
    /// its neighbour is not closed.
    pub fn from_query_snapshot(snapshot: &Value) -> Result<Self, MatchError> {
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
            all_returned_snapshot_digests: snapshot
                .get("allReturnedSnapshotDigests")
                .and_then(Value::as_array)
                .ok_or(MatchError::MalformedRecordedMatcher {
                    why: "no allReturnedSnapshotDigests array",
                })?
                .iter()
                .map(|d| {
                    d.as_str()
                        .map(str::to_owned)
                        .ok_or(MatchError::MalformedRecordedMatcher {
                            why: "a candidate digest is not a string",
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
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
        check_candidate_shape(entry, candidate)?;
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

/// The obligation as a verdict: recompute, and compare against what a query
/// snapshot claimed.
pub fn verify_matched(
    recorded: &RecordedMatcher,
    candidates: &[Candidate],
    claimed: &[String],
) -> Result<MatchedVerdict, MatchError> {
    let replay = recompute_matched(recorded, candidates)?;
    let reproduced = replay.matched == claimed;
    Ok(MatchedVerdict { replay, reproduced })
}

/// A candidate of the matcher's target kind must conform to that kind's closed
/// §8 shape. A candidate of any other kind is left alone — deciding it is the
/// predicate's job, and the answer is a legitimate `false`.
fn check_candidate_shape(entry: &MatcherEntry, candidate: &Candidate) -> Result<(), MatchError> {
    let schema = &entry.candidate_schema;
    if candidate
        .snapshot
        .pointer("/sourceKind")
        .and_then(Value::as_str)
        != Some(schema.source_kind)
    {
        return Ok(());
    }
    check_shape(&candidate.snapshot, schema.members, "").map_err(|why| {
        MatchError::IncompleteCandidate {
            source_kind: schema.source_kind.to_owned(),
            why,
            declared_digest: candidate.declared_digest.clone(),
        }
    })
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
