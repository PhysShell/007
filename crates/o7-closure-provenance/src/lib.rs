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
use o7_closure_matcher::{verify_matched, ImplementationCheck};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

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
    /// §7.3's acquisition locator: which object these bytes are supposed to be.
    ///
    /// Required rather than optional, because a citation that cannot say which
    /// object it asked for cannot be checked against §7.3 — and an optional
    /// expectation is the one nobody sets.
    pub locator: AcquisitionLocator,
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
    /// The sources this fact claims to be derived from, in the order the
    /// derivation consumes them — each naming both the bytes and, per §7.3, the
    /// object they are supposed to be.
    pub derived_from: Vec<CitedSource>,
}

/// The binding a decision basis asserts between a retained record and the
/// assessment that authorised retaining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredBinding {
    pub record_digest: String,
    pub assessment_digest: String,
}

/// Which of §17's minimum decision bases a caller is asking this basis to be.
///
/// SUPPLIED BY THE CALLER, NEVER READ OFF THE BASIS, and that is the whole
/// design. `DecisionBasis` already carries `observation_id`, and selecting the
/// requirements from it would let the object under examination nominate the
/// standard it is examined against — an adapter saying "I am a check, so here
/// is what I owe", which is self-certification with a table in front of it.
/// §17.1's first consequence says the same thing about the subject and for the
/// same reason: the expectation arrives from outside the artifacts being
/// checked.
///
/// `observation_id` is still compared against a query snapshot's
/// `requiredObservationId`. That is a relation between two artifacts, not a
/// selection of the rule, and the difference is the point.
///
/// §17 tabulates four rows. Two of them describe a `DecisionBasis` and are
/// here; `subject` and `falsification` describe the arguments of `staleness`
/// and `scan_verdict`, which take no basis at all and so cannot be profiles of
/// one. `tests/correction_g2.rs` records that split rather than leaving the two
/// missing rows to be read as an oversight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionProfile {
    /// §17: `check` — observed head_sha, observed conclusion.
    Check,
    /// §17: `review` — observed commit_id, derived carries_finding.
    Review,
    /// §17: `absence` — expected query snapshot digest.
    ///
    /// The one decision whose subject is that no object was found, and therefore
    /// the one with no observed field to require: demanding `commit_id` and a
    /// derived `carries_finding` would demand evidence OF THE OBJECT the
    /// decision says is not there. `Review` is not a substitute for this
    /// profile, it is its opposite.
    Absence,
}

impl DecisionProfile {
    /// §17's own name for this row, used in refusals so the reader can find it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Review => "review",
            Self::Absence => "absence",
        }
    }

    /// What §17's minimum decision basis requires for this profile.
    #[must_use]
    pub const fn requires(self) -> &'static [BasisRequirement] {
        match self {
            Self::Check => CHECK_BASIS,
            Self::Review => REVIEW_BASIS,
            Self::Absence => ABSENCE_BASIS,
        }
    }
}

/// One entry of §17's minimum decision basis.
#[derive(Debug, Clone, Copy)]
pub struct BasisRequirement {
    /// §17's own wording for this requirement, carried into refusals so a reader
    /// can find the row in the contract instead of in this crate.
    pub name: &'static str,
    pub needs: Needs,
}

/// What satisfying a [`BasisRequirement`] takes.
#[derive(Debug, Clone, Copy)]
pub enum Needs {
    /// A field observed on a surface.
    ///
    /// THE SURFACE IS PART OF THE REQUIREMENT, not decoration. §8.1's
    /// `github-pull-request-head` carries a `headSha` too, and it is the pull
    /// request's rather than the check's; a rule that matched pointer names
    /// alone would count it and report a check decision as fully evidenced.
    ///
    /// Two spellings for the same reason `DerivationInput` has two: a complete
    /// §8 projection is keyed canonically and a §7 reduced record in §5.3's
    /// decoded space. Requiring the canonical name alone would refuse every
    /// reduced record and call it completeness, against redaction §8's
    /// requirement that a decision whose inputs survived the gate stay makeable.
    Observed {
        surface: &'static str,
        canonical: &'static str,
        decoded: &'static str,
    },
    /// A derived fact, by derivation id.
    ///
    /// The id only. Whether the named version exists, is bound to its
    /// implementation, and actually recomputes the claim is `check_derived`'s
    /// question, and answering it twice in two places is how two answers start
    /// disagreeing.
    Derived { derivation: &'static str },
    /// The basis names the query snapshot an absence claim rests on.
    ///
    /// PRESENCE ONLY, and the narrowness is the design. Whether that snapshot
    /// is COMPLETE, whether its matcher is bound to its implementation, whether
    /// replay reproduces the recorded selection, whether its
    /// `requiredObservationId` is this decision's, and whether its matched set
    /// is empty are §13 and §14 questions, imposed a few lines below by
    /// `qualify_query` and `supports_absence`. Restating any of them here would
    /// be a second copy of a frozen rule, and two copies of one rule is how the
    /// two answers begin to differ.
    ///
    /// The division: §13/§14 say what the snapshot must BE, this says what the
    /// basis must PROVIDE before those questions have a subject at all. A basis
    /// naming no snapshot does not fail the §13 checks — it never reaches them.
    ExpectedQuerySnapshot,
}

/// §17: `check` — observed head_sha, observed conclusion.
const CHECK_BASIS: &[BasisRequirement] = &[
    BasisRequirement {
        name: "observed head_sha",
        needs: Needs::Observed {
            surface: "github-actions-check",
            canonical: "/headSha",
            decoded: "/head_sha",
        },
    },
    BasisRequirement {
        name: "observed conclusion",
        needs: Needs::Observed {
            surface: "github-actions-check",
            canonical: "/conclusion",
            decoded: "/conclusion",
        },
    },
];

/// §17: `absence` — expected query snapshot digest.
///
/// ONE REQUIREMENT, deliberately. See [`Needs::ExpectedQuerySnapshot`] for why
/// the §13/§14 obligations are not repeated here.
const ABSENCE_BASIS: &[BasisRequirement] = &[BasisRequirement {
    name: "expected query snapshot digest",
    needs: Needs::ExpectedQuerySnapshot,
}];

/// §17: `review` — observed commit_id, derived carries_finding.
const REVIEW_BASIS: &[BasisRequirement] = &[
    BasisRequirement {
        name: "observed commit_id",
        needs: Needs::Observed {
            surface: "github-submitted-review",
            canonical: "/commitId",
            decoded: "/commit_id",
        },
    },
    BasisRequirement {
        name: "derived carries_finding",
        needs: Needs::Derived {
            derivation: "review-carries-finding",
        },
    },
];

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
    /// For an absence claim: the query snapshot the claim rests on, and the
    /// subject it must be an enumeration OF.
    ///
    /// It lives HERE and not in the store — that placement is the authority path
    /// this slice exists to establish.
    ///
    /// The digest and the subject are ONE member and not two optional ones so
    /// that a basis cannot name a snapshot without saying what change it is
    /// supposed to be about. Two independent options are two things that can
    /// disagree, and the one nobody sets is the one that stops being checked.
    pub expected_query: Option<ExpectedQuery>,
    pub bindings: Vec<DeclaredBinding>,
    /// The redaction policy version this decision is made under, per §9.5.
    ///
    /// Beside `expected_query` for the same reason it exists: §9.4
    /// requires every field of an assessment to have a range independent of the
    /// content inspected, and this member had none. The value cannot come from
    /// the assessment — that is the party that would be leaking — and cannot be
    /// a literal in this crate, because no contract sentence registers one. It
    /// comes from the caller, exactly as `expected_sha` and the
    /// `DecisionProfile` do.
    pub expected_redaction_policy: String,
    /// The detector identity this evaluation accepts, per §9.4. See
    /// [`ExpectedDetector`].
    pub expected_detector: ExpectedDetector,
}

