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

use o7_closure_canonical::digest;
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
    /// The retained assessment bytes are not the ones its digest names.
    AssessmentDigestMismatch { requested: String, computed: String },
    /// The resolved assessment is not a conforming §9 `RetentionAssessment`.
    MalformedAssessment {
        assessment_digest: String,
        why: String,
    },
    /// A head read claims to have happened and its event is not retained.
    NoSuchHeadReadEvent { event_digest: String },
    /// The retained head-read event or its snapshot is not what its digest names,
    /// or does not conform to §8.1.
    MalformedHeadRead { event_digest: String, why: String },
    /// A scan claims evidence that is not retained, is not what its digest names,
    /// or does not answer the query the scan says it does.
    UnevidencedScan {
        snapshot_digest: String,
        why: String,
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

/// Resolve one digest to the bytes it names, refusing anything else.
///
/// Three refusals, in order, and none of them is optional:
///
/// 1. the store has no such record;
/// 2. the store returned bytes whose digest is not the one requested — the
///    resolver is not trusted, because a resolver that can answer a correct
///    request with another object is the substitution the whole chain is
///    supposed to make impossible;
/// 3. the record has no reachable authorising assessment, which redaction
///    policy §9.2 calls bytes somebody kept rather than evidence somebody was
///    permitted to keep.
fn resolve_record<E: RetainedEvidence>(
    store: &E,
    requested: &str,
    declared: &[DeclaredBinding],
    into: &mut Vec<Unresolved>,
) -> Option<Value> {
    let Some(record) = store.resolve(requested) else {
        into.push(Unresolved::NoSuchRecord {
            digest: requested.to_owned(),
        });
        return None;
    };

    match digest(&record) {
        Ok(computed) if computed.as_str() == requested => {}
        Ok(computed) => {
            into.push(Unresolved::RecordDigestMismatch {
                requested: requested.to_owned(),
                computed: computed.as_str().to_owned(),
            });
            return None;
        }
        Err(e) => {
            into.push(Unresolved::RecordDigestMismatch {
                requested: requested.to_owned(),
                computed: format!("<not canonicalizable: {e}>"),
            });
            return None;
        }
    }

    let Some(binding) = store.binding_for(requested) else {
        into.push(Unresolved::NoRetentionBinding {
            record_digest: requested.to_owned(),
        });
        return None;
    };

    // THE SUBJECT RELATION. A binding is a claim about a particular record, and
    // the store answering a request about A with a binding naming B is a
    // well-formed pointer at the wrong subject — worse than a missing one,
    // because it resolves. Nothing else in the chain would notice.
    if binding.record_digest != requested {
        into.push(Unresolved::BindingSubjectMismatch {
            requested: requested.to_owned(),
            binding_names: binding.record_digest.clone(),
        });
        return None;
    }

    // THE REFERENCE MUST RESOLVE. §9.2 requires the assessment's canonical bytes
    // retained and reachable: a binding naming an assessment nobody kept
    // authorises nothing, and reading only the digest string makes the
    // permission a rumour about a document.
    let Some(assessment) = store.resolve(&binding.assessment_digest) else {
        into.push(Unresolved::NoSuchAssessment {
            record_digest: requested.to_owned(),
            assessment_digest: binding.assessment_digest.clone(),
        });
        return None;
    };
    match digest(&assessment) {
        Ok(computed) if computed.as_str() == binding.assessment_digest => {}
        Ok(computed) => {
            into.push(Unresolved::AssessmentDigestMismatch {
                requested: binding.assessment_digest.clone(),
                computed: computed.as_str().to_owned(),
            });
            return None;
        }
        Err(e) => {
            into.push(Unresolved::AssessmentDigestMismatch {
                requested: binding.assessment_digest.clone(),
                computed: format!("<not canonicalizable: {e}>"),
            });
            return None;
        }
    }
    if let Err(why) = check_assessment_shape(&assessment) {
        into.push(Unresolved::MalformedAssessment {
            assessment_digest: binding.assessment_digest.clone(),
            why,
        });
        return None;
    }

    // The retained binding is the authority on its own outcome. A basis that
    // names another assessment is asserting a permission that was never
    // granted, and it resolves perfectly well if nobody compares the two.
    for asserted in declared.iter().filter(|b| b.record_digest == requested) {
        if asserted.assessment_digest != binding.assessment_digest {
            into.push(Unresolved::BindingMismatch {
                record_digest: requested.to_owned(),
                declared: asserted.assessment_digest.clone(),
                retained: binding.assessment_digest.clone(),
            });
            return None;
        }
    }

    Some(record)
}

/// A resolved assessment is a conforming §9 `RetentionAssessment`.
///
/// The REQUIRED members only. This deliberately does not re-derive the redaction
/// gate's own decisions — `closure-redaction-policy-v1.md` froze those and this
/// crate consumes the outcome rather than re-deciding it. What it establishes is
/// that the object authorising a retention is the kind of object that can
/// authorise one at all: some other retained record, correctly digested and
/// honestly bound, is not a permission.
///
/// The assessment is NOT itself required to carry a `RetentionBinding`. It is a
/// control artifact, and requiring one would make every permission depend on a
/// permission — a recursion with no base case that would outlive the evidence it
/// was meant to protect.
fn check_assessment_shape(assessment: &Value) -> Result<(), String> {
    let kind = assessment
        .pointer("/sourceKind")
        .and_then(Value::as_str)
        .ok_or_else(|| "/sourceKind is absent or not a string".to_owned())?;
    if kind != "closure-retention-assessment" {
        return Err(format!(
            "/sourceKind is {kind:?}; an authorising assessment is a \
             closure-retention-assessment, not merely some retained object"
        ));
    }

    for required in [
        "/schemaVersion",
        "/redactionPolicyVersion",
        "/detector/id",
        "/detector/version",
        "/detector/configDigest",
        "/representation",
        "/assessedFields",
        "/coverageComplete",
        "/outcome",
        "/observedAt",
    ] {
        if assessment.pointer(required).is_none() {
            return Err(format!("{required} is REQUIRED by §9 and absent"));
        }
    }

    let outcome = assessment
        .pointer("/outcome")
        .and_then(Value::as_str)
        .ok_or_else(|| "/outcome is not a string".to_owned())?;
    if !["RETAIN", "BLOCK_SECRET", "CANNOT_ASSESS"].contains(&outcome) {
        return Err(format!(
            "/outcome is {outcome:?}; §9.3 defines RETAIN, BLOCK_SECRET, CANNOT_ASSESS"
        ));
    }

    // §9: a RETAIN outcome MUST NOT appear without coverageComplete: true.
    if outcome == "RETAIN" && assessment.pointer("/coverageComplete") != Some(&Value::Bool(true)) {
        return Err(
            "/outcome is RETAIN with /coverageComplete not true, which §9 forbids: a clean \
             verdict over an incomplete examination is not a clean verdict"
                .to_owned(),
        );
    }

    Ok(())
}

/// Read one pointer out of a resolved record, honouring the per-field retention
/// axis.
///
/// A complete §8 projection answers pointers directly. A
/// `github-reduced-source-record` answers only from `retainedFields`, and a
/// pointer in `blockedFields` is a RETENTION LOSS rather than an absent field —
/// the two are different facts and collapsing them loses the distinction
/// redaction policy §10 is built on.
///
/// The locator is deliberately not consulted. §7.3: a locator value is not
/// surviving source evidence and MUST NOT satisfy a decision-basis pointer,
/// because otherwise `/stableId` sits in `blockedFields` while the same
/// source-derived id stays readable through the locator — the field gate
/// bypassed by an alias.
fn read_pointer(record: &Value, digest_of: &str, pointer: &str) -> Result<Value, Unresolved> {
    let is_reduced = record.pointer("/sourceKind").and_then(Value::as_str)
        == Some("github-reduced-source-record");

    if !is_reduced {
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
        // Retention policy §7.1 partitions the required set exhaustively, so a
        // pointer in neither list is not "absent from this projection" — it is a
        // record that does not account for the field at all. Refused as a
        // retention loss, because admitting it would let an incomplete partition
        // read as evidence of nothing having been blocked.
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

    if fact.derived_from.len() != entry.arity {
        into.push(Unresolved::DerivationDisagrees {
            derivation: fact.derivation.clone(),
            claimed: fact.value.clone(),
            recomputed: Value::Null,
        });
        return;
    }

    let mut sources = Vec::with_capacity(entry.arity);
    for source_digest in &fact.derived_from {
        // Every cited source goes through the same resolution the decision's own
        // inputs do. A derived fact resting on bytes nobody was permitted to keep
        // is not better evidenced than a decision resting on them.
        match resolve_record(store, source_digest, declared, into) {
            Some(record) => sources.push(record),
            None => return,
        }
    }

    match (entry.derive)(&sources) {
        Some(recomputed) if recomputed == fact.value => {}
        Some(recomputed) => into.push(Unresolved::DerivationDisagrees {
            derivation: fact.derivation.clone(),
            claimed: fact.value.clone(),
            recomputed,
        }),
        // The derivation could not read what it needs. Not `false`: a rule that
        // could not run has established nothing, and reporting that as the
        // negative outcome is the absent-signal-as-negative-result error.
        None => into.push(Unresolved::DerivationDisagrees {
            derivation: fact.derivation.clone(),
            claimed: fact.value.clone(),
            recomputed: Value::Null,
        }),
    }
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
        let Some(record) = resolve_record(store, &input.source_digest, &basis.bindings, &mut why)
        else {
            continue;
        };
        match read_pointer(&record, &input.source_digest, &input.pointer) {
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
        if resolve_record(store, expected, &basis.bindings, &mut why).is_none() {
            // resolve_record has already recorded why.
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

    if let Err(why) = check_scan_evidence(scan, store) {
        return ScanVerdict::CannotCheck { why };
    }

    if claims == 0 {
        ScanVerdict::ZeroClaimsMeaningful
    } else {
        ScanVerdict::Claims(claims)
    }
}

/// The snapshot a scan names is retained, is what its digest names, is a query
/// snapshot, and answers THIS scan's query.
fn check_scan_evidence<E: RetainedEvidence>(
    scan: &FalsificationSurfaceScan,
    store: &E,
) -> Result<(), String> {
    let snapshot = store.resolve(&scan.snapshot_digest).ok_or_else(|| {
        format!(
            "the {} scan names snapshot {} and it is not retained; §16 requires the scan to \
             be evidenced, and a COMPLETE flag on its own is a caller's assertion about \
             itself",
            scan.surface, scan.snapshot_digest
        )
    })?;

    match digest(&snapshot) {
        Ok(computed) if computed.as_str() == scan.snapshot_digest => {}
        Ok(computed) => {
            return Err(format!(
                "the scan's evidence resolved to bytes that are not the ones {} names (they \
                 hash to {})",
                scan.snapshot_digest,
                computed.as_str()
            ))
        }
        Err(e) => return Err(format!("the scan's evidence cannot be canonicalized: {e}")),
    }

    if snapshot.pointer("/sourceKind").and_then(Value::as_str) != Some("github-query-snapshot") {
        return Err(format!(
            "the {} scan is evidenced by an object that is not a github-query-snapshot; only \
             a query snapshot carries the binding and enumeration a scan asserts, so a single \
             source object cannot evidence a scan of a surface",
            scan.surface
        ));
    }

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

    Ok(())
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
    read: &SubjectRead,
    expected_sha: &str,
    store: &E,
) -> Staleness {
    let mut observed = Vec::new();
    let mut unresolved = Vec::new();

    for (role, r) in [("head_before", &read.before), ("head_after", &read.after)] {
        match r {
            HeadRead::Failed { reason } => {
                unresolved.push(format!("{role} did not happen ({reason})"));
            }
            HeadRead::Observed { event_digest } => match resolve_head_sha(store, event_digest) {
                Ok(sha) => observed.push(sha),
                Err(why) => unresolved.push(format!("{role} {why}")),
            },
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

/// One head read, followed from its event to the retained snapshot that carries
/// the SHA.
fn resolve_head_sha<E: RetainedEvidence>(store: &E, event_digest: &str) -> Result<String, String> {
    let event = store
        .resolve(event_digest)
        .ok_or_else(|| format!("names an event that is not retained ({event_digest})"))?;
    match digest(&event) {
        Ok(computed) if computed.as_str() == event_digest => {}
        Ok(computed) => {
            return Err(format!(
                "resolved to bytes that are not the ones {event_digest} names (they hash to \
                 {computed})",
                computed = computed.as_str()
            ))
        }
        Err(e) => {
            return Err(format!(
                "resolved to bytes that cannot be canonicalized: {e}"
            ))
        }
    }

    // §8.1: an AVAILABLE event carries a REQUIRED snapshotDigest; a FAILED one
    // MUST NOT. An event claiming success without one is malformed, and inventing
    // a digest for it is precisely what that rule exists to prevent.
    if event.pointer("/acquisition").and_then(Value::as_str) != Some("AVAILABLE") {
        return Err("is an event whose acquisition is not AVAILABLE".to_owned());
    }
    let snapshot_digest = event
        .pointer("/snapshotDigest")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "is an AVAILABLE event with no snapshotDigest, which §8.1 requires".to_owned()
        })?;

    let snapshot = store
        .resolve(snapshot_digest)
        .ok_or_else(|| format!("names a snapshot that is not retained ({snapshot_digest})"))?;
    match digest(&snapshot) {
        Ok(computed) if computed.as_str() == snapshot_digest => {}
        Ok(_) => return Err("names a snapshot whose bytes are not the ones it names".to_owned()),
        Err(e) => {
            return Err(format!(
                "names a snapshot that cannot be canonicalized: {e}"
            ))
        }
    }
    if snapshot.pointer("/sourceKind").and_then(Value::as_str) != Some("github-pull-request-head") {
        return Err("names a snapshot that is not a github-pull-request-head".to_owned());
    }

    snapshot
        .pointer("/headSha")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "names a head snapshot with no headSha".to_owned())
}
