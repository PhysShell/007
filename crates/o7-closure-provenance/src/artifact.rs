//! The single door: no artifact reaches decision semantics unvalidated.
//!
//! Three review rounds established the law and then found the next path it had
//! not been carried to. That is a property of the design rather than a sequence
//! of oversights: closed-form validation was a HABIT that six functions each had
//! to remember, and a habit is remembered once per consumer.
//!
//! ```text
//! RetainedEvidence::resolve(..)
//!         |
//!         v  digest identity
//!         v  known artifact kind
//!         v  exact closed form — required, optional-if-present, no unknown
//!         v     members, nested closure too, registered schemaVersion
//!         v  gate classification
//!         |
//!   ValidatedArtifact
//!         |
//!         +--> retention authority      +--> scan semantics
//!         +--> pointer semantics        +--> head-read semantics
//!         +--> query replay             +--> subject and relation checks
//! ```
//!
//! WHY A TYPE RATHER THAN A CONVENTION. [`ValidatedArtifact`] holds its
//! `serde_json::Value` in a private field and has no public constructor. The
//! only way to obtain one is [`validate`], so a downstream function that takes
//! `&ValidatedArtifact` cannot be handed a raw resolved object at all. The
//! obligation moves from "every author remembers to call the validator" to
//! "the call does not type-check without it", and the class of defect the last
//! three rounds kept finding becomes inexpressible rather than merely absent.
//!
//! WHERE THE TABLES COME FROM, AND WHY NOT FROM HERE. The five §8 projections
//! and the §13 query snapshot are `o7-closure-matcher`'s
//! [`SOURCE_SCHEMAS`](o7_closure_matcher::SOURCE_SCHEMAS) and
//! [`QUERY_SNAPSHOT_SCHEMAS`](o7_closure_matcher::QUERY_SNAPSHOT_SCHEMAS),
//! already parsed out of the contract by that crate's `schema_parity.rs`, and
//! the walk over them is its `check_shape`. None of it is copied here. Slice B
//! adds only the forms Slice A has no reason to define — the reduced source
//! record with its per-`locatorKind` locator, the §8.1 head-read event, and the
//! §9 retention assessment — and `tests/contract_parity.rs` checks those against
//! their own contract the same way. A second transcription would buy runtime
//! completeness at the price of a new provenance drift, which is the trade this
//! project exists to refuse.

use o7_closure_canonical::digest;
use o7_closure_matcher::{check_shape, Member, ValueKind, QUERY_SNAPSHOT_SCHEMAS, SOURCE_SCHEMAS};
use serde_json::Value;

// ---- Shapes Slice A does not define.

const fn req(name: &'static str, kind: ValueKind) -> Member {
    Member {
        name,
        required: true,
        kind,
    }
}

/// §7.3, one row per gated kind. The locator's closed shape is chosen by the
/// record's own `locatorKind`, which is why it cannot be one fixed table.
///
/// `github-pull-request-head` has no `stableId` on purpose — §8.1 identifies the
/// subject read by repository and pull request, and inventing a synthetic id
/// here would contradict the merged schema.
pub struct LocatorShape {
    pub locator_kind: &'static str,
    pub members: &'static [Member],
}

const REPO_PR_ID: &[Member] = &[
    req("repository", ValueKind::Text),
    req("pullRequest", ValueKind::Text),
    req("stableId", ValueKind::Text),
];
const REPO_ID: &[Member] = &[
    req("repository", ValueKind::Text),
    req("stableId", ValueKind::Text),
];
const REPO_PR: &[Member] = &[
    req("repository", ValueKind::Text),
    req("pullRequest", ValueKind::Text),
];

pub const LOCATOR_SHAPES: &[LocatorShape] = &[
    LocatorShape {
        locator_kind: "github-issue-comment",
        members: REPO_PR_ID,
    },
    LocatorShape {
        locator_kind: "github-submitted-review",
        members: REPO_PR_ID,
    },
    LocatorShape {
        locator_kind: "github-review-comment",
        members: REPO_PR_ID,
    },
    LocatorShape {
        locator_kind: "github-actions-check",
        members: REPO_ID,
    },
    LocatorShape {
        locator_kind: "github-pull-request-head",
        members: REPO_PR,
    },
];

#[must_use]
pub fn locator_shape(locator_kind: &str) -> Option<&'static LocatorShape> {
    LOCATOR_SHAPES
        .iter()
        .find(|l| l.locator_kind == locator_kind)
}

