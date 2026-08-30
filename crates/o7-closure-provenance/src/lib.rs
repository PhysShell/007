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

// No `digest` import: this module no longer hashes anything. Every digest
// identity check in the crate now happens once, inside `artifact::validate`,
// which is the observable form of the choke point.
use crate::artifact::{ArtifactKind, Gate, ValidatedArtifact};
use crate::derivations::DerivationInput;
use o7_closure_canonical::digest;
use o7_closure_matcher::verify_matched;
use serde_json::{Map, Value};

pub mod artifact;
pub mod derivations;
pub mod redaction;

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

    /// The canonical bytes of the `closure-retention-binding` authorising
    /// `record_digest`.
    ///
    /// A `Value` and not a decoded struct, and not a bare digest either.
    ///
    /// Redaction §9.2 and §9.5 define the binding as a separately canonicalized,
    /// retained object with an exact four-member shape. Returning a pre-decoded
    /// `RetentionBinding { record_digest, assessment_digest }` let a store
    /// synthesize authority out of two strings with no retained bytes behind
    /// them at all — the escape CX-5/CR-1 recorded. Returning a bare digest
    /// would be worse in a subtler way: a store answering "here is the digest
    /// you should check this against" is the store choosing its own expectation,
    /// which is the very confusion §17's authority path exists to prevent.
    /// Hexadecimal does not make a claim independent.
    ///
    /// So the store hands over BYTES, and they go through the same artifact
    /// door as everything else.
    fn binding_for(&self, record_digest: &str) -> Option<Value>;
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
    /// The store produced binding bytes that are not retained under their own
    /// digest.
    ///
    /// §9.2 makes the binding a **separately retained object**. Bytes a store
    /// can manufacture at call time are a claim about a permission, not the
    /// permission — and named separately from `NoSuchRecord` for the reason
    /// `NoSuchAssessment` is: a bare digest in a refusal does not tell a reader
    /// which link of the chain went missing.
    NoSuchRetentionBinding {
        record_digest: String,
        binding_digest: String,
    },
    /// The basis asserts an assessment the store's binding does not.
    BindingMismatch {
        record_digest: String,
        declared: String,
        retained: String,
    },
    /// The store answered a binding request for one record with a binding that
    /// names another. A well-formed pointer at the wrong subject, which is worse
    /// than a missing one because it resolves.
    BindingSubjectMismatch {
        requested: String,
        binding_names: String,
    },
    /// The authorising assessment is not retained. Redaction policy §9.2 requires
    /// the assessment's canonical bytes RETAINED and reachable; a binding naming
    /// an assessment nobody kept authorises nothing.
    NoSuchAssessment {
        record_digest: String,
        assessment_digest: String,
    },
    // `AssessmentDigestMismatch` and `MalformedAssessment` were here, and both
    // are REMOVED as unreachable. Every artifact now enters through one door, so
    // an assessment whose bytes are not the ones its digest names is a
    // `RecordDigestMismatch` like any other, and one that is not a conforming §9
    // object is a `MalformedArtifact` like any other. Keeping assessment-specific
    // spellings of two universal facts would be the last trace of the design this
    // round removes: the assessment as the one artifact with a validation path of
    // its own.
    // `NoSuchHeadReadEvent`, `MalformedHeadRead` and `UnevidencedScan` were here,
    // and all three are REMOVED as unconstructible. They named head-read and
    // scan facts, and both of those paths report through
    // `Staleness::CannotCheck { why }` and `ScanVerdict::CannotCheck { why }` —
    // not through this enum at all. They have had no construction site since
    // GREEN-B4 moved those paths behind the door; this round noticed because it
    // was checking that each of its own new refusals has one.
    //
    // A refusal nobody can produce is not a refusal. It is a promise, in public
    // API, that this crate can tell a reader a fact it has no way to tell them —
    // the same reason GREEN-B4 removed `AssessmentDigestMismatch` and
    // `MalformedAssessment`. Pre-existing rather than introduced here, and
    // recorded as such.
    /// The artifact is not a conforming closed form of its declared kind.
    ///
    /// Clause 1 of the round's law with the emphasis three review rounds showed
    /// it needs: **closed schema**, not merely bytes, digest and type. It is
    /// deliberately a refusal about the ARTIFACT rather than about the question
    /// being asked of it, because the ordering is the whole point. An object the
    /// contract forbids to exist must be refused before anything reasons about
    /// its retention, its partition, its enumeration or its subject — answering
    /// `PointerBlocked` for a malformed reduced record is already a statement
    /// about the retention semantics of an artifact that has no right to have
    /// any.
    MalformedArtifact {
        digest: String,
        kind: String,
        why: String,
    },
    /// The assessment is a conforming §9 object and does not authorise THIS
    /// record: its outcome refuses the record's own kind, contradicts the
    /// record's own outcome, or its findings and coverage do not compute the
    /// partition the record carries.
    AssessmentDoesNotAuthorise {
        record_digest: String,
        assessment_digest: String,
        why: String,
    },
    /// A retained record consumed in a role its own `sourceKind` does not
    /// support. Real bytes, correct digest, wrong kind of thing for the job.
    WrongKindForRole {
        digest: String,
        role: &'static str,
        found: String,
    },
    /// A derived fact does not follow from the sources it names.
    ///
    /// `recomputed` is a value the rule actually produced. It is never `Null`
    /// standing in for "the rule did not run": that spelling made evidence loss
    /// and a contradicted claim report as the same fact, and the two are the
    /// ones a reader most needs told apart.
    DerivationDisagrees {
        derivation: String,
        claimed: Value,
        recomputed: Value,
    },
    /// A derived fact cites a number of sources the derivation does not take.
    ///
    /// A defect in the CLAIM, not in the evidence: nothing was recomputed and
    /// nothing was lost, so it is neither a disagreement nor a retention loss.
    DerivationArityMismatch {
        derivation: String,
        expected: usize,
        cited: usize,
    },
    /// A field the derivation reads did not resolve in the source that carries
    /// it — blocked by the gate, or absent from the projection.
    ///
    /// EVIDENCE LOSS, and it is reported as itself. The accompanying
    /// [`Unresolved::PointerBlocked`] or [`Unresolved::PointerAbsent`] says
    /// which field and which of the two happened; this says what it cost.
    DerivationInputUnavailable {
        derivation: String,
        version: String,
        source_digest: String,
    },
    /// Every field the derivation reads resolved, and the rule still produced no
    /// value.
    ///
    /// Distinct from [`Unresolved::DerivationInputUnavailable`] because the
    /// remedy is different: nothing was lost, so the values that survived are
    /// not the ones the rule can use — a retained field of the wrong type, for
    /// instance. Never reported as the negative answer.
    DerivationCannotRecompute { derivation: String, version: String },
    /// A derived fact names a derivation this crate cannot re-execute.
    UnknownDerivation { derivation: String, version: String },
    /// A conforming §13 query snapshot whose state or whose own recorded claim
    /// does not support the decision consuming it.
    ///
    /// Deliberately not a `MalformedArtifact`: §13 says an INCOMPLETE snapshot
    /// is well-formed, and reporting it as malformed would tell a reader to go
    /// looking for a producer bug that is not there. The artifact is fine; it is
    /// the wrong evidence for this question.
    QueryDoesNotSupportRole {
        digest: String,
        role: &'static str,
        why: String,
    },
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

