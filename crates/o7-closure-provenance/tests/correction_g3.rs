//! RED-REDACTION-DOMAIN — G3. A member declared as "a string" is not a member
//! whose value means anything.
//!
//! THE CLAIM:
//!
//! > `observedAt` is the only statement a retained assessment makes about WHEN
//! > the detector looked, and `MemberKind::Text` accepts `"banana"`.
//!
//! THE ORDER MATTERS, AND IT IS THE OPPOSITE OF THE OBVIOUS ONE. The wrong fix
//! is `if member == "observedAt" && looks_like_a_timestamp(v)`, and it is wrong
//! before it is ugly: until this round both contracts declared the member and
//! said NOTHING about its value, so a consumer inventing a timestamp rule would
//! be enforcing a requirement no contract states — the same defect as an
//! implementation-declared `sourceKind`, pointing the other way. So the value
//! domain is frozen in the contracts FIRST, in this same commit:
//!
//! ```text
//! redaction policy V1 §9    observedAt is RFC 3339 UTC, literal Z,
//!                           no fractional seconds, no numeric offset
//! provenance V1 §8.1        the same, for both head-read event shapes
//! ```
//!
//! WHY THIS IS NOT REWRITING A CONTRACT TO BLESS THE CODE. It moves in the
//! opposite direction: before the amendment the implementation conformed, and
//! after it the implementation is WRONG and this file is red. A freeze is
//! destroyed by amendments that make current behaviour retroactively correct,
//! not by amendments that make it retroactively incorrect. Nothing here narrows
//! an existing requirement — the contracts stated no value domain at all, and
//! §8's `null`-versus-absent argument already implies this one.
//!
//! THE DENOMINATOR IS THE POINT. CX-1 was a check whose ceiling nobody had
//! enumerated, and fixing one member of an unenumerated set is how that happens
//! a third time. So this file does not ask "is `observedAt` bounded". It WALKS
//! every member table in the crate, collects every `Text` and `TextArray`
//! member, and requires each to carry a classification:
//!
//! ```text
//! Here       a check in this crate constrains the value, and the row names
//!            the witness that turns red when it is removed
//! Elsewhere  constrained by a mechanism that exists for another reason, and
//!            the row says which
//! Residual   nothing constrains it, and the row says WHAT THAT LEAVES OPEN
//! ```
//!
//! A member the walk finds and the table does not classify fails G3-F. A
//! classification naming a witness that does not exist fails G3-G. The
//! inventory is therefore an audit with a derived denominator, not a list
//! somebody wrote once and stopped maintaining.
//!
//! WHAT THE INVENTORY FOUND, and it is why the denominator was worth deriving:
//! `observedAt` appears THREE times, not once. The assessment carries one and
//! both head-read event shapes carry another each, and the two event ones are
//! load-bearing in a way the assessment's is not — §8.1 exists to bracket an
//! evaluation between two reads, and an unconstrained `observedAt` cannot order
//! them. The reported finding named one site.
//!
//! ```text
//! G3-A  assessment observedAt = "banana"                      RED
//! G3-B  head-read AVAILABLE observedAt = "banana"             RED
//! G3-C  head-read FAILED observedAt = "banana"                RED
//! G3-D  near-misses: fractional seconds, numeric offset,
//!       no designator, an instant that does not exist         RED
//! G3-E  a conforming instant is still accepted           BOUNDARY
//! G3-F  every Text/TextArray member is classified       INVENTORY
//! G3-G  every classification is checkable               INVENTORY
//! ```

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_matcher::ValueKind;
use o7_closure_provenance::artifact::{
    validate, HEAD_READ_EVENT_AVAILABLE, HEAD_READ_EVENT_FAILED, LOCATOR_SHAPES, REDUCED_RECORD,
    RETENTION_BINDING,
};
use o7_closure_provenance::redaction::{MemberKind, ASSESSMENT_CONDITIONAL, ASSESSMENT_REQUIRED};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const THIS_FILE: &str = include_str!("correction_g3.rs");