/// §7's `github-reduced-source-record`, minus `locator`, which §7.3 shapes by
/// `locatorKind` and [`check_reduced_locator`] therefore checks separately.
///
/// `retainedFields` is an `OpenObject` at this level and nowhere else: its keys
/// are §5.3 pointers and its values are whatever the complete projection would
/// have carried, so no member table can describe it. §7.1's partition rules are
/// what close it, in `redaction::check_authorises`, once the assessment that
/// determines the partition is in hand. Named here rather than skipped.
pub const REDUCED_RECORD: &[Member] = &[
    req("schemaVersion", ValueKind::Integer),
    req(
        "sourceKind",
        ValueKind::OneOf(&["github-reduced-source-record"]),
    ),
    req("locatorKind", ValueKind::Text),
    req("locator", ValueKind::OpenObject),
    req("redactionPolicyVersion", ValueKind::Text),
    req(
        "outcome",
        ValueKind::OneOf(&["BLOCK_SECRET", "CANNOT_ASSESS"]),
    ),
    req("coverageComplete", ValueKind::Bool),
    req("retainedFields", ValueKind::OpenObject),
    req("blockedFields", ValueKind::TextArray),
];

/// §8.1's `HeadReadEvent`, `acquisition = AVAILABLE`.
///
/// §8.1's block names `role`, `acquisition`, `snapshotDigest` and `observedAt`
/// and does not repeat `schemaVersion` and `sourceKind`. Those are required
/// anyway by provenance V1 §7, which obliges every canonical object to be
/// domain-separated by its own content — the same reading §9 states outright for
/// the assessment. Recorded as an observation in the DOC pass rather than
/// decided silently here.
pub const HEAD_READ_EVENT_AVAILABLE: &[Member] = &[
    req("schemaVersion", ValueKind::Integer),
    req("sourceKind", ValueKind::OneOf(&["github-head-read-event"])),
    req("role", ValueKind::OneOf(&["HEAD_BEFORE", "HEAD_AFTER"])),
    req("acquisition", ValueKind::OneOf(&["AVAILABLE"])),
    req("snapshotDigest", ValueKind::Text),
    req("observedAt", ValueKind::Text),
];

/// §8.1's `HeadReadEvent`, `acquisition = FAILED`.
///
/// `snapshotDigest` is absent from this table, and that absence is the rule:
/// §8.1 says a failed read MUST NOT carry one, because the only digests
/// available to invent are a stale one or a fabricated one and both make a
/// failed read look like a successful read of unchanged bytes. A closed key set
/// enforces "MUST BE ABSENT" without a second rule saying so.
pub const HEAD_READ_EVENT_FAILED: &[Member] = &[
    req("schemaVersion", ValueKind::Integer),
    req("sourceKind", ValueKind::OneOf(&["github-head-read-event"])),
    req("role", ValueKind::OneOf(&["HEAD_BEFORE", "HEAD_AFTER"])),
    req("acquisition", ValueKind::OneOf(&["FAILED"])),
    req("reason", ValueKind::Text),
    req("observedAt", ValueKind::Text),
];

// ---- Kinds, and which side of the gate each falls on.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Produced through the redaction gate. §9.2: MUST have a reachable
    /// authorising assessment.
    Gated,
    /// Constructed rather than fetched, or a control artifact. No assessment
    /// about it can exist, and demanding one would demand a rubber stamp whose
    /// presence then reads as evidence that a gate ran.
    Ungated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// One of the five §8 complete projections. All are gated: §5.3 gives every
    /// one of them a required field set, `github-pull-request-head` included.
    CompleteProjection(&'static str),
    ReducedSourceRecord,
    QuerySnapshot,
    HeadReadEvent,
    RetentionAssessment,
}

impl ArtifactKind {
    #[must_use]
    pub const fn gate(self) -> Gate {
        match self {
            Self::CompleteProjection(_) | Self::ReducedSourceRecord => Gate::Gated,
            // §5.3 places `github-query-snapshot` outside the gate in as many
            // words. The event and the assessment are control artifacts: making
            // a permission depend on a permission is a recursion with no base
            // case that would outlive the evidence it protects.
            Self::QuerySnapshot | Self::HeadReadEvent | Self::RetentionAssessment => Gate::Ungated,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::CompleteProjection(kind) => kind,
            Self::ReducedSourceRecord => "github-reduced-source-record",
            Self::QuerySnapshot => "github-query-snapshot",
            Self::HeadReadEvent => "github-head-read-event",
            Self::RetentionAssessment => "closure-retention-assessment",
        }
    }
}

// ---- The validated artifact.

