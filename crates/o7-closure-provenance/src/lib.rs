//! Classifier provenance binding — Slice B.
//!
//! Slice A established that a set of bytes is the one a digest names, and said
//! plainly that this is content binding rather than authority: the expected
//! digest's *authority* is a provenance question. This crate is that question.
//!
//! THE DIRECTION IS THE WHOLE POINT. The tempting shape is a store that hands
//! back a snapshot and the digest to check it against, in one call:
//!
//! ```text
//! store.query_snapshot() -> (snapshot, digest)     // FORBIDDEN
//! ```
//!
//! That is the ninth defect of Slice A wearing a tenth hat — a thing and a
//! certificate written by the same act, which is self-consistency and never
//! authority. So the digest enters from the frozen decision basis, and the store
//! is only ever asked to resolve one:
//!
//! ```text
//! decision basis          names the expected digest D
//!         |
//! retained evidence       resolve(D) -> bytes, and digest(bytes) is CHECKED
//!         |
//! replay / classifier
//! ```
//!
//! A store cannot choose what it is checked against, because no method on
//! [`RetainedEvidence`] returns a digest.
//!
//! WHAT THIS CRATE DOES NOT DO. No acquisition, no network, no detector
//! implementation, no redaction semantics — `closure-redaction-policy-v1.md`
//! already froze the gate and this crate consumes its outcome rather than
//! re-deciding it. No attestation, no verification witness, no `Reproduced`.

#![forbid(unsafe_code)]

use serde_json::Value;

pub mod derivations;

/// One value a decision actually read: a retained source, and the JSON pointer
/// within it.
///
/// A pointer rather than a field name because `closure-redaction-policy-v1.md`
/// §7.1 keys retention by full JSON pointer, and comparing `/user/login` by its
/// trimmed leaf is a different comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    pub source_digest: String,
    pub pointer: String,
}

/// A derived acquisition fact together with the source digests it names.
///
/// Provenance V1 §18: `carries_finding` is not a GitHub field, and every derived
/// fact that influences the classifier must list the digests it was derived
/// from. §18 stops at *naming*; this crate additionally recomputes, because a
/// name that is never checked is a citation nobody followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedFact {
    /// A registered derivation id — see [`derivations`].
    pub derivation: String,
    pub version: String,
    pub value: Value,
    /// The source snapshot digests this fact claims to be derived from, in the
    /// order the derivation consumes them.
    pub derived_from: Vec<String>,
}

/// The binding a decision basis asserts between a retained record and the
/// assessment that authorised retaining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredBinding {
    pub record_digest: String,
    pub assessment_digest: String,
}

/// What the classifier consumed for one observation, frozen before evaluation.
///
/// Provenance V1 §17: the snapshot answers *what GitHub returned*, and this
/// answers *what the adapter handed the classifier*. Both are needed, and an
/// adapter bug is invisible without the second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionBasis {
    pub observation_id: String,
    pub inputs: Vec<DecisionInput>,
    pub derived: Vec<DerivedFact>,
    /// For an absence claim: the query snapshot digest replay must be checked
    /// against. It lives HERE and not in the store — that placement is the
    /// authority path this slice exists to establish.
    pub expected_query_digest: Option<String>,
    pub bindings: Vec<DeclaredBinding>,
}

/// A `closure-retention-binding` as the store holds it — the authority on which
/// assessment permitted a record to be kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionBinding {
    pub record_digest: String,
    pub assessment_digest: String,
}

/// Retained evidence, addressed by digest.
///
/// Deliberately minimal, and deliberately unable to volunteer a digest. Every
/// method takes one and returns bytes or nothing; none returns a digest for the
/// caller to then check against. That absence is load-bearing rather than
/// incidental — it is what stops the store from selecting its own expectation.
pub trait RetainedEvidence {
    /// The canonical object retained under `digest`, if any.
    ///
    /// The implementation is NOT trusted to return the right bytes: every caller
    /// in this crate recomputes the digest of what comes back and refuses a
    /// mismatch. A store that returns the wrong object under a correct key is
    /// the substitution this check exists for.
    fn resolve(&self, digest: &str) -> Option<Value>;

    /// The retention binding authorising `record_digest`.
    fn binding_for(&self, record_digest: &str) -> Option<RetentionBinding>;
}