/// The role a decision consumes a retained artifact IN.
///
/// Clause 2 of the round's law lists `role` alongside subject, state and
/// partition, and this is where it bites: `admissibility`, `scan_verdict` and
/// `staleness` resolve artifacts for entirely different jobs, and before B3 they
/// went down one identical path. A submitted review standing in for the query
/// snapshot an absence claim rests on resolves, re-digests and passes every
/// check — while answering a question it cannot answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsumedAs {
    /// A source record the redaction gate produced, read through a decision
    /// pointer.
    GatedSource,
    /// The query snapshot an absence claim is replayed against — §13.
    ExpectedQuerySnapshot,
    /// The query snapshot evidencing a falsification surface scan — §16.
    ScanEvidence,
    /// One of the two §8.1 acquisition events bracketing an evaluation.
    HeadReadEvent,
    /// The `github-pull-request-head` projection an event names.
    SubjectHead,
    /// The §9 assessment a gated record's binding names.
    RetentionAssessment,
    /// The §9.2 binding that names that assessment.
    RetentionBinding,
    /// One of the source snapshots a §13 query snapshot enumerates, consumed as
    /// input to matcher replay.
    QueryCandidate,
}

impl ConsumedAs {
    const fn name(self) -> &'static str {
        match self {
            Self::GatedSource => "a gated source record",
            Self::ExpectedQuerySnapshot => "the expected query snapshot",
            Self::ScanEvidence => "a falsification scan's evidence",
            Self::HeadReadEvent => "a head-read event",
            Self::SubjectHead => "a subject head projection",
            Self::RetentionAssessment => "an authorising retention assessment",
            Self::RetentionBinding => "a retention binding",
            Self::QueryCandidate => "a query snapshot's replay candidate",
        }
    }

    /// Whether an artifact of this kind can fill this role at all.
    fn accepts(self, kind: ArtifactKind) -> bool {
        match self {
            Self::GatedSource => matches!(
                kind,
                ArtifactKind::CompleteProjection(_) | ArtifactKind::ReducedSourceRecord
            ),
            Self::ExpectedQuerySnapshot | Self::ScanEvidence => {
                matches!(kind, ArtifactKind::QuerySnapshot)
            }
            Self::HeadReadEvent => matches!(kind, ArtifactKind::HeadReadEvent),
            Self::SubjectHead => {
                kind == ArtifactKind::CompleteProjection("github-pull-request-head")
            }
            Self::RetentionAssessment => matches!(kind, ArtifactKind::RetentionAssessment),
            Self::RetentionBinding => matches!(kind, ArtifactKind::RetentionBinding),
            // NOT `GatedSource`, though both are gated. §13's matcher is defined
            // over canonical §8 source snapshots, and `GatedSource` also admits
            // a reduced source record — which is a legitimate thing to read a
            // decision pointer out of and not a thing a matcher can score. The
            // role has to say which, because the gate classification alone
            // cannot: both kinds are gated, and being gated is what §9.2 turns
            // on, not what §13 does.
            Self::QueryCandidate => matches!(kind, ArtifactKind::CompleteProjection(_)),
        }
    }
}

/// THE DOOR. The only way an artifact enters decision semantics.
///
/// Order is the claim, not an implementation detail:
///
/// ```text
/// 1  the store has the bytes at all
/// 2  the bytes are the ones the BASIS named          (re-digested, not trusted)
/// 3  the object is a conforming closed form of the kind it declares
/// 4  that kind can fill the role this job needs
/// 5  if the kind is GATED, §9.2 authority resolves and authorises THIS record
/// ```
///
/// Step 3 before steps 4 and 5 is what the fourth round exists to establish.
/// Asking whether a malformed object is authorised, or answering
/// `PointerBlocked` for one, is already reasoning about the retention semantics
/// of something the contract forbids to exist.
///
/// Nothing downstream takes a `serde_json::Value`. [`ValidatedArtifact`] has a
/// private inner value and no public constructor, so a consumer written next
/// year cannot reach pointer, scan, head or relation semantics without coming
/// through here — which is the difference between a rule and a habit.
fn resolve_artifact<E: RetainedEvidence>(
    store: &E,
    requested: &str,
    role: ConsumedAs,
    declared: &[DeclaredBinding],
    into: &mut Vec<Unresolved>,
) -> Option<ValidatedArtifact> {
    let Some(raw) = store.resolve(requested) else {
        into.push(Unresolved::NoSuchRecord {
            digest: requested.to_owned(),
        });
        return None;
    };

    let artifact = match artifact::validate(requested, &raw) {
        Ok(artifact) => artifact,
        Err(artifact::ValidationError::DigestMismatch { computed }) => {
            into.push(Unresolved::RecordDigestMismatch {
                requested: requested.to_owned(),
                computed,
            });
            return None;
        }
        Err(artifact::ValidationError::Malformed(why)) => {
            into.push(Unresolved::MalformedArtifact {
                digest: requested.to_owned(),
                kind: raw
                    .pointer("/sourceKind")
                    .and_then(Value::as_str)
                    .unwrap_or("<absent>")
                    .to_owned(),
                why,
            });
            return None;
        }
    };

    if !role.accepts(artifact.kind()) {
        into.push(Unresolved::WrongKindForRole {
            digest: requested.to_owned(),
            role: role.name(),
            found: artifact.kind().name().to_owned(),
        });
        return None;
    }

    // THE GATE CLASSIFICATION, and it is the artifact's own kind that decides.
    //
    // §9.2 requires a RetentionBinding for "every retained record produced
    // through this gate — complete projection or reduced record", and §5.3's
    // gated set includes `github-pull-request-head`. So the subject head goes
    // through the same authority chain as any other gated source, by being a
    // gated kind, rather than by a binding lookup bolted into the head path.
    // That bolt is exactly the design this round removes.
    //
    // Ungated kinds invent no assessment. §5.3 places the query snapshot outside
    // the gate; the event and the assessment are control artifacts, and making a
    // permission depend on a permission is a recursion with no base case.
    if artifact.kind().gate() == Gate::Gated && !authorised(store, &artifact, declared, into) {
        return None;
    }

    Some(artifact)
}