/// A retained object that has passed full closed-form validation.
///
/// The inner value is private and there is no public constructor: [`validate`]
/// is the only way to obtain one. Every downstream function in this crate takes
/// `&ValidatedArtifact`, so "was this validated?" stops being a question a
/// reviewer has to ask of each call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArtifact {
    digest: String,
    kind: ArtifactKind,
    value: Value,
}

impl ValidatedArtifact {
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    #[must_use]
    pub const fn kind(&self) -> ArtifactKind {
        self.kind
    }

    /// Read a pointer out of the validated object.
    ///
    /// Reading is not the risk and never was — reading something that was never
    /// established to be a conforming artifact is. Holding a `&self` here IS
    /// that establishment, which is why this is safe to expose and why nothing
    /// downstream needs the raw `Value`.
    #[must_use]
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        self.value.pointer(pointer)
    }

    #[must_use]
    pub fn str_at(&self, pointer: &str) -> Option<&str> {
        self.value.pointer(pointer).and_then(Value::as_str)
    }

    // `as_value()` WAS HERE, and it is REMOVED.
    //
    // It existed for one caller, `check_derived`, and it was justified by
    // provenance V1 §18's identity rule: changing `derive`'s parameter type
    // would force `review-carries-finding/2` for a signature change rather than
    // a behaviour change. The justification was sound and the conclusion was
    // wrong. Handing the rule the artifact's whole raw object did not merely
    // fail to carry the type-level proof — it threw away the distinction
    // between a canonical §8 projection and a §5.3-keyed reduced record, and
    // lost every derived fact over a redacted-but-retained source.
    //
    // `check_derived` now materialises a source view per the derivation's own
    // declared inputs, so the rule still reads a plain object, §18's identity
    // still lives in the implementation file's bytes, and no path in this crate
    // reaches an artifact's raw value again.
}

/// Why an artifact did not pass the door.
///
/// Two variants and not one, because the two are different facts about the
/// store. A digest mismatch says the resolver answered a correct request with
/// another object; a malformed artifact says the object it answered with is one
/// the contract forbids to exist. Collapsing them would lose which of the two
/// happened at exactly the boundary where that matters most.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DigestMismatch { computed: String },
    Malformed(String),
}

/// Everything clause 1 of the law requires, in one place, for every kind.
///
/// `requested` is the digest the DECISION BASIS named. The bytes are re-digested
/// against it here rather than trusted, so a store that answers a correct
/// request with another object is refused before its kind is even consulted.
pub fn validate(requested: &str, value: &Value) -> Result<ValidatedArtifact, ValidationError> {
    // 1. DIGEST IDENTITY. The resolver is not trusted.
    match digest(value) {
        Ok(computed) if computed.as_str() == requested => {}
        Ok(computed) => {
            return Err(ValidationError::DigestMismatch {
                computed: computed.as_str().to_owned(),
            })
        }
        Err(e) => {
            return Err(ValidationError::DigestMismatch {
                computed: format!("<not canonicalizable: {e}>"),
            })
        }
    }

    // 2. A KNOWN KIND. §7 obliges every canonical object to carry its own
    // sourceKind, so an object without one is not a canonical object at all —
    // not an object of some other kind to be handled leniently.
    let declared = value
        .pointer("/sourceKind")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ValidationError::Malformed(
                "/sourceKind is absent or not a string; §7 requires every canonical object \
                 to carry it"
                    .to_owned(),
            )
        })?;

    // 3. THE EXACT CLOSED FORM of the kind it DECLARES.
    let kind = check_closed_form(declared, value).map_err(ValidationError::Malformed)?;
    Ok(ValidatedArtifact {
        digest: requested.to_owned(),
        kind,
        value: value.clone(),
    })
}