/// The witness files a `Bounding::Here` row may point into, and their bytes.
///
/// The guard below used to search THIS FILE only, which was right about the
/// property — a row may not outlive the check it claims — and too narrow about
/// where the check may live. K1's evidence correction is the case that showed
/// it: the bound on `redactionPolicyVersion` is held by witnesses in
/// `correction_k1.rs`, and the alternative to widening this table was copying
/// them here, which would make two witnesses for one rule and leave the
/// inventory citing the copy.
const WITNESS_FILES: &[(&str, &str)] = &[
    ("correction_g3.rs", THIS_FILE),
    ("correction_k1.rs", include_str!("correction_k1.rs")),
    ("correction_m2.rs", include_str!("correction_m2.rs")),
];

// ---- The behavioural half.

fn assessment_observed_at(observed_at: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": "1",
        "detector": {
            "id": "synthetic-detector",
            "version": "1",
            "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        },
        "representation": "decoded-source-field-values",
        "assessedFields": ["/id"],
        "coverageComplete": true,
        "outcome": "RETAIN",
        "observedAt": observed_at,
    })
}

fn available_event(observed_at: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE",
        "acquisition": "AVAILABLE",
        "snapshotDigest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        "observedAt": observed_at,
    })
}

fn failed_event(observed_at: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": "HEAD_AFTER",
        "acquisition": "FAILED",
        "reason": "the read did not complete",
        "observedAt": observed_at,
    })
}

/// Validation is the door, so this asks the door directly rather than through a
/// decision: the question is whether the OBJECT conforms, and routing it through
/// `admissibility` would make a shape result depend on a basis.
fn validates(object: &Value) -> Result<(), String> {
    let d = digest(object).expect("digest").as_str().to_owned();
    match validate(&d, object) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("{e:?}")),
    }
}

#[track_caller]
fn refuses(object: &Value, what: &str, value: &str) {
    assert!(
        validates(object).is_err(),
        "{what}: accepted {value:?} as an observation time. The contracts freeze this member to \
         an RFC 3339 UTC instant with a literal Z; a member that admits any string records that \
         a time was written down, not when anything was observed"
    );
}

/// G3-A — the reported site. An assessment claiming it observed the source at
/// `"banana"`.
#[test]
fn g3a_an_assessment_observed_at_must_be_an_instant() {
    refuses(&assessment_observed_at(json!("banana")), "G3-A", "banana");
}

/// G3-B — the site the report did not name, and the one that costs more. §8.1
/// brackets an evaluation between two head reads, and `observedAt` is what says
/// which came first. Two events neither of which carries an orderable instant
/// are a bracket in name only.
#[test]
fn g3b_an_available_head_read_observed_at_must_be_an_instant() {
    refuses(&available_event(json!("banana")), "G3-B", "banana");
}

/// G3-C — and the FAILED shape, carried separately because §8.1 gives the event
/// two closed forms and a fix applied to one table is a fix applied to one
/// table.
#[test]
fn g3c_a_failed_head_read_observed_at_must_be_an_instant() {
    refuses(&failed_event(json!("banana")), "G3-C", "banana");
}

/// G3-D — the near-misses, which are the cases that decide whether this is a
/// value domain or a plausibility check.
///
/// `"banana"` is refused by anything. These four are refused only by a rule that
/// knows what the contract froze: three are the SAME INSTANT under three
/// spellings, and one is a well-formed string naming a time that does not exist.
/// Both contracts settle the first on digest identity — one fact, one canonical
/// form — and a checker that accepts all three lets one assessment exist under
/// three digests.
#[test]
fn g3d_near_misses_are_refused_by_the_domain_and_not_by_luck() {
    for (value, why) in [
        ("2026-08-05T09:03:00.000Z", "fractional seconds"),
        ("2026-08-05T11:03:00+02:00", "a numeric offset"),
        ("2026-08-05T09:03:00", "no designator at all"),
        ("2026-02-30T09:03:00Z", "a date that does not exist"),
        // Found by a negative control, not by writing the list. Relaxing the
        // length check left every case above still refused — the Z check caught
        // the offset, the punctuation check caught the rest — so nothing in this
        // file was actually holding the length. A string that is a conforming
        // instant with something appended is the case only the length refuses.
        (
            "2026-08-05T09:03:00Z ",
            "a conforming instant with a trailing space",
        ),
        (
            "2026-08-05T09:03:00Zt",
            "a conforming instant with anything appended",
        ),
    ] {
        let what = format!("G3-D ({why})");
        refuses(&assessment_observed_at(json!(value)), &what, value);
        refuses(&available_event(json!(value)), &what, value);
        refuses(&failed_event(json!(value)), &what, value);
    }
}