/// §9.2's authority chain for a gated artifact: a binding about THIS record,
/// naming an assessment that is itself retained, conforming, and authorising.
fn authorised<E: RetainedEvidence>(
    store: &E,
    record: &ValidatedArtifact,
    declared: &[DeclaredBinding],
    into: &mut Vec<Unresolved>,
) -> bool {
    let requested = record.digest();

    let Some(binding_bytes) = store.binding_for(requested) else {
        into.push(Unresolved::NoRetentionBinding {
            record_digest: requested.to_owned(),
        });
        return false;
    };

    // §9.2'S CHAIN, AND THE ORDER IS THE CLAIM.
    //
    //     binding_for(record)  ->  bytes            a claim, and only a claim
    //     digest(bytes)        ->  D                what makes it addressable
    //     resolve(D)           ->  retained bytes   REQUIRED
    //     validate(D, retained)                     the door
    //     /recordDigest == record                   the subject relation
    //
    // Digesting the handed-over bytes and handing that digest back to the
    // validator would be a tautology: any bytes are the bytes of their own
    // digest. What makes this a check is `resolve(D)` — §9.2 says the binding is
    // a SEPARATELY RETAINED object, so a store that can produce the bytes but
    // cannot produce them under their own digest has produced a claim nobody
    // kept. Before this round these two members were read straight out of the
    // handed-over bytes: no digest, no closed §9.5 form, no registered version,
    // no retention.
    let Ok(binding_digest) = digest(&binding_bytes) else {
        into.push(Unresolved::NoRetentionBinding {
            record_digest: requested.to_owned(),
        });
        return false;
    };
    let binding_digest = binding_digest.as_str().to_owned();

    // Absence reported as absence, and named. `NoSuchRecord` would say the same
    // thing about the store and not say which artifact it was about — the
    // distinction `NoSuchAssessment` already draws one link further down the
    // chain.
    if store.resolve(&binding_digest).is_none() {
        into.push(Unresolved::NoSuchRetentionBinding {
            record_digest: requested.to_owned(),
            binding_digest,
        });
        return false;
    }

    // Then the SAME door every other artifact comes through. §9.5's exact key
    // set, its registered `schemaVersion` and `sourceKind`, and the role, are
    // all checked there rather than by a path of this function's own. The
    // binding is ungated, so this does not recurse into another binding lookup.
    let Some(binding) = resolve_artifact(
        store,
        &binding_digest,
        ConsumedAs::RetentionBinding,
        declared,
        into,
    ) else {
        return false;
    };

    // THE SUBJECT RELATION. A binding is a claim about a particular record, and
    // the store answering a request about A with a binding naming B is a
    // well-formed pointer at the wrong subject — worse than a missing one,
    // because it resolves.
    let names = binding.str_at("/recordDigest").unwrap_or_default();
    if names != requested {
        into.push(Unresolved::BindingSubjectMismatch {
            requested: requested.to_owned(),
            binding_names: names.to_owned(),
        });
        return false;
    }
    let assessment_digest = binding
        .str_at("/assessmentDigest")
        .unwrap_or_default()
        .to_owned();

    // §9.2 requires the assessment's canonical bytes RETAINED and reachable: a
    // binding naming an assessment nobody kept authorises nothing, and reading
    // only the digest string makes the permission a rumour about a document.
    // Absence is reported as absence and nothing else — an earlier revision of
    // this function reported NoSuchAssessment for a malformed one too, which
    // named the wrong fact about the store at the exact boundary where which
    // fact it is matters most.
    if store.resolve(&assessment_digest).is_none() {
        into.push(Unresolved::NoSuchAssessment {
            record_digest: requested.to_owned(),
            assessment_digest: assessment_digest.clone(),
        });
        return false;
    }

    // The assessment then comes through the SAME door. It is an ungated control
    // artifact, so this does not recurse into another binding lookup, but its
    // closed §9 form is checked by the one validator rather than by a private
    // path of its own.
    let Some(assessment) = resolve_artifact(
        store,
        &assessment_digest,
        ConsumedAs::RetentionAssessment,
        declared,
        into,
    ) else {
        return false;
    };

    if let Err(reason) = redaction::check_authorises(record, &assessment) {
        into.push(Unresolved::AssessmentDoesNotAuthorise {
            record_digest: requested.to_owned(),
            assessment_digest: assessment_digest.clone(),
            why: reason,
        });
        return false;
    }

    // The retained binding is the authority on its own outcome. A basis that
    // names another assessment is asserting a permission that was never granted,
    // and it resolves perfectly well if nobody compares the two.
    for asserted in declared.iter().filter(|b| b.record_digest == requested) {
        if asserted.assessment_digest != assessment_digest {
            into.push(Unresolved::BindingMismatch {
                record_digest: requested.to_owned(),
                declared: asserted.assessment_digest.clone(),
                retained: assessment_digest.clone(),
            });
            return false;
        }
    }

    true
}

/// A query snapshot that has been QUALIFIED: not merely a conforming §13
/// artifact, but one whose state and whose own recorded claim survive
/// examination.
///
/// ```text
/// ValidatedArtifact              the artifact is what it says it is
///        |
///        v  qualify_query
///        v    §13  enumeration is COMPLETE
///        v    §13  every declared candidate is retained and conforming
///        v    §13  the matched subsequence RECOMPUTES to the claim
///        |
///  QualifiedQuery                 evidence about a query
///        |
///        +-- supports_absence        §13  the absence role
///        +-- supports_claim_count    §16  the scan role
/// ```
///
/// ONE CONSTRUCTION, TWO ROLES, AND THAT IS THE POINT. Before this the absence
/// path resolved the snapshot through the structural door and stopped, while the
/// scan path separately remembered to look at `/enumeration`. The same artifact
/// was therefore refused as scan evidence and admitted as absence evidence — not
/// because the two decisions need different things, but because one consumer
/// remembered a clause and the other did not. A clause one path remembers is a
/// procedure; this is the property.
///
/// The private field and the absent constructor are what make the two role
/// predicates unreachable without qualification. A `ValidatedArtifact` alone can
/// no longer produce `Admissible::Yes` over an absence claim or
/// `ScanVerdict::ZeroClaimsMeaningful`.
struct QualifiedQuery {
    digest: String,
    /// The matched subsequence as REPLAY produced it, having been checked
    /// against the snapshot's own `matchedSnapshotDigests`. Carried rather than
    /// read back out of the artifact: the recomputation is the fact, and the
    /// claim is what it was checked against.
    matched: Vec<String>,
}