/// Dispatch to the closed form of `declared` and check the whole object.
///
/// THE NEGATIVE CONTROL AIMS HERE. Removing or bypassing this call is what
/// "bypass the door" means, and `tests/correction_b4.rs` is required to turn red
/// when it is. A choke point nothing proves is a choke point is a new sign over
/// the old corridor.
fn check_closed_form(declared: &str, value: &Value) -> Result<ArtifactKind, String> {
    // The §8 projections and the §13 query snapshot, over Slice A's tables and
    // Slice A's walk. schemaVersion is matched as a REGISTERED value and not
    // merely as an integer: §8 gives a changed projection a new version, so a
    // version this registry does not know describes a key set nobody agreed to.
    if let Some(schema) = SOURCE_SCHEMAS.iter().find(|s| s.source_kind == declared) {
        check_registered_version(value, &[schema.schema_version], declared)?;
        check_shape(value, schema.members, "")?;
        return Ok(ArtifactKind::CompleteProjection(schema.source_kind));
    }

    match declared {
        "github-query-snapshot" => {
            let versions: Vec<i64> = QUERY_SNAPSHOT_SCHEMAS
                .iter()
                .map(|s| s.schema_version)
                .collect();
            let version = check_registered_version(value, &versions, declared)?;
            let schema = QUERY_SNAPSHOT_SCHEMAS
                .iter()
                .find(|s| s.schema_version == version)
                .ok_or_else(|| format!("no §13 shape for schemaVersion {version}"))?;
            check_shape(value, schema.members, "")?;
            Ok(ArtifactKind::QuerySnapshot)
        }
        "github-reduced-source-record" => {
            // §9.5: the three independently retained redaction objects each
            // carry schemaVersion 1. Registered value, not merely an integer.
            check_registered_version(value, &[1], declared)?;
            check_shape(value, REDUCED_RECORD, "")?;
            check_reduced_locator(value)?;
            Ok(ArtifactKind::ReducedSourceRecord)
        }
        "github-head-read-event" => {
            // §8.1 gives the event two shapes, chosen by its own acquisition
            // status. Checking one union of both would admit an AVAILABLE event
            // with no snapshotDigest and a FAILED event carrying one, which is
            // the exact confusion the split exists to prevent.
            let acquisition = value
                .pointer("/acquisition")
                .and_then(Value::as_str)
                .ok_or_else(|| "/acquisition is absent or not a string".to_owned())?;
            let members = match acquisition {
                "AVAILABLE" => HEAD_READ_EVENT_AVAILABLE,
                "FAILED" => HEAD_READ_EVENT_FAILED,
                other => {
                    return Err(format!(
                        "/acquisition is {other:?}; §8.1 defines AVAILABLE and FAILED, and \
                         NOT_PRODUCED is inadmissible on a head read because nobody can \
                         decline to emit the subject's own head"
                    ))
                }
            };
            check_registered_version(value, &[1], declared)?;
            check_shape(value, members, "")?;
            Ok(ArtifactKind::HeadReadEvent)
        }
        "closure-retention-assessment" => {
            // §9.5: the assessment is one of the three independently retained
            // redaction objects and carries schemaVersion 1. A registered value,
            // not merely an integer — the last escape RED-B4 measured on the one
            // artifact GREEN-B3 had already closed.
            check_registered_version(value, &[1], declared)?;
            crate::redaction::check_assessment(value)?;
            Ok(ArtifactKind::RetentionAssessment)
        }
        other => Err(format!(
            "{other:?} is not a kind this crate can read. An object declaring an \
             unregistered kind is evidence whose shape is unknown, which is not the same \
             as evidence that failed a check"
        )),
    }
}

/// §8 and §13 both give a changed shape a new version, so the version is part of
/// admissibility rather than a typed field.
fn check_registered_version(
    value: &Value,
    registered: &[i64],
    declared: &str,
) -> Result<i64, String> {
    let found = value
        .pointer("/schemaVersion")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
        })
        .ok_or_else(|| {
            "/schemaVersion is absent or not an integer; §7 requires every canonical \
             object to carry it"
                .to_owned()
        })?;
    if registered.contains(&found) {
        Ok(found)
    } else {
        Err(format!(
            "/schemaVersion is {found} and the registered versions of {declared:?} are \
             {registered:?}. A version this reader does not know describes a key set it \
             cannot check, and checking it against a neighbouring version's table would be \
             checking the wrong contract"
        ))
    }
}

/// §7.3: the locator's shape follows `locatorKind`, and §9.5 lists it among the
/// nested objects that are exact key sets.
fn check_reduced_locator(record: &Value) -> Result<(), String> {
    let locator_kind = record
        .pointer("/locatorKind")
        .and_then(Value::as_str)
        .ok_or_else(|| "/locatorKind is absent or not a string".to_owned())?;
    let shape = locator_shape(locator_kind).ok_or_else(|| {
        format!(
            "/locatorKind is {locator_kind:?}, for which §7.3 defines no locator shape. §7 \
             defines it as the provenance V1 kind that was REFUSED, and a kind the gate \
             never covered was never offered to it"
        )
    })?;
    let locator = record
        .pointer("/locator")
        .ok_or_else(|| "/locator is REQUIRED and absent".to_owned())?;
    check_shape(locator, shape.members, "/locator")
}