/// Why a decision could not be evaluated from retained evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// The decision names a digest the store cannot resolve.
    NoSuchRecord { digest: String },
    /// The store returned bytes that are not the ones the digest names.
    RecordDigestMismatch { requested: String, computed: String },
    /// The record resolved, and the pointer this decision reads did not survive
    /// retention.
    PointerBlocked { digest: String, pointer: String },
    /// The record resolved and simply has no such pointer.
    PointerAbsent { digest: String, pointer: String },
    /// A retained record with no reachable authorising assessment. Redaction
    /// policy §9.2: bytes somebody kept, not evidence somebody was permitted to
    /// keep.
    NoRetentionBinding { record_digest: String },
    /// The basis asserts an assessment the store's binding does not.
    BindingMismatch {
        record_digest: String,
        declared: String,
        retained: String,
    },
    /// A derived fact does not follow from the sources it names.
    DerivationDisagrees {
        derivation: String,
        claimed: Value,
        recomputed: Value,
    },
    /// A derived fact names a derivation this crate cannot re-execute.
    UnknownDerivation { derivation: String, version: String },
}

/// The outcome of binding one decision basis to retained evidence.
///
/// Two variants and not three: there is no "partially admissible". A decision
/// whose inputs all resolve is evaluated by the frozen classifier semantics
/// unchanged; a decision missing any input is `CANNOT_CHECK`. Redaction policy
/// §10 is explicit that the gate does not determine the state — the state
/// follows per decision from which inputs survived.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an unread admissibility verdict is an unchecked axis reported as a passed one"]
pub enum Admissible {
    /// Every input resolved. The resolved values, in the order the basis listed
    /// them.
    Yes { values: Vec<Value> },
    /// At least one input did not resolve. Never `false`, never an empty set,
    /// never `OWED`.
    CannotCheck { why: Vec<Unresolved> },
}

/// Bind one decision basis to retained evidence.
///
/// FIRST CUT. This is the implementation one writes before thinking about who
/// authored which value, and the preregistered witnesses in
/// `tests/witnesses.rs` are the record of what it fails to catch. It is
/// committed in this state deliberately: the escape set is frozen before the
/// implementation that must survive it, so the implementation cannot be shaped
/// to a test set written after the fact.
pub fn admissibility<E: RetainedEvidence>(basis: &DecisionBasis, store: &E) -> Admissible {
    let mut values = Vec::new();
    for input in &basis.inputs {
        if let Some(record) = store.resolve(&input.source_digest) {
            if let Some(found) = record.pointer(&input.pointer) {
                values.push(found.clone());
            }
        }
    }
    Admissible::Yes { values }
}

// ---- Falsification surface scan — provenance V1 §16.

/// How far a scan of a falsification surface actually got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanCompleteness {
    Complete,
    Incomplete { reason: String },
    Failed { reason: String },
}

/// The record §16 requires to exist even when zero claims were found.
///
/// An empty `Vec<FalsificationFact>` does not say "the surface was fully
/// examined and there are no claims". It equally permits: never fetched, fetch
/// broke, only page 1 read, parser died, surface unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalsificationSurfaceScan {
    pub surface: String,
    pub query_binding: String,
    pub completeness: ScanCompleteness,
    /// The source or query snapshot digest this scan is evidenced by.
    pub snapshot_digest: String,
}

/// What a scan plus its claim count is permitted to mean.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an unread scan verdict is the empty-set escape reopening"]
pub enum ScanVerdict {
    /// A COMPLETE scan that found nothing. This is the only reading under which
    /// zero claims is a fact about the surface rather than about the adapter.
    ZeroClaimsMeaningful,
    /// A COMPLETE scan with claims.
    Claims(usize),
    /// Anything else. Never "zero falsifications".
    CannotCheck { why: String },
}

/// FIRST CUT — see [`admissibility`].
pub fn scan_verdict(scan: &FalsificationSurfaceScan, claims: usize) -> ScanVerdict {
    let _ = scan;
    if claims == 0 {
        ScanVerdict::ZeroClaimsMeaningful
    } else {
        ScanVerdict::Claims(claims)
    }
}

// ---- Subject read — provenance V1 §8.1.

/// One head read, which either happened or did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadRead {
    Observed(String),
    /// §8.1: a failed head read records a reason and carries no
    /// `snapshotDigest`.
    Failed {
        reason: String,
    },
}

/// The pair of head reads bracketing an evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRead {
    pub before: HeadRead,
    pub after: HeadRead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an unread staleness verdict is a missing head read reported as a fresh subject"]
pub enum Staleness {
    NotStale,
    Stale {
        observed: String,
    },
    /// A head read that did not happen. §8.1 is explicit that this is
    /// `CANNOT_CHECK` and never "not stale".
    CannotCheck {
        why: String,
    },
}

/// FIRST CUT — see [`admissibility`].
pub fn staleness(read: &SubjectRead, expected_sha: &str) -> Staleness {
    for observed in [&read.before, &read.after] {
        if let HeadRead::Observed(sha) = observed {
            if sha != expected_sha {
                return Staleness::Stale {
                    observed: sha.clone(),
                };
            }
        }
    }
    Staleness::NotStale
}