/// G3-E — BOUNDARY. The form every fixture in this crate already uses, and the
/// one the contracts freeze, must keep validating. A domain check that refused
/// it would have replaced an absent rule with a wrong one.
#[test]
fn g3e_a_conforming_instant_is_still_accepted() {
    for object in [
        assessment_observed_at(json!("2026-08-05T09:03:00Z")),
        available_event(json!("2026-08-05T09:03:00Z")),
        failed_event(json!("2026-08-05T09:03:00Z")),
    ] {
        assert!(
            validates(&object).is_ok(),
            "G3-E: refused a conforming RFC 3339 UTC instant: {:?}",
            validates(&object)
        );
    }
}

// ---- The inventory, over a DERIVED denominator.

/// How a string-typed member's value is constrained, if it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bounding {
    /// A check in this crate constrains the value. Names the witness that turns
    /// red when the check is removed, and the file it lives in — both, because
    /// the claim is only checkable if the guard can find the witness.
    Here {
        file: &'static str,
        witness: &'static str,
    },
    /// Constrained by a mechanism that exists for another reason. Names it.
    Elsewhere(&'static str),
    /// Nothing constrains it. Says what that leaves open.
    Residual(&'static str),
}

/// Every `Text`/`TextArray` member this crate declares, and what bounds it.
///
/// Keyed by `table/member` so two members of one name in different tables are
/// two rows — `observedAt` appears three times and they are not one fact.
///
/// ERRATUM, K1 — TWO ROWS OF THIS TABLE WERE WRONG, AND THE TABLE IS THE
/// MECHANISM THAT WAS SUPPOSED TO MAKE THAT IMPOSSIBLE.
///
/// Until `GREEN-POLICY-DOMAIN` both `redactionPolicyVersion` rows read
/// `Elsewhere`, citing §9.5's equality:
///
/// ```text
/// assessment/redactionPolicyVersion
///     "§9.5 equality: a record's own /redactionPolicyVersion must equal its
///      authorising assessment's, checked by check_authorises. The VALUE is
///      unconstrained — that a policy by that name was ever published is not
///      established here"
/// reduced/redactionPolicyVersion
///     "§9.5 equality, as for the assessment's"
/// ```
///
/// External review (Codex, on `30e0891`) falsified that classification, and it
/// was wrong in two independent ways rather than one:
///
/// 1. The cited check DOES NOT RUN for a complete §8 projection. Those carry no
///    `redactionPolicyVersion`, so there is no second value to compare, and
///    `check_authorises` said so in its own comment. The citation named a check
///    that case never reached. This is the half the review reported.
/// 2. EQUALITY IS NOT A BOUND ON RANGE, and this half was found by testing the
///    citation rather than reading it. Both values come from the same producer,
///    so the same credential on both sides satisfies §9.5 exactly.
///    `correction_k1.rs`'s K1-B is that specimen.
///
/// The first row's own text half-admitted it — "The VALUE is unconstrained" —
/// and still classified the member as bounded. That is the defect worth
/// recording: `Elsewhere` was read as "someone else's problem" when it means
/// "constrained by a mechanism that exists for another reason", and a
/// justification that names what it does NOT establish is not a bound. The
/// rows below now read `Here`, naming witnesses that turn red when the check is
/// removed.
///
/// ERRATUM, M2 — AND THE SECOND TIME THIS TABLE CLASSIFIED AN UNBOUNDED MEMBER
/// AS ACCOUNTED FOR.
///
/// Until `GREEN-DETECTOR-RANGE` all three `detector` rows read `Residual`:
///
/// ```text
/// assessment/detector/id
///     "DET-BIND is OWED. No registry binds a detector identity to an
///      implementation, so a record may name a detector nobody ran"
/// assessment/detector/version
///     "DET-BIND is OWED, as for detector/id"
/// assessment/detector/configDigest
///     "DET-BIND is OWED. Nothing resolves this digest, so unlike every other
///      digest in these tables it is never re-digested against bytes — it is a
///      string shaped like a commitment to a configuration nobody retained"
/// ```
///
/// Every sentence of that is TRUE, and none of it is what §9.4 asks. External
/// review (Codex, on `b9eaa7d`) reported `detector.id` as free text, and the
/// classification failed by answering a different question: binding an identity
/// to something that ran and bounding what the member may CARRY are two
/// obligations. DET-BIND is the first. §9.4 asks only the second, and no row
/// here addressed it.
///
/// That is a subtler failure than K1's and worth separating from it. K1's rows
/// cited a mechanism that did not hold. These cited a mechanism that does hold,
/// for a property nobody had asked about — a residual that is accurate,
/// load-bearing, and not a bound. `Residual` means "names what it leaves open";
/// these named something else that was also open.
///
/// The rows below now read `Here` for the RANGE. DET-BIND is unchanged, still
/// OWED, and still the reason `configDigest` is a recorded claim whose
/// referenced configuration is not resolved — closing a range is not resolving
/// a reference, and this erratum makes no such claim.
///
/// The prior text is quoted above rather than replaced silently, per the
/// adjudication: the earlier classification is not to be made to look correct
/// in hindsight. An inventory whose failures are edited out of it is the same
/// instrument as one that never ran.
const INVENTORY: &[(&str, Bounding)] = &[
    // §9's assessment.
    (
        "assessment/redactionPolicyVersion",
        Bounding::Here {
            file: "correction_k1.rs",
            witness: "k1a_an_unbounded_policy_version_authorises_a_complete_projection",
        },
    ),
    (
        "assessment/assessedFields",
        Bounding::Elsewhere(
            "the CX-1 ceiling in check_authorises: assessedFields ⊆ always ∪ present_only for \
             the record's locatorKind",
        ),
    ),
    (
        "assessment/observedAt",
        Bounding::Here {
            file: "correction_g3.rs",
            witness: "g3a_an_assessment_observed_at_must_be_an_instant",
        },
    ),
    (
        "assessment/detector/id",
        Bounding::Here {
            file: "correction_m2.rs",
            witness: "m2a_a_detector_id_of_free_text_authorises_a_record",
        },
    ),
    (
        "assessment/detector/version",
        Bounding::Here {
            file: "correction_m2.rs",
            witness: "m2b_a_detector_version_of_free_text_authorises_a_record",
        },
    ),
    (
        "assessment/detector/configDigest",
        Bounding::Here {
            file: "correction_m2.rs",
            witness: "m2c_a_detector_config_digest_of_free_text_authorises_a_record",
        },
    ),
    (
        "assessment/findings/field",
        Bounding::Elsewhere(
            "check_authorises: every finding's field must be in the §5.3 required set AND in \
             blockedFields. A finding naming a pointer outside the denominator blocks nothing \
             while reporting BLOCK_SECRET, and is refused",
        ),
    ),
    (
        "assessment/findings/findingId",
        Bounding::Residual(
            "no caller-supplied allowlist, by ruling. A finding may name a rule id no \
             configuration defines; §9's own text makes it a rule identifier from the BOUND \
             detector configuration, and the binding is DET-BIND's OWED half",
        ),
    ),
    // §7's reduced source record.
    (
        "reduced/locatorKind",
        Bounding::Here {
            file: "correction_g3.rs",
            witness: "g3h_an_unknown_locator_kind_is_refused_by_its_value_domain",
        },
    ),
    (
        "reduced/redactionPolicyVersion",
        Bounding::Here {
            file: "correction_k1.rs",
            witness: "k1b_equality_is_not_a_bound_on_range",
        },
    ),
    (
        "reduced/blockedFields",
        Bounding::Elsewhere(
            "§5.2's normative denominator: every partition key must be in the required set for \
             the record's locatorKind",
        ),
    ),
    // §7.3's locators. One row per kind, because the shapes differ.
    (
        "locator/repository",
        Bounding::Elsewhere(
            "§7.3: a locator is identity and never surviving evidence. read_pointer refuses to \
             answer a decision pointer out of it, so its value cannot reach a decision",
        ),
    ),
    (
        "locator/pullRequest",
        Bounding::Elsewhere("§7.3 identity-only, as for locator/repository"),
    ),
    (
        "locator/stableId",
        Bounding::Elsewhere("§7.3 identity-only, as for locator/repository"),
    ),
    // §9.2's retention binding.
    (
        "binding/recordDigest",
        Bounding::Elsewhere(
            "§9.2's chain compares it for equality against the independently requested record \
             digest; a value that is not that digest fails the subject relation",
        ),
    ),
    (
        "binding/assessmentDigest",
        Bounding::Elsewhere(
            "§9.2 requires resolve(D) and re-digests the bytes that come back, so a value \
             naming nothing retained fails to resolve",
        ),
    ),
    // §8.1's head-read events.
    (
        "head_read_available/snapshotDigest",
        Bounding::Elsewhere(
            "resolved through resolve_artifact as ConsumedAs::SubjectHead, which re-digests",
        ),
    ),
    (
        "head_read_available/observedAt",
        Bounding::Here {
            file: "correction_g3.rs",
            witness: "g3b_an_available_head_read_observed_at_must_be_an_instant",
        },
    ),
    (
        "head_read_failed/observedAt",
        Bounding::Here {
            file: "correction_g3.rs",
            witness: "g3c_a_failed_head_read_observed_at_must_be_an_instant",
        },
    ),
    (
        "head_read_failed/reason",
        Bounding::Residual(
            "free text in a retained object. §9.4 removed free text from findings deliberately, \
             and this member was not reconsidered when that was decided. Narrower than the \
             finding case — an acquisition layer writes it, not a detector over secret-bearing \
             content — but nothing here bounds what it may carry. NOT closed by this round: a \
             closed reason set is a contract change this file does not make on its own",
        ),
    ),
];

/// Walk a redaction member table, collecting the `Text`/`TextArray` members
/// under `table/member` keys.
fn walk_redaction(
    prefix: &str,
    members: &[o7_closure_provenance::redaction::Member],
) -> Vec<String> {
    let mut found = Vec::new();
    for member in members {
        let at = format!("{prefix}/{}", member.name);
        match member.kind {
            // Timestamp is string-valued and belongs in the denominator: the
            // question this file asks is which STRING members have a value
            // domain, and a member that has just acquired one is the answer
            // changing rather than the question going away.
            MemberKind::Text | MemberKind::TextArray | MemberKind::Timestamp => found.push(at),
            MemberKind::Object(nested) => found.extend(walk_redaction(&at, nested)),
            MemberKind::ObjectArray(nested) => found.extend(walk_redaction(&at, nested)),
            // Exhaustive on purpose. A kind added later must be classified here
            // deliberately, and a catch-all would let it leave the denominator
            // silently — which is the shape of the defect this file is about.
            MemberKind::Integer | MemberKind::Bool | MemberKind::OneOf(_) => {}
        }
    }
    found
}

/// The same over an artifact table, whose members are the matcher crate's.
fn walk_artifact(prefix: &str, members: &[o7_closure_matcher::Member]) -> Vec<String> {
    let mut found = Vec::new();
    for member in members {
        let at = format!("{prefix}/{}", member.name);
        match member.kind {
            ValueKind::Text | ValueKind::TextArray | ValueKind::Timestamp => found.push(at),
            ValueKind::Object(nested) => found.extend(walk_artifact(&at, nested)),
            // Exhaustive for the same reason as the redaction walk above. The
            // `_ => {}` this replaces was the CX-1 shape sitting inside the
            // audit written to prevent it: a string-valued kind added to the
            // matcher crate would have dropped out of the denominator with
            // nothing failing, and its inventory rows would have turned into
            // orphans blamed on the tables.
            ValueKind::Integer | ValueKind::Bool | ValueKind::OneOf(_) | ValueKind::OpenObject => {}
        }
    }
    found
}

/// Every `Text`/`TextArray` member the crate's own tables declare.
fn declared_string_members() -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    all.extend(walk_redaction("assessment", ASSESSMENT_REQUIRED));
    all.extend(walk_redaction("assessment", ASSESSMENT_CONDITIONAL));
    all.extend(walk_artifact("reduced", REDUCED_RECORD));
    all.extend(walk_artifact("binding", RETENTION_BINDING));
    all.extend(walk_artifact(
        "head_read_available",
        HEAD_READ_EVENT_AVAILABLE,
    ));
    all.extend(walk_artifact("head_read_failed", HEAD_READ_EVENT_FAILED));
    // §7.3's locator shapes are per-kind and mostly the same three members; the
    // inventory classifies the members, so they collapse to one key each.
    for shape in LOCATOR_SHAPES {
        all.extend(walk_artifact("locator", shape.members));
    }
    // The reduced record's own /locator is an OpenObject at that level and is
    // shaped by the tables above, so it contributes no member of its own.
    all
}