impl QualifiedQuery {
    /// §13: `NotProduced` is legal only when the named matcher, applied to the
    /// retained candidate set, yields an EMPTY matched subsequence.
    ///
    /// A qualified query whose matched set is non-empty is perfectly good
    /// evidence — of the opposite claim. The role is where that distinction
    /// lives, which is why it is not folded into qualification.
    fn supports_absence(&self) -> Result<(), String> {
        if self.matched.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the absence claim's own query snapshot {} matched {} candidate(s) {:?}; §13 \
             permits NotProduced only over an empty matched subsequence, and evidence that \
             contradicts a claim is not support for it",
            self.digest,
            self.matched.len(),
            self.matched
        ))
    }

    /// §16 and CX-4: a claim count supplied by the caller must agree with the
    /// evidence the scan cites.
    ///
    /// ONLY THE NON-NEGOTIABLE HALF is asserted. The contract does not say that
    /// one matched source yields exactly one falsification fact, so `claims`
    /// larger or smaller than `matched.len()` is not by itself a contradiction.
    /// What a loose `usize` may NOT do is independently turn retained, non-empty
    /// evidence into zero — `0` is the value that means *this surface falsifies
    /// nothing*, and it is the one value the evidence can refute on its own.
    fn supports_claim_count(&self, claims: usize) -> Result<(), String> {
        if claims > 0 || self.matched.is_empty() {
            return Ok(());
        }
        Err(format!(
            "the scan reports 0 claims and its evidence {} records {} matched candidate(s) \
             {:?}; zero is the one count the evidence can refute by itself, and a caller's \
             integer does not turn a retained match into an empty surface",
            self.digest,
            self.matched.len(),
            self.matched
        ))
    }
}

/// The ONE place a query snapshot becomes evidence.
///
/// Everything before this is [`resolve_artifact`]'s: retained, re-digested, a
/// conforming §13 object at a registered `schemaVersion`. This adds the three
/// things §13 requires before such an object can support any decision at all.
fn qualify_query<E: RetainedEvidence>(
    snapshot: &ValidatedArtifact,
    store: &E,
    declared: &[DeclaredBinding],
) -> Result<QualifiedQuery, String> {
    // 1. THE STATE. §13 puts the enumeration in the artifact and closes the
    //    vocabulary to two, precisely because a reader that accepts an
    //    unrecognised state has to decide what it means and every available
    //    default is wrong: COMPLETE manufactures authority nobody claimed,
    //    INCOMPLETE silently discards evidence.
    match snapshot.str_at("/enumeration") {
        Some("COMPLETE") => {}
        Some(other @ "INCOMPLETE") => {
            return Err(format!(
                "the query snapshot records enumeration {other}; the artifact is the \
                 authority on its own enumeration state, and a partial enumeration cannot \
                 establish what was not there"
            ))
        }
        Some(other) => {
            return Err(format!(
                "the query snapshot records enumeration {other:?}, which is not one of §13's \
                 two states; refusing it is the only reading that does not invent a fact"
            ))
        }
        None => {
            return Err(
                "the query snapshot carries no /enumeration, which §13 makes REQUIRED".to_owned(),
            )
        }
    }

    // 2. THE REPLAY RECORD, bound to the digest THIS artifact was validated
    //    under. `recorded_query` returns `None` for any other kind, so the role
    //    check upstream is not the only thing standing between a submitted
    //    review and this function.
    let recorded = snapshot
        .recorded_query()
        .ok_or_else(|| "not a query snapshot".to_owned())?
        .map_err(|e| format!("the query snapshot does not bind to its own digest: {e}"))?;

    // 3. THE CANDIDATE SET. §13 makes `allReturnedSnapshotDigests` the COMPLETE
    //    candidate set, each retained. A candidate nobody kept cannot be
    //    replayed against, so a claim resting on it is unreplayable rather than
    //    merely unverified — and replaying over the subset that happened to load
    //    is "partial success is not success" with the failure hidden inside a
    //    shorter list.
    let mut candidates = Vec::new();
    for cited in recorded.recorded_matcher().all_returned_snapshot_digests() {
        // Retention first, and named, because §13's reason is specific: this is
        // the COMPLETE candidate set, so a candidate nobody kept makes replay
        // over the rest a shorter list reported as a complete one. Same pattern
        // as `NoSuchAssessment` and `NoSuchRetentionBinding` — a bare digest in
        // a refusal does not say which link went missing.
        if store.resolve(cited).is_none() {
            return Err(format!(
                "the query snapshot declares candidate {cited} and nothing is retained under \
                 it; §13 requires the complete candidate set, so replay over the rest would \
                 be a shorter list reported as a complete one"
            ));
        }

        // THEN THE DOOR — the whole of it, not clause 1.
        //
        // This used to be `store.resolve` followed by `artifact::validate`, and
        // the comment beside it called that "the door". It was not.
        // `artifact::validate` establishes that an object is a conforming
        // artifact; `resolve_artifact` is where the ROLE, the gate
        // classification and §9.2's authority chain are, and the candidate loop
        // did not call it. So a conforming §8 projection with no
        // `RetentionBinding` — bytes somebody kept, not evidence somebody was
        // permitted to keep — could join a replay and support an absence claim.
        //
        // §5.3 places `github-query-snapshot` outside the gate precisely because
        // it holds only enumeration facts and the digests of objects that passed
        // the gate ON THEIR OWN. The candidates are the objects; being named by
        // an ungated artifact does not make them ungated.
        //
        // Nothing gate-specific is written here. `ArtifactKind::gate()` classifies
        // a complete projection as gated, and `resolve_artifact` applies §9.2 to
        // it — the same way the subject head acquires authority by being a gated
        // kind rather than by a lookup bolted into the head path.
        let mut why = Vec::new();
        let Some(candidate) =
            resolve_artifact(store, cited, ConsumedAs::QueryCandidate, declared, &mut why)
        else {
            return Err(format!(
                "the query snapshot's candidate {cited} did not pass the door: {why:?}"
            ));
        };
        let Some(candidate) = candidate.matcher_candidate() else {
            // Unreachable while `ConsumedAs::QueryCandidate` accepts exactly the
            // complete projections `matcher_candidate` answers for. Stated
            // rather than assumed away, because the two are separate tables and
            // a later kind added to one and not the other must not fall through
            // to "scored as a non-match".
            return Err(format!(
                "the query snapshot's candidate {cited} passed the candidate role and is not \
                 a complete §8 projection; the role table and the bridge disagree"
            ));
        };
        candidates.push(candidate);
    }

    // 4. THE CLAIM AGAINST THE EVIDENCE. `verify_matched` takes the claim from
    //    the artifact rather than from this caller, so what it compares is the
    //    snapshot's own `matchedSnapshotDigests` against the subsequence the
    //    registered matcher produces now.
    let verdict = verify_matched(recorded.recorded_matcher(), &candidates)
        .map_err(|e| format!("the query snapshot does not replay: {e}"))?;
    if !verdict.reproduced {
        return Err(format!(
            "the query snapshot claims {:?} matched and its own candidate set produces {:?}",
            recorded.recorded_matcher().matched_snapshot_digests(),
            verdict.replay.matched
        ));
    }

    Ok(QualifiedQuery {
        digest: snapshot.digest().to_owned(),
        matched: verdict.replay.matched,
    })
}

