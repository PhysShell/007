//! Pure closure classifier (issue #147, the step after the frozen Step 0B
//! fixtures and their integrity validator).
//!
//! ```text
//! explicit observations -> classifier -> closure predicate
//! ```
//!
//! WHAT THIS CRATE IS NOT. It performs no acquisition. It opens no socket, calls
//! no GitHub API, runs no `git`, reads no `.git/`, consults no branch or working
//! tree, and produces no attestation. A later acquisition adapter turns API
//! responses into a [`ClassifierInput`]; a later envelope step wraps the output.
//! Both are outside this crate by design — the classifier must be callable
//! entirely in memory.
//!
//! WHAT THE PREDICATE CLAIMS. Only this: every observation the policy explicitly
//! required has the recorded state, for this immutable subject, under the
//! declared assumptions. `PASS` does NOT mean the observation set is complete,
//! the pull request is correct, review was exhaustive, no unknown defect exists,
//! or that anything may be merged. The policy carries
//! `completeness_claimed = false` and the required set is enumerated in the
//! output so a later reader can see exactly what was asked.
//!
//! ACQUISITION STATUS IS AN INPUT, NOT AN INFERENCE. This crate never decides
//! that an observation is missing by finding an absent key, an empty array, a
//! null or a parse error. The caller states what happened via [`Acquisition`],
//! which is what keeps the two failure modes apart:
//!
//! ```text
//! OWED          nobody produced the required evidence
//! CANNOT_CHECK  an attempt to obtain it did not yield a trustworthy answer
//! ```
//!
//! NO PROSE ORACLE. Nothing here reads English. A defect claim reaches the
//! classifier as an already-established [`FalsificationFact`] carrying
//! provenance; extracting such facts from vendor responses is the acquisition
//! layer's job. This crate decides only what an established fact does to the
//! state vector.

// Mirror the workspace discipline.
#![forbid(unsafe_code)]

use serde::Serialize;
use std::collections::BTreeMap;

/// The five states of #147, kept distinct. Neighbours are never merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    /// Admissible positive evidence exists for the frozen subject.
    Pass,
    /// An admissible established defect exists for the frozen subject.
    Finding,
    /// Required evidence does not yet exist; nobody has produced it.
    Owed,
    /// An attempt to obtain a required observation did not yield a trustworthy
    /// answer. This is NOT `Owed`: somebody tried and the attempt failed.
    CannotCheck,
    /// The pull request head differs from the expected subject, before or after
    /// evaluation.
    Stale,
}

impl State {
    /// Headline precedence, presentation only:
    /// `STALE > FINDING > CANNOT_CHECK > OWED > PASS`.
    fn rank(self) -> u8 {
        match self {
            State::Pass => 0,
            State::Owed => 1,
            State::CannotCheck => 2,
            State::Finding => 3,
            State::Stale => 4,
        }
    }
}

/// What happened when the caller tried to obtain one observation. The classifier
/// never infers any of these from the shape of missing data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquisition<T> {
    /// The observation was obtained.
    Available(T),
    /// The acquisition succeeded and established that nobody has produced the
    /// required evidence — an authoritative absence, not a failure.
    NotProduced,
    /// The producer was rate-limited, so no verdict is available yet. Still
    /// nobody's verdict; never a pass and never a failure.
    RateLimited,
    /// The attempt itself failed: transport, tool, permission, malformed
    /// response. A failed query that yielded no values is not an empty result.
    Failed { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckConclusion {
    Success,
    Failure,
}

/// A CI check as observed, with the commit it is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckEvidence {
    pub stable_id: String,
    pub head_sha: String,
    pub conclusion: CheckConclusion,
}

/// A submitted review object as observed. `commit_id` is its immutable binding;
/// `carries_finding` is supplied by the acquisition layer, never inferred here
/// from body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewEvidence {
    pub stable_id: String,
    pub commit_id: String,
    pub author: String,
    pub carries_finding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationInput {
    Check(Acquisition<CheckEvidence>),
    Review(Acquisition<ReviewEvidence>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    GithubActionsCheck,
    SubmittedReview,
    ReviewComment,
    IssueComment,
}

/// How far a falsification-channel record has actually got. #147 defines
/// `FINDING` as an admissible VERIFIED defect, and the wide falsification
/// surface only means "a checkable counterexample may arrive from a surface that
/// could never carry a verdict" — it does not mean an unverified claim is a
/// defect. So this gates, and each value maps to exactly one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verification {
    /// Verified: the defect was reproduced. -> `FINDING`
    Reproduced,
    /// A concrete claim exists and its verification is still owed. -> `OWED`
    Claimed,
    /// Verification was attempted and did not yield a trustworthy answer.
    /// -> `CANNOT_CHECK`
    VerificationFailed,
}