/// G3-H — the crate already bounds a `Text` member's value domain, and this is
/// the control that says so.
///
/// `locatorKind` is declared `ValueKind::Text` exactly like `observedAt`, and
/// `check_reduced_locator` refuses a value §7.3 defines no shape for. So the
/// question G3 raises is not whether a value domain on a string member is a
/// thing this crate does — it is why three members do not have one. Carried as a
/// passing witness at RED so the inventory's one `Here` row is a claim with
/// evidence rather than a category waiting to be filled.
#[test]
fn g3h_an_unknown_locator_kind_is_refused_by_its_value_domain() {
    let record = json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-something-nobody-defined",
        "locator": {"repository": "PhysShell/007", "stableId": "1"},
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": {},
        "blockedFields": [],
    });
    assert!(
        validates(&record).is_err(),
        "G3-H: accepted a locatorKind §7.3 defines no shape for. If this ever passes, the one \
         Here row in the inventory is unfounded and every Elsewhere row that leans on a table \
         lookup needs re-examining"
    );
}

/// G3-F — the denominator is DERIVED, and every member in it is classified.
///
/// The failure this exists to prevent is the one CX-1 already caused once: a
/// check whose ceiling nobody enumerated, fixed at the member somebody noticed.
/// Deriving the set from the tables means a member added later arrives
/// unclassified and fails here, rather than joining a list nobody rereads.
#[test]
fn g3f_every_string_member_is_classified() {
    let declared = declared_string_members();
    let classified: BTreeSet<String> = INVENTORY.iter().map(|(k, _)| (*k).to_owned()).collect();

    let unclassified: Vec<&String> = declared.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "these Text/TextArray members are declared and carry no classification: {unclassified:?}. \
         Every one accepts any string until something says otherwise, and an unlisted member is \
         the CX-1 shape exactly — a denominator nobody enumerated"
    );

    let orphans: Vec<&String> = classified.difference(&declared).collect();
    assert!(
        orphans.is_empty(),
        "these inventory rows classify members no table declares: {orphans:?}. A row for a \
         member that no longer exists is an audit reporting on nothing, and it is how the count \
         stays reassuring while the set moves"
    );
}