/// Read one pointer out of a VALIDATED record, honouring the per-field retention
/// axis.
///
/// Takes a [`ValidatedArtifact`] and not a `Value`, which is the whole point:
/// on `dc91e70` this function answered `PointerBlocked` for a reduced record
/// carrying an unassessed member, and that answer is a statement about the
/// retention semantics of an object §7 forbids to exist. It could only be
/// reached because the argument was a raw resolved value. It no longer is.
///
/// The locator is deliberately not consulted. §7.3: a locator value is not
/// surviving source evidence and MUST NOT satisfy a decision-basis pointer,
/// because otherwise `/id` sits in `blockedFields` while the same source-derived
/// id stays readable through the locator — the field gate bypassed by an alias.
fn read_pointer(record: &ValidatedArtifact, pointer: &str) -> Result<Value, Unresolved> {
    let digest_of = record.digest();
    if record.kind() != ArtifactKind::ReducedSourceRecord {
        return record
            .pointer(pointer)
            .cloned()
            .ok_or_else(|| Unresolved::PointerAbsent {
                digest: digest_of.to_owned(),
                pointer: pointer.to_owned(),
            });
    }

    let blocked = record
        .pointer("/blockedFields")
        .and_then(Value::as_array)
        .is_some_and(|fields| fields.iter().any(|f| f.as_str() == Some(pointer)));
    if blocked {
        return Err(Unresolved::PointerBlocked {
            digest: digest_of.to_owned(),
            pointer: pointer.to_owned(),
        });
    }

    match record
        .pointer("/retainedFields")
        .and_then(Value::as_object)
        .and_then(|fields| fields.get(pointer))
    {
        Some(value) => Ok(value.clone()),
        // §7.1 partitions the required set exhaustively, so a pointer in neither
        // list is not "absent from this projection" — it is a record that does
        // not account for the field at all. Refused as a retention loss, because
        // admitting it would let an incomplete partition read as evidence of
        // nothing having been blocked.
        None => Err(Unresolved::PointerBlocked {
            digest: digest_of.to_owned(),
            pointer: pointer.to_owned(),
        }),
    }
}

/// Recompute a derived fact from the sources it names, and refuse it if they do
/// not produce it.
///
/// Provenance V1 §18 requires a derived fact to name its inputs. This goes one
/// step further, and the step is the whole value: a name that is never followed
/// is satisfied equally well by the right answer and by a citation that does not
/// support it. `carries_finding` is the fact §18 itself names, the classifier is
/// documented as never inferring it from body text, and that puts the entire
/// weight on an adapter's assertion.
fn check_derived<E: RetainedEvidence>(
    store: &E,
    fact: &DerivedFact,
    declared: &[DeclaredBinding],
    into: &mut Vec<Unresolved>,
) {
    let Some(entry) = derivations::resolve(&fact.derivation, &fact.version) else {
        into.push(Unresolved::UnknownDerivation {
            derivation: fact.derivation.clone(),
            version: fact.version.clone(),
        });
        return;
    };

    if fact.derived_from.len() != entry.arity() {
        into.push(Unresolved::DerivationArityMismatch {
            derivation: fact.derivation.clone(),
            expected: entry.arity(),
            cited: fact.derived_from.len(),
        });
        return;
    }

    let mut sources = Vec::with_capacity(entry.arity());
    for (inputs, source_digest) in entry.sources.iter().zip(&fact.derived_from) {
        // Every cited source goes through the same resolution the decision's own
        // inputs do. A derived fact resting on bytes nobody was permitted to keep
        // is not better evidenced than a decision resting on them.
        let Some(record) = resolve_artifact(
            store,
            source_digest,
            ConsumedAs::GatedSource,
            declared,
            into,
        ) else {
            return;
        };

        match derivation_source_view(&record, inputs) {
            Ok(view) => sources.push(view),
            Err(ViewError::Input(why)) => {
                into.push(why);
                into.push(Unresolved::DerivationInputUnavailable {
                    derivation: fact.derivation.clone(),
                    version: fact.version.clone(),
                    source_digest: source_digest.clone(),
                });
                return;
            }
            // The registry entry, not the artifact. Nothing was lost and nothing
            // disagreed; the crate simply cannot assemble what the rule reads.
            Err(ViewError::Undeclarable) => {
                into.push(Unresolved::DerivationCannotRecompute {
                    derivation: fact.derivation.clone(),
                    version: fact.version.clone(),
                });
                return;
            }
        }
    }

    match (entry.derive)(&sources) {
        Some(recomputed) if recomputed == fact.value => {}
        Some(recomputed) => into.push(Unresolved::DerivationDisagrees {
            derivation: fact.derivation.clone(),
            claimed: fact.value.clone(),
            recomputed,
        }),
        // Every declared input resolved and the rule still produced nothing.
        // Not `false`: a rule that could not run has established nothing, and
        // reporting that as the negative outcome is the
        // absent-signal-as-negative-result error. Not `DerivationDisagrees`
        // either — nothing disagreed.
        None => into.push(Unresolved::DerivationCannotRecompute {
            derivation: fact.derivation.clone(),
            version: fact.version.clone(),
        }),
    }
}

/// THE DERIVATION SOURCE VIEW: one validated artifact, read in ITS OWN
/// representation, projected into the vocabulary the rule reads.
///
/// ```text
/// ValidatedArtifact
///       |
///       +-- CompleteProjection   read input.canonical   §8 member names
///       +-- ReducedSourceRecord  read input.decoded     §5.3 retainedFields
///       |
///       v
/// { input.canonical: value, .. }   <- the only thing the rule ever sees
/// ```
///
/// WHY THIS EXISTS. `check_derived` used to hand the rule the artifact's whole
/// raw object, which threw away the one distinction between the two
/// representations. A reduced record keyed in §5.3's decoded space then answered
/// nothing to a rule reading canonical members, and a fact whose every input had
/// survived the gate came back as if the evidence were gone. Redaction §8 says
/// exactly the opposite: facts derived solely from fields that survived §7.1 are
/// unaffected.
///
/// WHY IT READS `retainedFields` AND NOTHING ELSE. §7.3 and §8: the locator is
/// identity, not surviving evidence, so a record whose `/id` was blocked does
/// not answer for it out of `locator.stableId` even though the two describe the
/// same number. Delegating to [`read_pointer`] is what makes that true here
/// rather than true again — it is the same reader the decision's own inputs go
/// through, so the field gate cannot be honoured in one path and forgotten in
/// the other.
///
/// WHY THE VIEW IS BUILT RATHER THAN PASSED THROUGH, even for a complete
/// projection whose members are already canonical. The rule then reads exactly
/// the fields its registry entry declares, in both representations, over one
/// code path. A rule that quietly started consulting a field nobody declared
/// would stop working immediately instead of working until somebody redacted
/// that field.
fn derivation_source_view(
    record: &ValidatedArtifact,
    inputs: &[DerivationInput],
) -> Result<Value, ViewError> {
    let mut view = Value::Object(Map::new());
    for input in inputs {
        let pointer = match record.kind() {
            ArtifactKind::ReducedSourceRecord => input.decoded,
            _ => input.canonical,
        };
        let value = read_pointer(record, pointer).map_err(ViewError::Input)?;
        place(&mut view, input.canonical, value).ok_or(ViewError::Undeclarable)?;
    }
    Ok(view)
}