/// One record on the falsification channel — NOT necessarily an established
/// defect: see [`Verification`], which decides what this record contributes.
/// Carries no prose. The classifier never reads text to decide whether some
/// wording amounts to a defect claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalsificationFact {
    pub source_kind: SourceKind,
    pub stable_id: String,
    /// Present only where the surface has an immutable commit binding. `None` is
    /// admissible — an issue comment has no commit binding, and requiring one
    /// would silently close the wide surface. But a record explicitly bound to
    /// ANOTHER commit is rejected as invalid input rather than allowed to
    /// contaminate this subject: a wide surface means more KINDS of surface, not
    /// defects of any commit in the universe.
    pub subject_sha: Option<String>,
    pub author: String,
    pub verification: Verification,
}

/// Refusals to classify. A malformed input never receives a well-formed closure
/// predicate — that is precisely how an inference would sneak in disguised as a
/// domain fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassifierError {
    /// A policy-required observation had no entry at all. We cannot know whether
    /// it was not produced, or lost by a broken adapter, or never fetched.
    /// Naming that unknown `OWED` would convert a construction failure into a
    /// domain fact, which is the whole thing this design refuses to do.
    MissingRequiredObservation { id: String },
    /// A falsification record explicitly bound to a different commit. Evidence
    /// is never combined across SHAs.
    FalsificationSubjectMismatch {
        stable_id: String,
        subject_sha: String,
        expected_sha: String,
    },
}

impl std::fmt::Display for ClassifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClassifierError::MissingRequiredObservation { id } => write!(
                f,
                "required observation `{id}` has no entry: acquisition state is carried by the \
                 caller, never inferred from absence"
            ),
            ClassifierError::FalsificationSubjectMismatch {
                stable_id,
                subject_sha,
                expected_sha,
            } => write!(
                f,
                "falsification `{stable_id}` is bound to {subject_sha}, not to the subject \
                 {expected_sha}"
            ),
        }
    }
}