/// The query snapshot an absence claim rests on, and the subject it must
/// enumerate.
///
/// §17's `absence` row requires the basis to name the snapshot. §13 makes the
/// snapshot's `binding` part of what it IS, and §17.1 requires the subject to
/// arrive from OUTSIDE the artifact being checked — so the digest alone cannot
/// establish that a complete, empty enumeration is an enumeration of THIS
/// change. Observation ids are reusable across pull requests; a subject is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedQuery {
    pub digest: String,
    /// The repository and pull request this decision is about, supplied by the
    /// caller exactly as `staleness` takes its subject and a falsification scan
    /// declares its binding.
    pub subject: QueryBinding,
}

/// The detector identity an evaluation accepts, supplied by the caller.
///
/// §9.4 requires every field of an assessment to be "a closed vocabulary value,
/// a structural identifier, a boolean, or a JSON pointer", and says outright
/// that "no field of an assessment is free text". All three `detector` members
/// were declared `Text`, which admits every string — so a producer could write a
/// credential into any of them and the assessment carrying it is canonicalized,
/// digested and retained permanently as the authority for a record a decision
/// then reads.
///
/// THE VALUES ARE THE CALLER'S FOR K1'S REASON. No contract sentence registers a
/// detector id, version or configuration digest, so a literal here would be this
/// implementation inventing the norm it enforces; taking them from the
/// assessment would be asking the party that would be leaking to nominate its
/// own range. §17.1: the expectation arrives from outside the artifacts being
/// checked.
///
/// WHAT THIS IS NOT. It does not resolve `configDigest`, does not bind any of
/// the three to anything that ran, and does not discharge DET-BIND. It closes
/// the RANGE and nothing else: after it, these members can carry only what the
/// caller already knew, which is what §9.4 asks of them and all it asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedDetector {
    pub id: String,
    pub version: String,
    /// Still a recorded claim whose referenced configuration is not resolved —
    /// §23's residual is unchanged. Constraining what the member may CARRY is a
    /// different question from resolving what it REFERS to.
    pub config_digest: String,
}

/// The object a citation says it asked for — §7.3's locator, from the caller.
///
/// §7.3 states three normative rules and this is the third:
///
/// ```text
/// locator MUST equal the acquisition locator of that source
/// ```
///
/// and says why in the sentence after it: "shape alone is not identity. A record
/// whose locator has the right keys and the wrong values, or the right values
/// under the wrong `locatorKind`, is a well-formed pointer at the wrong object —
/// which is worse than a missing one, because it resolves."
///
/// THE THREE VARIANTS ARE §7.3'S THREE SHAPES, so a locator that does not name
/// one of the five kinds is not representable. `Check` has no pull request and
/// `Head` has no stable id, both for the reasons §7.3 gives: a check is
/// identified by repository and id, and inventing a synthetic id for the subject
/// head would contradict the merged §8.1 schema.
///
/// IT COMES FROM THE CALLER, like every other expectation this crate compares
/// against an artifact. Deriving it from the record under examination would be
/// the record supplying the identity it is checked against — §17.1's first
/// consequence, at the one member whose whole job is identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquisitionLocator {
    /// `github-issue-comment`, `github-submitted-review`, `github-review-comment`.
    InPullRequest {
        repository: String,
        pull_request: String,
        stable_id: String,
    },
    /// `github-actions-check`.
    Check {
        repository: String,
        stable_id: String,
    },
    /// `github-pull-request-head`.
    Head {
        repository: String,
        pull_request: String,
    },
}

impl AcquisitionLocator {
    /// The §7.3 member names and values this locator asserts, in §7.3's order.
    ///
    /// Returned as pairs rather than compared here so the comparison can range
    /// over the SHAPE the record's `locatorKind` selects: a caller's `Check`
    /// locator against a record declaring `github-submitted-review` must fail on
    /// the members, not on a variant name this crate chose.
    #[must_use]
    pub fn members(&self) -> Vec<(&'static str, &str)> {
        match self {
            Self::InPullRequest {
                repository,
                pull_request,
                stable_id,
            } => vec![
                ("repository", repository.as_str()),
                ("pullRequest", pull_request.as_str()),
                ("stableId", stable_id.as_str()),
            ],
            Self::Check {
                repository,
                stable_id,
            } => vec![
                ("repository", repository.as_str()),
                ("stableId", stable_id.as_str()),
            ],
            Self::Head {
                repository,
                pull_request,
            } => vec![
                ("repository", repository.as_str()),
                ("pullRequest", pull_request.as_str()),
            ],
        }
    }
}

/// A source a decision or a derived fact cites: which object, and which bytes.
///
/// The digest and the locator travel together for `ExpectedQuery`'s reason — a
/// citation that names bytes without saying which object they are supposed to be
/// cannot be checked against §7.3, and two independent fields are two things
/// that can disagree, with the one nobody sets being the one that stops being
/// checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitedSource {
    pub digest: String,
    pub locator: AcquisitionLocator,
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