/// G3-G — every classification is checkable, and `Here` rows name a witness that
/// exists.
///
/// A classification is a claim. `Elsewhere` and `Residual` carry prose a reader
/// must weigh, and the least this can do is refuse an empty one. `Here` claims a
/// mechanism in this crate, and that claim is checkable: the named witness must
/// exist in the file the row names, so a check deleted with its test cannot
/// leave a row behind saying it is bounded.
///
/// The file is part of the row since K1's erratum. Searching only this file was
/// right about the property and wrong about the scope, and the row it could not
/// express is the one whose misclassification let an unbounded member sit in
/// the inventory reading `Elsewhere`.
#[test]
fn g3g_every_classification_is_checkable() {
    for (member, bounding) in INVENTORY {
        match bounding {
            Bounding::Here { file, witness } => {
                // Two asserts rather than a let-else and a `panic!`: this
                // crate denies `clippy::panic` in tests as well, and taking an
                // allowance to write a nicer control flow would be spending a
                // lint the E1 audit exists to keep honest.
                let source = WITNESS_FILES
                    .iter()
                    .find(|(name, _)| name == file)
                    .map(|(_, source)| *source)
                    .unwrap_or_default();
                assert!(
                    !source.is_empty(),
                    "{member} points at {file:?}, which WITNESS_FILES does not include. A row \
                     whose evidence this guard cannot open is a row nobody is checking"
                );
                assert!(
                    source.contains(&format!("fn {witness}(")),
                    "{member} claims a check held by {witness:?} in {file:?}, and that file \
                     declares no such witness. A row asserting a bound whose evidence does not \
                     exist is worse than no row"
                );
            }
            Bounding::Elsewhere(why) | Bounding::Residual(why) => assert!(
                why.len() > 20,
                "{member}: {why:?} does not say what bounds it or what is left open. A \
                 classification nobody can weigh is a checkbox"
            ),
        }
    }
}