/// Why a source view could not be built — and the two reasons are about
/// different things, which is the whole reason they are not one.
enum ViewError {
    /// A field the rule reads did not resolve in the artifact. A fact about the
    /// EVIDENCE, carrying the pointer-level refusal that says which and why.
    Input(Unresolved),
    /// A registry entry declared a `canonical` name that is not a pointer this
    /// crate can write, or two that collide. A fact about the DECLARATION, held
    /// unreachable by `tests/derivation_source_view.rs` rather than assumed
    /// away.
    Undeclarable,
}

/// Write a value at an RFC 6901 pointer, creating intermediate objects.
///
/// `None` when the pointer is not one (no leading `/`) or crosses a member that
/// already exists and is not an object.
fn place(target: &mut Value, pointer: &str, value: Value) -> Option<()> {
    let segments: Vec<String> = pointer
        .strip_prefix('/')?
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();
    place_segments(target, &segments, value)
}

fn place_segments(target: &mut Value, segments: &[String], value: Value) -> Option<()> {
    let (head, rest) = segments.split_first()?;
    let map = target.as_object_mut()?;
    if rest.is_empty() {
        map.insert(head.clone(), value);
        return Some(());
    }
    let child = map
        .entry(head.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    place_segments(child, rest, value)
}

/// Bind one decision basis to retained evidence.
///
/// Every input this decision read must resolve to retained bytes, under a digest
/// the BASIS named, authorised by the assessment the store records — and every
/// derived fact must be reproducible from the sources it cites. Anything else is
/// `CANNOT_CHECK` for this decision and nothing wider: redaction policy §10 is
/// explicit that the gate does not determine the state, and a blocked field this
/// decision never read does not sweep the observation aside.
///
/// Every failure is collected rather than short-circuited. A caller that fixes
/// the first refusal and re-runs would otherwise discover the second only on the
/// next attempt, which is how a chain of custody gets repaired one link at a
/// time without anybody seeing its length.
pub fn admissibility<E: RetainedEvidence>(basis: &DecisionBasis, store: &E) -> Admissible {
    let mut why = Vec::new();
    let mut values = Vec::new();

    for input in &basis.inputs {
        let Some(record) = resolve_artifact(
            store,
            &input.source_digest,
            ConsumedAs::GatedSource,
            &basis.bindings,
            &mut why,
        ) else {
            continue;
        };
        match read_pointer(&record, &input.pointer) {
            Ok(value) => values.push(value),
            Err(e) => why.push(e),
        }
    }

    for fact in &basis.derived {
        check_derived(store, fact, &basis.bindings, &mut why);
    }

    // The expected query digest is named by the basis and resolved like any
    // other record. It is NOT read out of the store: that direction is the
    // authority path this slice exists to establish, and reversing it would make
    // the store the author of its own expectation.
    if let Some(expected) = &basis.expected_query_digest {
        if let Some(snapshot) = resolve_artifact(
            store,
            expected,
            ConsumedAs::ExpectedQuerySnapshot,
            &basis.bindings,
            &mut why,
        ) {
            // AND THEN THE ROLE. Structural validity was never the question here:
            // §13 makes an INCOMPLETE snapshot a well-formed record that simply
            // cannot evidence an absence, and a snapshot whose own candidates
            // contradict its claim is well-formed too. This is the same
            // qualification the scan path consumes, so the two cannot come to
            // disagree about one artifact.
            let qualified = qualify_query(&snapshot, store, &basis.bindings)
                .and_then(|q| q.supports_absence().map(|()| q));
            if let Err(reason) = qualified {
                why.push(Unresolved::QueryDoesNotSupportRole {
                    digest: expected.clone(),
                    role: ConsumedAs::ExpectedQuerySnapshot.name(),
                    why: reason,
                });
            }
        }
    }

    if why.is_empty() {
        Admissible::Yes { values }
    } else {
        Admissible::CannotCheck { why }
    }
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
    /// What the scan says it enumerated.
    ///
    /// Structured rather than a display string because it is CHECKED against the
    /// evidencing snapshot's own binding. An unchecked label would admit a real,
    /// complete, correctly-digested scan of a DIFFERENT query — evidence that
    /// resolves and answers another question.
    pub binding: QueryBinding,
    pub completeness: ScanCompleteness,
    /// The query snapshot digest this scan is evidenced by.
    ///
    /// A QUERY snapshot specifically, not any source snapshot: a scan is a claim
    /// about what an enumeration returned, and only a query snapshot carries the
    /// binding and enumeration state that claim rests on. A single source object
    /// cannot evidence a scan of a surface.
    pub snapshot_digest: String,
}

/// What a scan claims to have enumerated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryBinding {
    pub repository: String,
    pub pull_request: String,
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

/// What a scan is permitted to mean, given how far it actually got AND what it
/// is evidenced by.
///
/// Completeness is read first, then the evidence, and only then the claim count.
/// Reading the count first was the original defect: zero is the same number
/// whether the surface was empty or the request died, and once it has been
/// reported as an answer nothing downstream can recover which it was.
///
/// RESOLVING THE SNAPSHOT IS NOT ENOUGH. §16 requires a scan to be evidenced by a
/// retained snapshot, and a snapshot that resolves may still be about a different
/// pull request or a different surface — real, complete, correctly digested, and
/// answering another question. So the scan's declared binding is checked against
/// the snapshot's own. "The scan is genuine, just of another query" is a distinct
/// escape from "the scan is unevidenced", and only the relation closes it.
pub fn scan_verdict<E: RetainedEvidence>(
    scan: &FalsificationSurfaceScan,
    claims: usize,
    store: &E,
) -> ScanVerdict {
    match &scan.completeness {
        ScanCompleteness::Incomplete { reason } => {
            return ScanVerdict::CannotCheck {
                why: format!(
                    "the {} scan did not complete ({reason}), so {claims} is a lower bound and \
                     not a total; zero claims from it would say nothing about the surface",
                    scan.surface
                ),
            }
        }
        ScanCompleteness::Failed { reason } => {
            return ScanVerdict::CannotCheck {
                why: format!(
                    "the {} scan failed ({reason}); a failed query that yielded no values is \
                     not an empty result",
                    scan.surface
                ),
            }
        }
        ScanCompleteness::Complete => {}
    }

    // The evidence is qualified BEFORE the count is looked at, and the count is
    // then checked against it. Reading the count first was the original defect:
    // zero is the same number whether the surface was empty or the request died.
    // Reading it after qualification but not against the evidence was the next
    // one — CX-4 — where a caller's `usize` turned a snapshot recording a match
    // into a fact about an empty surface.
    let qualified = match check_scan_evidence(scan, store) {
        Ok(qualified) => qualified,
        Err(why) => return ScanVerdict::CannotCheck { why },
    };
    if let Err(why) = qualified.supports_claim_count(claims) {
        return ScanVerdict::CannotCheck { why };
    }

    if claims == 0 {
        ScanVerdict::ZeroClaimsMeaningful
    } else {
        ScanVerdict::Claims(claims)
    }
}

/// The snapshot a scan names came through the door, and answers THIS scan's
/// query.
///
/// Everything before the relation checks — retained, re-digested, a conforming
/// §13 query snapshot at a registered version, with its pagination, matcher and
/// digest arrays all present — is [`resolve_artifact`]'s job now. On `dc91e70`
/// this function checked the kind tag and three members, so a snapshot with no
/// pagination, no matcher and no digest arrays evidenced a COMPLETE scan.
fn check_scan_evidence<E: RetainedEvidence>(
    scan: &FalsificationSurfaceScan,
    store: &E,
) -> Result<QualifiedQuery, String> {
    let mut why = Vec::new();
    let Some(snapshot) = resolve_artifact(
        store,
        &scan.snapshot_digest,
        ConsumedAs::ScanEvidence,
        &[],
        &mut why,
    ) else {
        return Err(format!(
            "the {} scan's evidence did not pass validation: {why:?}. §16 requires the scan \
             to be evidenced, and a COMPLETE flag on its own is a caller's assertion about \
             itself",
            scan.surface
        ));
    };

    // The relation. Everything above establishes that the evidence is real; this
    // establishes that it is evidence about THIS query.
    let evidenced_surface = snapshot.pointer("/surface").and_then(Value::as_str);
    if evidenced_surface != Some(scan.surface.as_str()) {
        return Err(format!(
            "the scan claims surface {:?} and its evidence is a snapshot of {:?}",
            scan.surface,
            evidenced_surface.unwrap_or("<absent>")
        ));
    }
    let repository = snapshot
        .pointer("/binding/repository")
        .and_then(Value::as_str);
    let pull_request = snapshot
        .pointer("/binding/pullRequest")
        .and_then(Value::as_str);
    if repository != Some(scan.binding.repository.as_str())
        || pull_request != Some(scan.binding.pull_request.as_str())
    {
        return Err(format!(
            "the scan claims {}#{} and its evidence is a snapshot of {}#{}; a real scan of \
             another query is not evidence about this one",
            scan.binding.repository,
            scan.binding.pull_request,
            repository.unwrap_or("<absent>"),
            pull_request.unwrap_or("<absent>")
        ));
    }

    // THE STATE AND THE CLAIM, through the SAME qualification the absence path
    // consumes. `ScanCompleteness::Complete` is the CALLER's account of how far
    // it got; §13 puts the enumeration in the artifact and says the rule turns
    // on that value. Everything above this line is the scan's own relation —
    // surface and binding — because those are facts about a query the absence
    // path does not have; everything §13 says about the artifact itself is not
    // repeated here.
    // A scan carries no decision basis, so it declares no bindings. The retained
    // binding is the authority either way; `declared` only ever ADDS the
    // obligation that a basis's own claim about which assessment authorised a
    // record agrees with the retained one, and a caller with no claim makes none.
    qualify_query(&snapshot, store, &[])
}

// ---- Subject read — provenance V1 §8.1.

/// One head read, which either happened or did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadRead {
    /// A read that happened, named by the digest of its retained `HeadReadEvent`.
    ///
    /// A REFERENCE and never a SHA. The earlier `Observed(String)` let a caller
    /// assert two matching heads with nothing retained behind either; §8.1
    /// requires a durable event per read, and an event with
    /// `acquisition = AVAILABLE` carries a REQUIRED `snapshotDigest`. The SHA is
    /// read out of that chain or it is not read at all.
    Observed { event_digest: String },
    /// §8.1: a failed head read records a reason and carries no
    /// `snapshotDigest` — there are no bytes to point at.
    Failed { reason: String },
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

/// The subject an evaluation is about, supplied INDEPENDENTLY of the evidence.
///
/// The whole point is where this comes from. Deriving the target repository and
/// pull request from the same retained events being checked is the party under
/// examination supplying the identity it is examined against: two head reads of
/// some other pull request agree with each other perfectly, and a subject read
/// that never touched this subject reports that it did not move.
///
/// So the caller states the subject, and the retained artifacts are checked
/// against it. `expected_sha` travels with the other two because a SHA without a
/// repository and a pull request does not identify a subject either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub repository: String,
    pub pull_request: String,
    pub expected_sha: String,
}