/// What the caller asserts about retention, supplied independently of the store
/// that holds the evidence being judged.
///
/// TWO EXPECTATIONS THAT TRAVEL TOGETHER because they are consumed together, at
/// the one place a gated record is authorised. Threading them as two adjacent
/// borrowed parameters through five call sites is how the wrong one gets passed
/// once and nobody notices.
///
/// Both obey §17.1's first consequence: the expectation arrives from OUTSIDE the
/// artifacts being checked. A store that could supply either would be selecting
/// the standard it is examined against.
#[derive(Debug, Clone, Copy)]
struct Expected<'a> {
    /// §9.2 bindings the caller declares between a record and its assessment.
    declared: &'a [DeclaredBinding],
    /// §9.4's range for the `detector` block, from the caller.
    detector: &'a ExpectedDetector,
    /// §9.5's `redactionPolicyVersion` this evaluation is made under.
    ///
    /// WHY THE VALUE COMES FROM HERE AND NOT FROM A TABLE IN THIS CRATE. §9.4
    /// requires every field of an assessment to have a range that does not
    /// depend on the content inspected, and `redactionPolicyVersion` had none:
    /// `Text` admits every string, so a producer could write a credential into
    /// it and the assessment carrying it is retained permanently. No contract
    /// sentence registers a permitted value — §9 declares the member, §9.5
    /// relates it to the record's, and neither names one — so a literal here
    /// would be this implementation inventing the norm it claims to enforce.
    /// The caller names the policy the decision is made under, exactly as it
    /// names `expected_sha`, `expected_query` and the `DecisionProfile`.
    policy: &'a str,
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
    /// A reduced record's §7.3 locator names an object other than the one the
    /// citation asked for.
    ///
    /// A RELATION failure and not a malformed artifact, and the difference is
    /// §17.1's two classes. The record is well formed: its locator has exactly
    /// the members §7.3 gives its kind, and every one of them is a string. What
    /// it is not is the object this decision cited — "a well-formed pointer at
    /// the wrong object, which is worse than a missing one, because it
    /// resolves". Reporting that as malformed would say the producer emitted
    /// something broken, when what happened is that a correct artifact was
    /// consumed under a citation it does not answer.
    LocatorSubjectMismatch {
        digest: String,
        member: &'static str,
        declared: String,
        cited: String,
    },
    /// The basis does not carry something §17's minimum decision basis requires
    /// for the profile the CALLER asked for.
    ///
    /// A defect in the BASIS, and the one refusal here that is about evidence
    /// that was never offered rather than evidence that failed a check. Every
    /// other `Unresolved` answers "this did not hold"; this one answers
    /// "nothing was presented, and an empty answer is not a passed check".
    ///
    /// `missing` is §17's own wording for the requirement, so a reader can find
    /// the row in the contract rather than in this crate.
    BasisIncompleteForProfile {
        profile: &'static str,
        missing: &'static str,
    },
    /// Two of §17's requirements for one observation were each satisfied, and by
    /// DIFFERENT artifacts of the same surface.
    ///
    /// A defect in the BASIS and not in any artifact in it: every one resolved,
    /// validated, and answered the question put to it. §17 states a minimum
    /// decision basis *per observation*, and an observation of a review is an
    /// observation of one review — so a review supplying the observed commit
    /// identity and a second review supplying the derivation are not that
    /// observation's basis, they are two halves of two.
    ///
    /// `surface` rather than the requirement names: the surface is what the two
    /// requirements have in common and is the thing a reader has to look at.
    BasisSubjectNotShared {
        profile: &'static str,
        surface: &'static str,
    },
    /// A derived fact fills a derivation's source slot with an artifact of a
    /// kind that slot does not take.
    ///
    /// A defect in the CLAIM, like [`Unresolved::DerivationArityMismatch`] and
    /// for the same reason: the citation is malformed, so nothing was lost and
    /// nothing disagreed. Reported before the rule is allowed to read anything,
    /// because a rule that read the wrong surface has recomputed a different
    /// fact and reported it under this one's name.
    ///
    /// `expected` and `found` are contract kind names — §8 `sourceKind` for a
    /// complete projection, §7.3 `locatorKind` for a reduced record. They are
    /// one vocabulary, which `tests/correction_g1.rs` holds rather than assumes.
    DerivationSlotKindMismatch {
        derivation: String,
        version: String,
        slot: usize,
        expected: &'static str,
        found: String,
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
    /// A registered derivation is not the code bound to its `(id, version)`.
    ///
    /// §18's identity rule, refused at DECISION time rather than only in a test.
    /// Distinct from every other derivation refusal because nothing about the
    /// evidence is wrong: the sources resolved, the inputs are available, and
    /// the rule was never allowed to run — because nothing established that it
    /// is the rule the fact names. A behaviour change needs a new version, never
    /// a new digest on this one.
    DerivationImplementationDrift {
        derivation: String,
        version: String,
        expected: String,
        computed: String,
    },
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
                ArtifactKind::CompleteProjection(_) | ArtifactKind::ReducedSourceRecord(_)
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
    expected: Expected<'_>,
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
    if artifact.kind().gate() == Gate::Gated && !authorised(store, &artifact, expected, into) {
        return None;
    }

    Some(artifact)
}