impl std::error::Error for ClassifierError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub repository: String,
    pub pull_request: u64,
    /// The frozen subject every admissible positive observation must bind to.
    pub expected_sha: String,
    pub head_before: String,
    pub head_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub id: String,
    /// Authoritative enumeration. Output observation order follows it.
    pub required_observations: Vec<String>,
    // NOTE: there is deliberately no `completeness_claimed` field. V0 must
    // always disclaim completeness, and a caller-settable bool documented as
    // "always false" is a false claim waiting to be serialized. The output
    // materializes `completenessClaimed: false` unconditionally, so the
    // forbidden state cannot be constructed at all.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifierInput {
    pub subject: Subject,
    pub policy: Policy,
    pub observations: BTreeMap<String, ObservationInput>,
    pub falsifications: Vec<FalsificationFact>,
    /// Debt tracked independently of reviewer cleanliness. A clean reviewer does
    /// not discharge it.
    pub known_debts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceOut {
    pub kind: SourceKind,
    pub stable_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservationOut {
    pub id: String,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceOut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyOut {
    pub id: String,
    pub required_observations: Vec<String>,
    pub completeness_claimed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FalsificationOut {
    pub source: SourceOut,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_sha: Option<String>,
    pub author: String,
    pub verification: Verification,
    /// What this record contributes, so a headline of `FINDING`, `OWED` or
    /// `CANNOT_CHECK` driven by the falsification channel is explainable from
    /// the predicate itself rather than only from this crate's source.
    pub state: State,
}

/// The deterministic closure predicate. No timestamp, no generated id, no
/// iteration-order nondeterminism: the same semantic input serializes to the
/// same bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Predicate {
    pub schema_version: u32,
    pub repository: String,
    pub pull_request: u64,
    pub git_commit: String,
    pub policy: PolicyOut,
    pub observations: Vec<ObservationOut>,
    /// Falsification-channel records, each with its own state. NOT a list of
    /// established defects: a `CLAIMED` record is an unverified counterexample
    /// and contributes `OWED`. Kept as their own list because such a record
    /// belongs to no required observation, and attaching it to one would
    /// misreport which observation produced the state.
    pub falsifications: Vec<FalsificationOut>,
    /// True when the head moved before or after evaluation. Kept beside the
    /// per-observation vector rather than overwriting it, so a stale snapshot
    /// still shows what each observation said about the frozen subject.
    pub subject_stale: bool,
    pub headline: State,
    pub known_debts: Vec<String>,
}

impl Predicate {
    /// Deterministic serialization. Provided as a convenience; the classifier
    /// core is fully testable without touching a filesystem.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// A check is positive evidence only when it is bound to the frozen subject.
/// A check bound elsewhere is not evidence about this subject at all, so it
/// leaves the observation owed rather than combining across commits.
fn classify_check(
    acq: &Acquisition<CheckEvidence>,
    expected_sha: &str,
) -> (State, Option<SourceOut>) {
    match acq {
        Acquisition::Available(check) => {
            let source = Some(SourceOut {
                kind: SourceKind::GithubActionsCheck,
                stable_id: check.stable_id.clone(),
            });
            if check.head_sha != expected_sha {
                (State::Owed, source)
            } else {
                match check.conclusion {
                    CheckConclusion::Success => (State::Pass, source),
                    CheckConclusion::Failure => (State::Finding, source),
                }
            }
        }
        Acquisition::NotProduced | Acquisition::RateLimited => (State::Owed, None),
        Acquisition::Failed { .. } => (State::CannotCheck, None),
    }
}

/// Positive reviewer evidence is strict: the submitted review's immutable
/// commit binding must equal the frozen subject. A wrong-SHA review yields no
/// verdict here — neither a pass nor a finding.
fn classify_review(
    acq: &Acquisition<ReviewEvidence>,
    expected_sha: &str,
) -> (State, Option<SourceOut>) {
    match acq {
        Acquisition::Available(review) => {
            let source = Some(SourceOut {
                kind: SourceKind::SubmittedReview,
                stable_id: review.stable_id.clone(),
            });
            if review.commit_id != expected_sha {
                (State::Owed, source)
            } else if review.carries_finding {
                (State::Finding, source)
            } else {
                (State::Pass, source)
            }
        }
        Acquisition::NotProduced | Acquisition::RateLimited => (State::Owed, None),
        Acquisition::Failed { .. } => (State::CannotCheck, None),
    }
}

/// Classify one snapshot. Every invocation is independent: no memory of a
/// previous headline, so a finding that disappears does not linger.
pub fn classify(input: &ClassifierInput) -> Result<Predicate, ClassifierError> {
    let expected = input.subject.expected_sha.as_str();

    // Refuse malformed input before deriving anything. A predicate produced from
    // an input we could not read correctly is worse than no predicate: it looks
    // exactly like one we could.
    for id in &input.policy.required_observations {
        if !input.observations.contains_key(id) {
            return Err(ClassifierError::MissingRequiredObservation { id: id.clone() });
        }
    }
    for f in &input.falsifications {
        if let Some(sha) = &f.subject_sha {
            if sha != expected {
                return Err(ClassifierError::FalsificationSubjectMismatch {
                    stable_id: f.stable_id.clone(),
                    subject_sha: sha.clone(),
                    expected_sha: expected.to_owned(),
                });
            }
        }
    }

    // The head moving invalidates the SNAPSHOT, not the individual observations,
    // which remain statements about the frozen subject. Recorded alongside the
    // vector so nothing is overwritten.
    let subject_stale =
        input.subject.head_before != expected || input.subject.head_after != expected;

    let observations: Vec<ObservationOut> = input
        .policy
        .required_observations
        .iter()
        .map(|id| {
            // Presence was established above, so `None` here is unreachable —
            // and is still not silently turned into a state.
            let (state, source) = match input.observations.get(id) {
                None => (State::CannotCheck, None),
                Some(ObservationInput::Check(acq)) => classify_check(acq, expected),
                Some(ObservationInput::Review(acq)) => classify_review(acq, expected),
            };
            ObservationOut {
                id: id.clone(),
                state,
                source,
            }
        })
        .collect();

    let falsifications: Vec<FalsificationOut> = input
        .falsifications
        .iter()
        .map(|f| FalsificationOut {
            source: SourceOut {
                kind: f.source_kind,
                stable_id: f.stable_id.clone(),
            },
            subject_sha: f.subject_sha.clone(),
            author: f.author.clone(),
            verification: f.verification,
            state: match f.verification {
                Verification::Reproduced => State::Finding,
                Verification::Claimed => State::Owed,
                Verification::VerificationFailed => State::CannotCheck,
            },
        })
        .collect();

    // Headline is derived presentation over everything recorded: the observation
    // vector, plus falsification (which has no required-observation slot of its
    // own), plus subject staleness. The vector itself is never rewritten.
    let mut headline = observations
        .iter()
        .map(|o| o.state)
        .chain(falsifications.iter().map(|f| f.state))
        .max_by_key(|s| s.rank())
        .unwrap_or(State::Pass);
    if subject_stale {
        headline = State::Stale;
    }

    Ok(Predicate {
        schema_version: 1,
        repository: input.subject.repository.clone(),
        pull_request: input.subject.pull_request,
        git_commit: input.subject.expected_sha.clone(),
        policy: PolicyOut {
            id: input.policy.id.clone(),
            required_observations: input.policy.required_observations.clone(),
            // Unconditional: V0 never claims completeness, and the input has no
            // field through which a caller could claim it.
            completeness_claimed: false,
        },
        observations,
        falsifications,
        subject_stale,
        headline,
        known_debts: input.known_debts.clone(),
    })
}