/// Whether the subject moved, given two head reads that may not both have
/// happened.
///
/// A successful read is a REFERENCE to a retained `HeadReadEvent`, and the SHA is
/// read out of that chain: event -> `snapshotDigest` -> `github-pull-request-head`
/// -> `headSha`. Each hop is resolved and re-digested. The earlier shape took the
/// SHA from the caller, so two fabricated matching values reported `NotStale`
/// with the store never consulted — the party whose subject was in question
/// supplying the evidence that it had not changed.
///
/// BOTH reads must resolve before either answer is available. Absence of a
/// contradiction is not evidence of consistency, and a read that did not happen
/// is exactly where the head is most likely to have moved unobserved.
///
/// An observed contradiction still wins when the other end failed: a fact is a
/// fact, and discarding it because its partner broke is the mirror-image error.
pub fn staleness<E: RetainedEvidence>(
    subject: &Subject,
    read: &SubjectRead,
    store: &E,
) -> Staleness {
    let expected_sha = subject.expected_sha.as_str();
    let mut observed = Vec::new();
    let mut unresolved = Vec::new();

    // §8.1's "two reads are not two pointers" needs NO separate check here, and
    // saying so is worth more than the check was. Each slot below requires its
    // event to carry that slot's OWN role, and one event carries one role — so a
    // single event offered as both reads is already refused, by the after slot,
    // for naming a HEAD_BEFORE event. An explicit distinctness test would have no
    // reachable failure. Mutation testing is what found it: deleting the
    // distinctness check broke nothing, because the role check was already doing
    // the work, and a check that cannot fire is not evidence of the property it
    // is named after.

    for (slot, expected_role, r) in [
        ("head_before", "HEAD_BEFORE", &read.before),
        ("head_after", "HEAD_AFTER", &read.after),
    ] {
        match r {
            HeadRead::Failed { reason } => {
                unresolved.push(format!("{slot} did not happen ({reason})"));
            }
            HeadRead::Observed { event_digest } => {
                match resolve_head_sha(store, event_digest, expected_role, subject) {
                    Ok(sha) => observed.push(sha),
                    Err(why) => unresolved.push(format!("{slot} {why}")),
                }
            }
        }
    }

    // A resolved disagreement is decisive even if the other end did not resolve.
    for sha in &observed {
        if sha != expected_sha {
            return Staleness::Stale {
                observed: sha.clone(),
            };
        }
    }

    if unresolved.is_empty() {
        Staleness::NotStale
    } else {
        Staleness::CannotCheck {
            why: format!(
                "{}; nothing witnesses that the head did not move, and an unread head is not \
                 an unchanged one",
                unresolved.join("; ")
            ),
        }
    }
}