/// §9.2's authority chain for a gated artifact: a binding about THIS record,
/// naming an assessment that is itself retained, conforming, and authorising.
fn authorised<E: RetainedEvidence>(
    store: &E,
    record: &ValidatedArtifact,
    expected: Expected<'_>,
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
        expected,
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
        expected,
        into,
    ) else {
        return false;
    };

    if let Err(reason) =
        redaction::check_authorises(record, &assessment, expected.policy, expected.detector)
    {
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
    for asserted in expected
        .declared
        .iter()
        .filter(|b| b.record_digest == requested)
    {
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
    /// §13's `requiredObservationId` — the observation this snapshot is a query
    /// FOR.
    ///
    /// Carried here rather than compared inside `qualify_query`, because
    /// qualification cannot know it: the observation a decision is about is the
    /// DECISION'S, and the artifact does not get to supply the identity it is
    /// checked against. That is the same direction the expected query digest
    /// travels, one relation further in.
    observation: String,
}

/// §13's `binding` against the subject the DECISION is about.
///
/// A free function over the validated snapshot rather than a method on
/// [`QualifiedQuery`], because both consumers ask it before qualification and at
/// the same point in their own relation checks — and because it is the one
/// question qualification cannot answer for itself. §17.1: the subject must
/// arrive from outside the artifacts being checked.
///
/// ONE FUNCTION FOR BOTH ROLES. A falsification scan declares the query it
/// claims to have enumerated; an absence claim names the change it is a claim
/// about. Those are the same relation, and two copies of it are two chances to
/// disagree about one artifact.
fn concerns(snapshot: &ValidatedArtifact, subject: &QueryBinding) -> Result<(), String> {
    let repository = snapshot
        .pointer("/binding/repository")
        .and_then(Value::as_str);
    let pull_request = snapshot
        .pointer("/binding/pullRequest")
        .and_then(Value::as_str);
    if repository == Some(subject.repository.as_str())
        && pull_request == Some(subject.pull_request.as_str())
    {
        return Ok(());
    }
    Err(format!(
        "the decision is about {}#{} and this query snapshot enumerates {}#{}; a real, \
         complete enumeration of another change is not evidence about this one",
        subject.repository,
        subject.pull_request,
        repository.unwrap_or("<absent>"),
        pull_request.unwrap_or("<absent>")
    ))
}

impl QualifiedQuery {
    /// §13: `NotProduced` is legal only when the named matcher, applied to the
    /// retained candidate set, yields an EMPTY matched subsequence.
    ///
    /// A qualified query whose matched set is non-empty is perfectly good
    /// evidence — of the opposite claim. The role is where that distinction
    /// lives, which is why it is not folded into qualification.
    fn supports_absence(&self, expected_observation: &str) -> Result<(), String> {
        // THE SUBJECT RELATION, and it is checked before the state. A snapshot of
        // another observation with an empty matched set is perfectly good
        // evidence — about a question nobody asked. §17.1 lists the subject
        // alongside role and state precisely so that "the artifact is complete
        // and its claim reproduces" cannot stand in for "it is about this".
        //
        // Exact equality, not a prefix or a surface match: `review/external` and
        // `review/external-2` are different observations, and a reader who has
        // to know which comparison was meant has been handed a procedure.
        if self.observation != expected_observation {
            return Err(format!(
                "the decision is about observation {expected_observation:?} and its query \
                 snapshot {} is a query for {:?}; a complete enumeration of another \
                 observation establishes nothing about this one",
                self.digest, self.observation
            ));
        }
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
    expected: Expected<'_>,
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

    // 1b. THE OBSERVATION THE STATE SUMMARISES. §14:
    //
    //     If a next page exists, pagination terminated early, a page fetch
    //     failed, or the pagination state is unknown, the acquisition layer
    //     MAY NOT claim authoritative absence. That is: CANNOT_CHECK.
    //
    //     COMPLETE + nextPagePresent: true  ->  refused
    //
    // `enumeration` is what the producer CONCLUDED; `nextPagePresent` is the
    // observation it concluded from. §13 requires both to be retained precisely
    // so the second can check the first, and reading only the first is
    // self-certification with the contradiction sitting one member away in the
    // same object. Nothing in this crate read the field before: it existed only
    // in the matcher's schema table, where it is typed and never consulted.
    //
    // THE OTHER TWO CONDITIONS §14 NAMES ARE NOT RESTATED HERE, and the
    // omissions are deliberate. "A page fetch failed" is an INCOMPLETE
    // enumeration and the branch above refuses it. "Pagination terminated
    // early" is NOT `pagesObtained` shorter than `pagesRequested` — a query
    // whose data runs out asks for more pages than it gets, and reports that by
    // setting this flag false — so a rule there would be an implementation
    // inventing a norm the contract does not state.
    //
    // The non-boolean and absent arms fold into one refusal because they are
    // one §14 condition: the pagination state is unknown. Both are already
    // refused at the artifact door, where §13's REQUIRED member and its boolean
    // type are checked, so neither arm is reachable from a validated snapshot
    // today. They are written rather than assumed because this is the function
    // that decides authoritative absence, and a decision that depends on where
    // some other check happens to live is a decision resting on an accident.
    match snapshot.pointer("/pagination/nextPagePresent") {
        Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => {
            return Err("the query snapshot records enumeration COMPLETE and \
                 /pagination/nextPagePresent true; §14 forbids an authoritative absence when a \
                 next page exists, and an enumeration that says both is contradicted by its \
                 own evidence rather than supported by it"
                .to_owned())
        }
        other => {
            return Err(format!(
                "the query snapshot's /pagination/nextPagePresent is {}, so its pagination \
                 state is unknown; §14 puts an unknown state where it puts a next page that \
                 exists, and a completeness witness nobody can read does not license the \
                 result it exists to license",
                other.map_or_else(|| "absent".to_owned(), ToString::to_string)
            ))
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
            resolve_artifact(store, cited, ConsumedAs::QueryCandidate, expected, &mut why)
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

    // THE IMPLEMENTATION AXIS, and reading it is the whole point.
    //
    // `verify_matched` answers two questions and this used to consume one.
    // `ImplementationCheck::CannotCheck` is a *silent* outcome — replay returns
    // it happily — so a schemaVersion-1 snapshot, which §13.1 gives no
    // `matcher.implementationDigest`, produced a qualified query while nothing
    // established that the code which just ran is the code the artifact named.
    // That is the reading `ImplementationCheck`'s own must_use message forbids:
    // an unread implementation check is an unchecked axis reported as a passed
    // one.
    //
    // VERSION 1 IS STILL A CONFORMING ARTIFACT. §13 registers it and the door
    // admits it. What it cannot do is fill a REPLAY-DEPENDENT ROLE, which is the
    // contract's own distinction and not a schema change made here.
    //
    // Found by external review of GREEN-B4R.2, and the way it survived is worth
    // recording: RED-B4R.1 hardened the FIXTURES to version 2 so they reach
    // `Bound`, and the guard it added asserts that the FIXTURES are bound.
    // Nothing asserted that this function requires it. A guard that checks the
    // opposite side of the boundary it names removes the only route by which a
    // test could have caught the gap.
    if !matches!(
        verdict.replay.implementation,
        ImplementationCheck::Bound { .. }
    ) {
        return Err(format!(
            "replay of query snapshot {} reached {:?} on the implementation axis; §13.1 \
             binds a matcher version to the bytes that implement it, and a selection \
             replayed by code nobody established to be that implementation is a \
             recomputation of an unknown rule",
            snapshot.digest(),
            verdict.replay.implementation
        ));
    }

    if !verdict.reproduced {
        return Err(format!(
            "the query snapshot claims {:?} matched and its own candidate set produces {:?}",
            recorded.recorded_matcher().matched_snapshot_digests(),
            verdict.replay.matched
        ));
    }

    let observation = snapshot
        .str_at("/requiredObservationId")
        .ok_or_else(|| {
            "the query snapshot carries no /requiredObservationId, which §13 makes REQUIRED"
                .to_owned()
        })?
        .to_owned();

    Ok(QualifiedQuery {
        digest: snapshot.digest().to_owned(),
        matched: verdict.replay.matched,
        observation,
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
    if !matches!(record.kind(), ArtifactKind::ReducedSourceRecord(_)) {
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
    expected: Expected<'_>,
    into: &mut Vec<Unresolved>,
) {
    let Some(entry) = derivations::resolve(&fact.derivation, &fact.version) else {
        into.push(Unresolved::UnknownDerivation {
            derivation: fact.derivation.clone(),
            version: fact.version.clone(),
        });
        return;
    };

    // THE IMPLEMENTATION BINDING, BEFORE THE RULE RUNS.
    //
    // §18 gives a registered derivation its identity in the bytes of the file
    // that implements it, and `derivations::verify_implementation` is the check.
    // It had exactly one caller in this crate and it was
    // `tests/derivation_binding.rs` — so CI caught drift in this repository and
    // the DECISION PATH did not. A derived fact could be recomputed by code
    // whose binding does not hold, under an unchanged `(id, version)`.
    //
    // The matcher does the opposite and always did: `recompute_matched` calls
    // `verify_binding` before scoring a single candidate. The asymmetry is the
    // whole finding — one replayable rule verified its implementation at
    // decision time and the other verified it in a test.
    //
    // Refused as its own fact. `DerivationDisagrees` would say the sources do
    // not imply the claim, and `DerivationCannotRecompute` would say the rule
    // ran and produced nothing. Neither happened: the rule was never allowed to
    // run, because nothing established it is the rule.
    if let Err(drift) = derivations::verify_implementation(entry) {
        into.push(Unresolved::DerivationImplementationDrift {
            derivation: fact.derivation.clone(),
            version: fact.version.clone(),
            expected: drift.expected.to_owned(),
            computed: drift.computed,
        });
        return;
    }

    if fact.derived_from.len() != entry.arity() {
        into.push(Unresolved::DerivationArityMismatch {
            derivation: fact.derivation.clone(),
            expected: entry.arity(),
            cited: fact.derived_from.len(),
        });
        return;
    }

    let mut sources = Vec::with_capacity(entry.arity());
    for (slot, (source, cited)) in entry.sources.iter().zip(&fact.derived_from).enumerate() {
        let source_digest = &cited.digest;
        // Every cited source goes through the same resolution the decision's own
        // inputs do. A derived fact resting on bytes nobody was permitted to keep
        // is not better evidenced than a decision resting on them.
        let Some(record) = resolve_artifact(
            store,
            source_digest,
            ConsumedAs::GatedSource,
            expected,
            into,
        ) else {
            return;
        };

        // THE SLOT'S SURFACE, BEFORE THE RULE READS A FIELD OUT OF IT.
        //
        // `ConsumedAs::GatedSource` above answers a different question, and the
        // difference is the artifact/relation split this crate is built on: the
        // door asks whether this KIND of object is the sort a decision may read
        // a gated pointer out of, which is true of every complete projection and
        // every reduced record. Whether it is the SURFACE THIS SLOT TAKES is a
        // relation between one derivation and one citation, and relation checks
        // belong at the consumption site — the same place observation binding
        // and §9.5 policy-version equality landed, for the same reason.
        //
        // Reading `surface()` rather than a member: a reduced record's
        // `/sourceKind` is `github-reduced-source-record` for all five surfaces,
        // so a check written against `sourceKind` alone would refuse every
        // reduced record in every slot and read as a repair. Redaction §8
        // requires the opposite — a fact whose inputs survived the gate stays
        // usable — and `tests/correction_g1.rs` G1-E is where that is enforced.
        //
        // Before the view is built, not after. A rule that read the wrong
        // surface has recomputed a DIFFERENT FACT and reported it under this
        // one's name; and the refusal a wrong-surface citation used to produce
        // was `PointerBlocked`, which says the redaction gate removed something.
        // It had not. Nothing was blocked, and an operator following that
        // refusal audits a policy that is working correctly.
        let found = record.kind().surface();
        if found != source.kind {
            into.push(Unresolved::DerivationSlotKindMismatch {
                derivation: fact.derivation.clone(),
                version: fact.version.clone(),
                slot,
                expected: source.kind,
                found: found.to_owned(),
            });
            return;
        }

        match derivation_source_view(&record, source.inputs) {
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
            ArtifactKind::ReducedSourceRecord(_) => input.decoded,
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
pub fn admissibility<E: RetainedEvidence>(
    profile: DecisionProfile,
    basis: &DecisionBasis,
    store: &E,
) -> Admissible {
    let checked = check_basis(basis, store);
    let mut why = checked.why;

    // §17'S MINIMUM DECISION BASIS, and it is a question about the BASIS rather
    // than about any artifact in it.
    //
    // Every refusal `check_basis` produces answers "this did not hold". This one
    // answers "nothing was presented to be held to", which is the failure the
    // whole crate exists to refuse and which this function performed on itself:
    // an empty basis produced an empty `why` and came back `Yes`. Each input
    // never named resolved vacuously; each derived fact never claimed recomputed
    // vacuously.
    //
    // WHY IT IS SKIPPED WHEN AN INPUT DID NOT RESOLVE. A requirement is
    // satisfied by an input that OBSERVED the field, and an input whose bytes
    // the store could not produce observed nothing — so its surface is unknown
    // and there is no way to tell which requirement it was meant to satisfy.
    // Reporting incompleteness there would misdescribe a retention failure as an
    // adapter one, the misdiagnosis class G1-F exists to refuse: the basis DID
    // name the input, and the remedy is to retain the bytes, not to fix the
    // adapter.
    //
    // This cannot become an escape. The pass only ever ADDS refusals, and it is
    // skipped only when `why` is already non-empty for the unresolved input — so
    // the verdict is `CannotCheck` either way. Skipping it can change which
    // reasons are listed; it can never admit a decision.
    if !checked.some_input_did_not_resolve {
        for requirement in profile.requires() {
            let satisfied = match requirement.needs {
                Needs::Observed {
                    surface,
                    canonical,
                    decoded,
                } => checked.observed.iter().any(|o| {
                    o.surface == surface && (o.pointer == canonical || o.pointer == decoded)
                }),
                Needs::Derived { derivation } => {
                    basis.derived.iter().any(|f| f.derivation == derivation)
                }
                Needs::ExpectedQuerySnapshot => basis.expected_query.is_some(),
            };
            if !satisfied {
                why.push(Unresolved::BasisIncompleteForProfile {
                    profile: profile.name(),
                    missing: requirement.name,
                });
            }
        }
        let mut by_surface: BTreeMap<&'static str, Vec<Vec<&str>>> = BTreeMap::new();
        for requirement in profile.requires() {
            match requirement.needs {
                Needs::Observed {
                    surface,
                    canonical,
                    decoded,
                } => {
                    by_surface.entry(surface).or_default().push(
                        checked
                            .observed
                            .iter()
                            .filter(|o| {
                                o.surface == surface
                                    && (o.pointer == canonical || o.pointer == decoded)
                            })
                            .map(|o| o.digest)
                            .collect(),
                    );
                }
                Needs::Derived { derivation } => {
                    // One set per SLOT KIND, unioned across the facts claiming
                    // this derivation — the same "exists" reading as above. The
                    // version comes from each fact rather than the requirement
                    // because the requirement names the id only, deliberately:
                    // whether the named version exists and recomputes is
                    // `check_derived`'s question, answered in one place.
                    let mut slots: BTreeMap<&'static str, Vec<&str>> = BTreeMap::new();
                    for fact in basis.derived.iter().filter(|f| f.derivation == derivation) {
                        let Some(entry) = derivations::resolve(&fact.derivation, &fact.version)
                        else {
                            continue;
                        };
                        for (index, slot) in entry.sources.iter().enumerate() {
                            if let Some(cited) = fact.derived_from.get(index) {
                                slots
                                    .entry(slot.kind)
                                    .or_default()
                                    .push(cited.digest.as_str());
                            }
                        }
                    }
                    for (kind, cited) in slots {
                        by_surface.entry(kind).or_default().push(cited);
                    }
                }
                // Names no surface: §17's absence row is about the basis naming
                // a snapshot at all, and the snapshot's own subject is checked
                // where the snapshot is qualified.
                Needs::ExpectedQuerySnapshot => {}
            }
        }
        for (surface, witnesses) in by_surface {
            // An empty set means that row was not satisfied, or the fact
            // claiming it was malformed. Both are already reported, and adding
            // "these are not one observation" for a row nobody presented would
            // answer a question that was never asked.
            let Some((first, rest)) = witnesses.split_first() else {
                continue;
            };
            if rest.is_empty() || witnesses.iter().any(Vec::is_empty) {
                continue;
            }
            let shared = first
                .iter()
                .any(|candidate| rest.iter().all(|set| set.contains(candidate)));
            if !shared {
                why.push(Unresolved::BasisSubjectNotShared {
                    profile: profile.name(),
                    surface,
                });
            }
        }
    }

    if why.is_empty() {
        Admissible::Yes {
            values: checked.values,
        }
    } else {
        Admissible::CannotCheck { why }
    }
}

/// The values a basis's named inputs resolve to, or every artifact- and
/// relation-validity refusal it produces.
///
/// **NOT A VERDICT, and the return type says so.** A `Result` carrying the
/// values read or the refusals found, never an [`Admissible`]. `Ok` means
/// "nothing this basis NAMED failed", which is a far weaker statement than
/// "this decision is evidenced": §17's minimum decision basis is
/// [`admissibility`]'s question and nothing else asks it. A consumer deciding
/// an observation calls [`admissibility`].
///
/// Public because a witness about ONE relation should not have to assemble a
/// complete §17 basis to ask about that relation — every fixture would then
/// carry machinery unrelated to its claim, and a failure in that machinery would
/// read as a failure of the relation under test.
pub fn relations_checked<E: RetainedEvidence>(
    basis: &DecisionBasis,
    store: &E,
) -> Result<Vec<Value>, Vec<Unresolved>> {
    let checked = check_basis(basis, store);
    if checked.why.is_empty() {
        Ok(checked.values)
    } else {
        Err(checked.why)
    }
}

/// What one pass over a basis found, before §17 completeness is considered.
struct CheckedBasis<'a> {
    why: Vec<Unresolved>,
    values: Vec<Value>,
    /// What each input turned out to OBSERVE — the surface it was read from and
    /// the pointer it was read by. Recorded during the walk because a surface is
    /// only knowable after resolution, and resolving twice is two chances to
    /// disagree about one artifact.
    observed: Vec<ObservedField<'a>>,
    some_input_did_not_resolve: bool,
}

/// One field a basis actually observed, and the artifact it observed it on.
///
/// The DIGEST is here because §17's table is a minimum basis per observation:
/// two rows about a review are two rows about ONE review, and a surface plus a
/// pointer cannot say which artifact answered. Carrying the surface alone is
/// what let two reviews jointly evidence one decision.
#[derive(Debug, Clone, Copy)]
struct ObservedField<'a> {
    surface: &'static str,
    pointer: &'a str,
    digest: &'a str,
}

fn check_basis<'a, E: RetainedEvidence>(basis: &'a DecisionBasis, store: &E) -> CheckedBasis<'a> {
    let expected = Expected {
        declared: &basis.bindings,
        detector: &basis.expected_detector,
        policy: &basis.expected_redaction_policy,
    };
    let mut why = Vec::new();
    let mut values = Vec::new();
    let mut observed: Vec<ObservedField<'a>> = Vec::new();
    let mut some_input_did_not_resolve = false;

    for input in &basis.inputs {
        let Some(record) = resolve_artifact(
            store,
            &input.source_digest,
            ConsumedAs::GatedSource,
            expected,
            &mut why,
        ) else {
            some_input_did_not_resolve = true;
            continue;
        };
        observed.push(ObservedField {
            surface: record.kind().surface(),
            pointer: input.pointer.as_str(),
            digest: input.source_digest.as_str(),
        });
        match read_pointer(&record, &input.pointer) {
            Ok(value) => values.push(value),
            Err(e) => why.push(e),
        }
    }

    for fact in &basis.derived {
        check_derived(store, fact, expected, &mut why);
    }

    // The expected query digest is named by the basis and resolved like any
    // other record. It is NOT read out of the store: that direction is the
    // authority path this slice exists to establish, and reversing it would make
    // the store the author of its own expectation.
    if let Some(expected_query) = &basis.expected_query {
        let query_digest = &expected_query.digest;
        if let Some(snapshot) = resolve_artifact(
            store,
            query_digest,
            ConsumedAs::ExpectedQuerySnapshot,
            expected,
            &mut why,
        ) {
            // AND THEN THE ROLE. Structural validity was never the question here:
            // §13 makes an INCOMPLETE snapshot a well-formed record that simply
            // cannot evidence an absence, and a snapshot whose own candidates
            // contradict its claim is well-formed too. This is the same
            // qualification the scan path consumes, so the two cannot come to
            // disagree about one artifact.
            // THE SUBJECT FIRST, and from outside. §13 makes the snapshot's
            // `binding` part of what the artifact is; the change this decision
            // is about is the CALLER'S, exactly as `staleness` takes its subject
            // and a scan declares the query it enumerated. An observation id is
            // a role name — `review/external` — reused across every pull request
            // and repository, so comparing it alone lets a complete, empty
            // enumeration of another change carry a negative here. An absence
            // claim is the one decision whose whole content is what was
            // searched.
            let qualified = concerns(&snapshot, &expected_query.subject)
                .and_then(|()| qualify_query(&snapshot, store, expected))
                .and_then(|q| q.supports_absence(&basis.observation_id).map(|()| q));
            if let Err(reason) = qualified {
                why.push(Unresolved::QueryDoesNotSupportRole {
                    digest: query_digest.clone(),
                    role: ConsumedAs::ExpectedQuerySnapshot.name(),
                    why: reason,
                });
            }
        }
    }

    CheckedBasis {
        why,
        values,
        observed,
        some_input_did_not_resolve,
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
    /// The redaction policy version this scan is evaluated under, per §9.5.
    ///
    /// The snapshot itself is ungated, but §13's candidate set is not: every
    /// candidate is a gated source authorised through an assessment, so the
    /// expectation has to reach this entry point too.
    pub expected_redaction_policy: String,
    /// The detector identity this evaluation accepts, per §9.4. See
    /// [`ExpectedDetector`].
    pub expected_detector: ExpectedDetector,
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
    // The scan declares no §9.2 bindings — it never has — but it does declare
    // the policy the surface was examined under, and §13's candidate set is
    // gated evidence authorised through assessments like any other.
    let expected = Expected {
        declared: &[],
        detector: &scan.expected_detector,
        policy: &scan.expected_redaction_policy,
    };
    let mut why = Vec::new();
    let Some(snapshot) = resolve_artifact(
        store,
        &scan.snapshot_digest,
        ConsumedAs::ScanEvidence,
        expected,
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
    // The same relation the absence path checks, through the same function: a
    // scan declares the query it enumerated, an absence claim names the change
    // it is about, and those are one question asked by two roles.
    concerns(&snapshot, &scan.binding)?;

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
    qualify_query(&snapshot, store, expected)
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
    /// §8.1: a failed head read records a reason CODE and carries no
    /// `snapshotDigest` — there are no bytes to point at.
    Failed { code: FailedRead },
}

/// §8.1's closed vocabulary for a head read that did not happen.
///
/// A CLOSED SET AND NOT A STRING, for redaction §9.4's reason applied one
/// contract over: "a closed field cannot carry a secret out because its range
/// does not depend on the content inspected". This member was a `String`, and
/// `staleness` interpolated it verbatim into its verdict — so an acquisition
/// layer holding an HTTP response and an authorization header could hand a
/// credential to whatever logs that verdict, with no artifact involved at all.
///
/// The four are separated because they are four different facts about the
/// subject. `NotFound` says the subject may not exist; `Unauthorized` says
/// nothing about the subject at all; reading the second as the first is an
/// absent signal reported as a negative result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailedRead {
    /// §15's API-side limit, which §8.1 names explicitly for this event.
    RateLimited,
    RequestFailed,
    NotFound,
    Unauthorized,
}

impl FailedRead {
    /// §8.1's own spelling, and the value a retained event carries.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::RateLimited => "RATE_LIMITED",
            Self::RequestFailed => "REQUEST_FAILED",
            Self::NotFound => "NOT_FOUND",
            Self::Unauthorized => "UNAUTHORIZED",
        }
    }
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
    /// The redaction policy version this evaluation is made under, per §9.5.
    ///
    /// §5.3 puts `github-pull-request-head` in the gated set, so both head
    /// snapshots are authorised through an assessment like any other gated
    /// record — and this entry point takes no `DecisionBasis`, which is why the
    /// expectation has to travel with the subject. `correction_k1.rs`'s K1-C is
    /// the witness that a repair reaching only the basis would leave this route
    /// open.
    pub expected_redaction_policy: String,
    /// The detector identity this evaluation accepts, per §9.4. See
    /// [`ExpectedDetector`].
    pub expected_detector: ExpectedDetector,
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
            HeadRead::Failed { code } => {
                unresolved.push(format!("{slot} did not happen ({})", code.code()));
            }
            HeadRead::Observed { event_digest } => {
                match resolve_head_read(store, event_digest, expected_role, subject) {
                    Ok(read) => observed.push(read),
                    Err(why) => unresolved.push(format!("{slot} {why}")),
                }
            }
        }
    }

    // §8.1'S ORDER, AND IT IS THE CONTRACT'S RATHER THAN THIS FUNCTION'S.
    //
    //     any required read unresolved      ->  CannotCheck
    //     else any resolved SHA != expected ->  Stale
    //     else                              ->  NotStale
    //
    // WHAT USED TO BE HERE, recorded because deleting it silently would repeat
    // the mistake. The disagreement scan ran FIRST, under the comment "a
    // resolved disagreement is decisive even if the other end did not resolve",
    // so a failed HEAD_AFTER beside a disagreeing HEAD_BEFORE returned `Stale`.
    // §8.1 says that case is CANNOT_CHECK, and says it in as many words:
    //
    //     HEAD_AFTER failed  ->  CANNOT_CHECK
    //                        ->  never a silent absence of STALE
    //
    // The comment is what makes it worth this many lines. It was not an
    // oversight — somebody found the contract's answer unsatisfying and wrote a
    // better-sounding rule into the code without amending the document, which is
    // invisible to any review looking for MISSING checks. The check was there,
    // commented and confident, implementing a law nobody agreed to.
    //
    // WHY THE CONTRACT IS RIGHT, since the old argument is superficially good. A
    // HEAD_BEFORE disagreeing with `expected_sha` does not establish that the
    // head moved DURING the evaluation. It establishes that the caller's
    // expectation and the pre-read disagree, which has at least three causes:
    // the head moved before the bracket opened, the expectation was stale
    // already, or the subject was never at that SHA. `Stale` names exactly one
    // of them, and the bracket is what distinguishes it. With one end missing
    // there is no bracket, and returning the one verdict the evidence cannot
    // distinguish is the confident answer this crate exists to refuse.
    //
    // `unresolved` is the test, not the `HeadRead::Failed` variant: a read that
    // declared itself successful and named an event nobody retained also
    // produced no SHA, and the question is whether a SHA was produced.
    if !unresolved.is_empty() {
        return Staleness::CannotCheck {
            why: format!(
                "{}; nothing witnesses that the head did not move, and an unread head is not \
                 an unchanged one",
                unresolved.join("; ")
            ),
        };
    }

    // §8.1'S BRACKET, AND WHETHER THIS PAIR IS ONE.
    //
    //     §8.1's whole purpose is to bracket an evaluation between two reads,
    //     and `observedAt` is what says which read came first.
    //
    // A `HEAD_AFTER` observed before its `HEAD_BEFORE` encloses no interval, so
    // the evaluation it claims to bracket happened outside it and the head is
    // unwitnessed for the whole of the span that matters.
    //
    // BEFORE THE DISAGREEMENT SCAN, and that placement is G4's argument rather
    // than a new one. A read disagreeing with `expected_sha` does not establish
    // that the head moved DURING the evaluation; the bracket is what separates
    // that from an expectation which was already stale. A reversed pair is a
    // missing bracket by a second route, so `Stale` is exactly as unsupported
    // here as it is beside a read that never happened.
    //
    // THE EQUAL CASE IS NOT REFUSED, and the omission is a ruling rather than an
    // oversight. §8.1 froze `observedAt` to whole seconds. Two genuine reads
    // around a fast evaluation therefore share a second, and two equal
    // timestamps equally cannot say which read came first. Both are true at
    // once, which makes it a question about whether §8.1's witness is adequate
    // at the precision it froze — not a rule an implementation may pick.
    // Refusing on `==` would refuse conformant evidence on the strength of an
    // implementation's preference; it is recorded as an open §8.1 question in
    // the contract and pinned by J2A-E, and stays out of the code until §8.1
    // settles it.
    //
    // The comparison is lexicographic and that is sound ONLY over §8.1's frozen
    // form; see `ObservedHead::observed_at`.
    if let [before, after] = observed.as_slice() {
        if before.observed_at > after.observed_at {
            return Staleness::CannotCheck {
                why: format!(
                    "the HEAD_BEFORE read was observed at {} and the HEAD_AFTER read at {}, so \
                     the pair encloses no interval; §8.1 exists to bracket an evaluation \
                     between two reads and these two leave it outside them",
                    before.observed_at, after.observed_at
                ),
            };
        }
    }

    for read in &observed {
        if read.sha != expected_sha {
            return Staleness::Stale {
                observed: read.sha.clone(),
            };
        }
    }

    Staleness::NotStale
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
fn resolve_head_read<E: RetainedEvidence>(
    store: &E,
    event_digest: &str,
    expected_role: &str,
    subject: &Subject,
) -> Result<ObservedHead, String> {
    // The subject carries the policy expectation because this entry point takes
    // no `DecisionBasis` — and both head snapshots are gated, so both are
    // authorised through an assessment. K1-C is the witness for that placement.
    let expected = Expected {
        declared: &[],
        detector: &subject.expected_detector,
        policy: &subject.expected_redaction_policy,
    };
    let mut why = Vec::new();
    let Some(event) = resolve_artifact(
        store,
        event_digest,
        ConsumedAs::HeadReadEvent,
        expected,
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
        expected,
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

    // §8.1 REQUIRES `observedAt` on every event, of either acquisition, and
    // freezes its value to exactly `YYYY-MM-DDThh:mm:ssZ`. It is read out HERE,
    // beside the SHA, because the two together are what one read contributes to
    // the bracket: this function used to return the SHA alone, and a caller
    // holding only SHAs cannot ask which read came first no matter how careful
    // it is. The absent arm is the same door-guaranteed shape as `headSha`
    // directly below it, and is written for the same reason.
    let observed_at = event
        .str_at("/observedAt")
        .ok_or_else(|| "names an event with no observedAt, which §8.1 requires".to_owned())?
        .to_owned();

    let sha = snapshot
        .str_at("/headSha")
        .map(str::to_owned)
        .ok_or_else(|| "names a head snapshot with no headSha".to_owned())?;

    Ok(ObservedHead { sha, observed_at })
}

/// What one successful head read contributes to §8.1's bracket.
///
/// The instant travels with the SHA rather than being fetched separately,
/// because they are one read's two halves and a pair of them is the whole of
/// the evidence `staleness` has. Splitting them was how the bracket came to be
/// checked for existing and never for enclosing anything.
#[derive(Debug, Clone)]
struct ObservedHead {
    sha: String,
    /// §8.1's `observedAt`, in the frozen `YYYY-MM-DDThh:mm:ssZ` form.
    ///
    /// COMPARED AS A STRING, AND THAT IS ONLY SOUND OVER THAT DOMAIN. The form
    /// is fixed width, zero-padded in every field and always UTC, so byte order
    /// and time order coincide. They stop coinciding the moment the domain
    /// admits an offset or a fractional second, and nothing about a string
    /// comparison would announce it — `tests/correction_j2a.rs`'s J2A-G asserts
    /// the frozen sentence from the contract for exactly that reason.
    observed_at: String,
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