/// One head read, followed from its event to the head projection that carries
/// the SHA — both through the door.
///
/// CX-6 IS CLOSED HERE BY THE CLASSIFICATION, NOT BY A CHECK ADDED HERE. §5.3's
/// gated set includes `github-pull-request-head`, so [`ArtifactKind::gate`]
/// returns `Gated` for it and [`resolve_artifact`] therefore requires §9.2
/// authority for the snapshot exactly as it does for a submitted review. There
/// is deliberately no `binding_for` call in this function: bolting one in would
/// be the same pattern the fourth round removes — a rule remembered once per
/// consumer — and the next path added would need it remembered again.
///
/// The event is ungated: it is an acquisition record, not a fetched source, and
/// §5.3 gives it no required field set. Its two shapes are chosen by its own
/// `acquisition`, so an AVAILABLE event with no `snapshotDigest` and a FAILED
/// event carrying one are both refused at the door.
fn resolve_head_sha<E: RetainedEvidence>(
    store: &E,
    event_digest: &str,
    expected_role: &str,
    subject: &Subject,
) -> Result<String, String> {
    let mut why = Vec::new();
    let Some(event) = resolve_artifact(
        store,
        event_digest,
        ConsumedAs::HeadReadEvent,
        &[],
        &mut why,
    ) else {
        return Err(format!("did not pass validation: {why:?}"));
    };

    // §8.1 tags every event HEAD_BEFORE or HEAD_AFTER. A slot that never reads
    // the tag accepts the after-read as the before-read, and the pair then
    // brackets no interval at all.
    match event.str_at("/role") {
        Some(role) if role == expected_role => {}
        Some(role) => {
            return Err(format!(
                "names an event whose role is {role}, and this slot is the {expected_role} read"
            ))
        }
        None => return Err("names an event with no /role, which §8.1 requires".to_owned()),
    }

    // §8.1: only an AVAILABLE event carries a snapshotDigest. The closed form
    // already guarantees the pairing; this is the read, not a second check.
    if event.str_at("/acquisition") != Some("AVAILABLE") {
        return Err("is an event whose acquisition is not AVAILABLE".to_owned());
    }
    let snapshot_digest = event
        .str_at("/snapshotDigest")
        .ok_or_else(|| "is an AVAILABLE event with no snapshotDigest".to_owned())?
        .to_owned();

    let mut why = Vec::new();
    let Some(snapshot) = resolve_artifact(
        store,
        &snapshot_digest,
        ConsumedAs::SubjectHead,
        &[],
        &mut why,
    ) else {
        return Err(format!(
            "names a head snapshot that did not pass validation and authority: {why:?}"
        ));
    };

    // THE SUBJECT. §8.1 makes `repository` and `pullRequest` REQUIRED on this
    // projection. Two real, correctly digested, correctly roled reads of ANOTHER
    // pull request agree with each other perfectly and report that this subject
    // did not move. The identity they are checked against is the caller's,
    // precisely so it is not taken from the artifacts under examination.
    let repository = snapshot.str_at("/repository");
    let pull_request = snapshot.str_at("/pullRequest");
    if repository != Some(subject.repository.as_str())
        || pull_request != Some(subject.pull_request.as_str())
    {
        return Err(format!(
            "names a head snapshot of {}#{}, and the subject is {}#{}; a consistent pair of \
             reads of another subject says nothing about this one",
            repository.unwrap_or("<absent>"),
            pull_request.unwrap_or("<absent>"),
            subject.repository,
            subject.pull_request
        ));
    }

    snapshot
        .str_at("/headSha")
        .map(str::to_owned)
        .ok_or_else(|| "names a head snapshot with no headSha".to_owned())
}

#[cfg(test)]
mod place_tests {
    //! `place` is private and every current declaration writes a single
    //! top-level member, so the nested and malformed paths have no reachable
    //! caller yet. They are not speculative — §5.3's required sets are full of
    //! `/user/login` and `/head/sha`, and §8's projections nest `user` — but
    //! they are unexercised, and unexercised behaviour at a boundary is how a
    //! source view comes to silently drop a field. Tested here rather than
    //! assumed.

    use super::place;
    use serde_json::{json, Map, Value};

    fn empty() -> Value {
        Value::Object(Map::new())
    }

    #[test]
    fn a_nested_pointer_creates_the_objects_it_runs_through() {
        let mut v = empty();
        assert!(place(&mut v, "/user/login", json!("octocat")).is_some());
        assert!(place(&mut v, "/user/id", json!("1")).is_some());
        assert_eq!(v, json!({"user": {"login": "octocat", "id": "1"}}));
    }

    #[test]
    fn rfc_6901_escapes_are_decoded() {
        let mut v = empty();
        assert!(place(&mut v, "/a~1b", json!(1)).is_some());
        assert!(place(&mut v, "/c~0d", json!(2)).is_some());
        assert_eq!(v, json!({"a/b": 1, "c~d": 2}));
    }

    #[test]
    fn a_string_that_is_not_a_pointer_is_refused() {
        let mut v = empty();
        assert!(place(&mut v, "stableId", json!("x")).is_none());
        assert_eq!(v, empty(), "a refused write leaves nothing behind");
    }

    /// Two declared inputs where one's path runs through the other's value.
    ///
    /// Refused rather than overwritten: silently replacing `user` with an object
    /// would hand the rule a view in which one declared field had vanished, and
    /// a rule reading a field that is not there yields `None`, which this round
    /// exists to stop reporting as an answer.
    #[test]
    fn a_pointer_crossing_a_non_object_is_refused() {
        let mut v = empty();
        assert!(place(&mut v, "/user", json!("octocat")).is_some());
        assert!(place(&mut v, "/user/login", json!("octocat")).is_none());
    }
}
