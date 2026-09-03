//! The redaction gate's artifacts, as a consumer must check them.
//!
//! `closure-redaction-policy-v1.md` froze the gate. This module does not
//! re-decide any of it — it establishes that an artifact offered as a gate
//! outcome **is one**, and that its own recorded state **authorises the record
//! it is bound to**. Those are two different obligations and the second is the
//! one three review rounds kept finding missing.
//!
//! ```text
//! artifact validity   bytes, digest, type, closed schema
//! relation validity   the artifact's own fields establish the exact subject,
//!                     role, state, partition and relation under which this
//!                     decision consumes it
//! ```
//!
//! Resolving the right bytes proves *this artifact exists and these are its
//! bytes*. It does not prove *this artifact concerns this subject*, *has the
//! role this decision assigns it*, or *authorises this other artifact*.
//!
//! WHERE THE TABLES COME FROM. The §9 member list and the §5.3 required field
//! sets are transcribed here and checked against the contract document by
//! `tests/contract_parity.rs`, which parses them out of the markdown. The
//! expectation is the document, never a second copy of it.

use o7_closure_matcher::utc_instant;
use serde_json::Value;

/// One member of a closed shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub name: &'static str,
    pub kind: MemberKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Text,
    Integer,
    Bool,
    /// An array whose every element is a string.
    TextArray,
    /// An RFC 3339 UTC instant, exactly `YYYY-MM-DDThh:mm:ssZ`.
    ///
    /// Delegates to `o7_closure_matcher::utc_instant`, which is where the §8.1
    /// head-read events get the same rule. One implementation: redaction policy
    /// V1 §9 and provenance V1 §8.1 freeze the SAME domain, and two
    /// transcriptions of one rule is one rule too many — the principle
    /// `contract_parity.rs` exists to enforce, applied to a check rather than to
    /// a table.
    Timestamp,
    /// A string whose value must be one of a closed set.
    OneOf(&'static [&'static str]),
    /// A nested object with its own closed shape.
    Object(&'static [Member]),
    /// An array of objects, each with the same closed shape.
    ObjectArray(&'static [Member]),
}

/// §9's `detector` block.
const DETECTOR: &[Member] = &[
    Member {
        name: "id",
        kind: MemberKind::Text,
    },
    Member {
        name: "version",
        kind: MemberKind::Text,
    },
    Member {
        name: "configDigest",
        kind: MemberKind::Text,
    },
];

/// §9: findings carry `field` and `findingId`, and nothing else — §9.4 removed
/// free text deliberately, and §9.3 forbids the matched substring, an excerpt, a
/// prefix or suffix, a length, a character count, or a digest of the matched
/// bytes. A closed shape is how that prohibition is enforced rather than trusted.
const FINDING: &[Member] = &[
    Member {
        name: "field",
        kind: MemberKind::Text,
    },
    Member {
        name: "findingId",
        kind: MemberKind::Text,
    },
];

/// §9's REQUIRED members. The two CONDITIONAL ones are handled separately
/// because their rule is **iff**, not presence.
pub const ASSESSMENT_REQUIRED: &[Member] = &[
    Member {
        name: "schemaVersion",
        kind: MemberKind::Integer,
    },
    Member {
        name: "sourceKind",
        kind: MemberKind::OneOf(&["closure-retention-assessment"]),
    },
    Member {
        name: "redactionPolicyVersion",
        kind: MemberKind::Text,
    },
    Member {
        name: "detector",
        kind: MemberKind::Object(DETECTOR),
    },
    Member {
        name: "representation",
        kind: MemberKind::OneOf(&["decoded-source-field-values"]),
    },
    Member {
        name: "assessedFields",
        kind: MemberKind::TextArray,
    },
    Member {
        name: "coverageComplete",
        kind: MemberKind::Bool,
    },
    Member {
        name: "outcome",
        kind: MemberKind::OneOf(&["RETAIN", "BLOCK_SECRET", "CANNOT_ASSESS"]),
    },
    Member {
        name: "observedAt",
        kind: MemberKind::Timestamp,
    },
];

/// §9's CONDITIONAL members, present **iff** their condition holds.
pub const ASSESSMENT_CONDITIONAL: &[Member] = &[
    Member {
        name: "findings",
        kind: MemberKind::ObjectArray(FINDING),
    },
    Member {
        name: "coverageFailureCode",
        kind: MemberKind::OneOf(&[
            "DETECTOR_UNAVAILABLE",
            "DETECTOR_FAILED",
            "INCOMPLETE_COVERAGE",
            "INVALID_RESULT",
        ]),
    },
];

/// §5.3's required field set for one gated source kind.
///
/// Pointers into the **decoded source object** — GitHub's field names, not the
/// canonical projection's. That distinction is load-bearing for the partition
/// check: §7.1 builds `retainedFields` and `blockedFields` out of this set, so a
/// reduced record is keyed in this space while a complete §8 projection is keyed
/// in the canonical one.
#[derive(Debug, Clone, Copy)]
pub struct RequiredFields {
    pub locator_kind: &'static str,
    pub always: &'static [&'static str],
    pub present_only: &'static [PresentOnly],
}

/// One §5.3 present-only field, carried in BOTH of §7.5's vocabularies.
///
/// §5.3's rule has two halves and they live in different spaces. "Joins the
/// required set exactly when it is present in the decoded source" is a
/// statement about a decoded pointer; the only thing a consumer holds is a
/// record, and a complete §8 projection spells the same field canonically. One
/// pointer alone cannot express the rule, so the pair is the table entry.
///
/// The canonical half is not a transformation of the decoded half. It is §8's
/// own OPTIONAL-IF-PRESENT spelling, transcribed, and
/// `tests/contract_parity.rs` checks both halves against the two documents
/// rather than against each other — a camel-case function would agree with
/// itself no matter what §8 said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentOnly {
    /// §5.3's pointer into the decoded source object.
    pub decoded: &'static str,
    /// The member a complete §8 projection carries the same field under.
    pub canonical: &'static str,
}

pub const REQUIRED_FIELDS: &[RequiredFields] = &[
    RequiredFields {
        locator_kind: "github-issue-comment",
        always: &[
            "/id",
            "/user/id",
            "/user/login",
            "/user/type",
            "/author_association",
            "/body",
            "/created_at",
            "/updated_at",
        ],
        present_only: &[],
    },
    RequiredFields {
        locator_kind: "github-submitted-review",
        always: &[
            "/id",
            "/user/id",
            "/user/login",
            "/user/type",
            "/author_association",
            "/state",
            "/body",
            "/submitted_at",
            "/commit_id",
        ],
        present_only: &[],
    },
    RequiredFields {
        locator_kind: "github-review-comment",
        always: &[
            "/id",
            "/pull_request_review_id",
            "/user/id",
            "/user/login",
            "/user/type",
            "/author_association",
            "/body",
            "/commit_id",
            "/original_commit_id",
            "/path",
            "/created_at",
            "/updated_at",
        ],
        present_only: &[
            PresentOnly {
                decoded: "/in_reply_to_id",
                canonical: "inReplyToId",
            },
            PresentOnly {
                decoded: "/line",
                canonical: "line",
            },
            PresentOnly {
                decoded: "/original_line",
                canonical: "originalLine",
            },
            PresentOnly {
                decoded: "/side",
                canonical: "side",
            },
            PresentOnly {
                decoded: "/start_line",
                canonical: "startLine",
            },
        ],
    },
    RequiredFields {
        locator_kind: "github-pull-request-head",
        always: &["/number", "/head/sha", "/head/ref", "/head/repo/full_name"],
        present_only: &[PresentOnly {
            decoded: "/updated_at",
            canonical: "updatedAt",
        }],
    },
    RequiredFields {
        locator_kind: "github-actions-check",
        always: &["/id", "/name", "/head_sha", "/status"],
        present_only: &[
            PresentOnly {
                decoded: "/conclusion",
                canonical: "conclusion",
            },
            PresentOnly {
                decoded: "/started_at",
                canonical: "startedAt",
            },
            PresentOnly {
                decoded: "/completed_at",
                canonical: "completedAt",
            },
        ],
    },
];

#[must_use]
pub fn required_fields(locator_kind: &str) -> Option<&'static RequiredFields> {
    REQUIRED_FIELDS
        .iter()
        .find(|r| r.locator_kind == locator_kind)
}

/// Both directions of a closed shape.
pub(crate) fn check_shape(value: &Value, members: &[Member], path: &str) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}/ is not an object"))?;

    for member in members {
        let at = format!("{path}/{}", member.name);
        let Some(found) = object.get(member.name) else {
            return Err(format!("{at} is REQUIRED and absent"));
        };
        check_member(found, member.kind, &at)?;
    }

    for key in object.keys() {
        if !members.iter().any(|m| m.name == key.as_str()) {
            return Err(format!(
                "{path}/{key} is outside the closed key set. §9.4 is explicit that a closed \
                 schema is the security argument rather than a tidiness rule: an open one \
                 lets a producer add a member holding the very content the gate refused"
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_member(found: &Value, kind: MemberKind, at: &str) -> Result<(), String> {
    if found.is_null() {
        return Err(format!(
            "{at} is null; provenance V1 §8 settled that null and absent are one fact and \
             must have one encoding"
        ));
    }
    match kind {
        MemberKind::Text if found.as_str().is_none() => Err(format!("{at} is not a string")),
        MemberKind::Timestamp => {
            let text = found
                .as_str()
                .ok_or_else(|| format!("{at} is not a string"))?;
            utc_instant(text).map_err(|why| format!("{at} {why}"))
        }
        MemberKind::Integer if !found.is_i64() && !found.is_u64() => {
            Err(format!("{at} is not an integer"))
        }
        MemberKind::Bool if !found.is_boolean() => Err(format!("{at} is not a boolean")),
        MemberKind::TextArray => {
            let items = found
                .as_array()
                .ok_or_else(|| format!("{at} is not an array"))?;
            match items.iter().position(|i| i.as_str().is_none()) {
                Some(position) => Err(format!("{at}[{position}] is not a string")),
                None => Ok(()),
            }
        }
        MemberKind::OneOf(admissible) => {
            let text = found
                .as_str()
                .ok_or_else(|| format!("{at} is not a string"))?;
            if admissible.contains(&text) {
                Ok(())
            } else {
                Err(format!(
                    "{at} is {text:?}; the contract defines {admissible:?}"
                ))
            }
        }
        MemberKind::Object(nested) => check_shape(found, nested, at),
        MemberKind::ObjectArray(nested) => {
            let items = found
                .as_array()
                .ok_or_else(|| format!("{at} is not an array"))?;
            for (i, item) in items.iter().enumerate() {
                check_shape(item, nested, &format!("{at}[{i}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ---- The assessment as an artifact: §9's closed form, both directions.

/// Every member REQUIRED by §9, every CONDITIONAL member that is present, and
/// nothing else — plus the two **iff** rules and §5.5's array discipline.
///
/// This is clause 1 of the round's law, and the reason it is a whole-object
/// check rather than a list of pointer probes: §13.1 of the provenance contract
/// settled that *checking several significant members of an object is not
/// checking the object, when the contract defines admissibility by the whole
/// closed form*. §9.4 says the same thing from the other side — the closure IS
/// the security argument, because an open schema lets a producer add
/// `"debug": "<the secret>"` and satisfy every rule anybody wrote down.
pub(crate) fn check_assessment(assessment: &Value) -> Result<(), String> {
    let object = assessment
        .as_object()
        .ok_or_else(|| "the assessment is not an object".to_owned())?;

    for member in ASSESSMENT_REQUIRED {
        let at = format!("/{}", member.name);
        let found = object
            .get(member.name)
            .ok_or_else(|| format!("{at} is REQUIRED by §9 and absent"))?;
        check_member(found, member.kind, &at)?;
    }
    for member in ASSESSMENT_CONDITIONAL {
        if let Some(found) = object.get(member.name) {
            check_member(found, member.kind, &format!("/{}", member.name))?;
        }
    }
    for key in object.keys() {
        let known = ASSESSMENT_REQUIRED
            .iter()
            .chain(ASSESSMENT_CONDITIONAL)
            .any(|m| m.name == key.as_str());
        if !known {
            return Err(format!(
                "/{key} is outside §9's closed key set. §9.4 is explicit that the closure is \
                 the security argument rather than a tidiness rule: an open schema lets a \
                 producer add a member holding the very content the gate refused, and satisfy \
                 every rule that was written about the fields somebody thought of"
            ));
        }
    }

    let outcome = member_str(assessment, "outcome")?;
    let findings_present = object.contains_key("findings");

    // §9: `findings: []` and an absent `findings` would be two encodings of one
    // fact, and provenance V1 §8 settled that argument for null versus absent.
    //
    // The PRESENCE half of §9's iff is deliberately not checked here. §9.6's
    // computation below refuses every state that iff refuses, and refuses states
    // the iff admits as well — `CANNOT_ASSESS` over a complete, clean assessment
    // satisfies the iff perfectly. Keeping both would leave one of them with no
    // reachable failure, and a check that cannot fire is not a check: a witness
    // green over it would be evidence of nothing, which is the defect the
    // previous round withdrew a claim for.
    if object
        .get("findings")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        return Err(
            "/findings is present and empty. §5.1 defines BLOCK_SECRET as at least one \
             blocking finding, so an empty list is a record claiming both that something was \
             found and that nothing was"
                .to_owned(),
        );
    }

    let coverage_complete = object.get("coverageComplete") == Some(&Value::Bool(true));
    let code_present = object.contains_key("coverageFailureCode");
    if code_present == coverage_complete {
        return Err(format!(
            "§5.4: /coverageFailureCode is present IFF /coverageComplete is false; here \
             coverage is {} and the code is {}",
            if coverage_complete {
                "complete"
            } else {
                "incomplete"
            },
            if code_present { "present" } else { "absent" }
        ));
    }

    // §9's last bullet, stated separately from §5.1 because it is the one that
    // cannot be recovered afterwards: a clean verdict over an incomplete
    // examination is not a clean verdict.
    if outcome == "RETAIN" && !coverage_complete {
        return Err(
            "/outcome is RETAIN with /coverageComplete not true, which §9 forbids".to_owned(),
        );
    }

    // §5.1's precedence, which §9.6 makes the authority: outcome MUST equal the
    // computation over this assessment's OWN findings and coverage. Without it a
    // record can read CANNOT_ASSESS over a complete, clean assessment, and every
    // structural check still passes while the two halves disagree.
    let computed = if findings_present {
        "BLOCK_SECRET"
    } else if coverage_complete {
        "RETAIN"
    } else {
        "CANNOT_ASSESS"
    };
    if outcome != computed {
        return Err(format!(
            "§9.6: /outcome is {outcome:?} and the §5.1 computation over this assessment's own \
             findings and coverage yields {computed:?}"
        ));
    }

    // §5.5. These are logically sets and physically arrays, and JCS does not sort
    // arrays — so order is inside the digest whether or not anybody chose it. A
    // duplicate additionally corrupts the §7.1 partition arithmetic below.
    ordered_unique(assessment, "assessedFields")?;
    if let Some(findings) = object.get("findings").and_then(Value::as_array) {
        let mut previous: Option<(&str, &str)> = None;
        for (i, finding) in findings.iter().enumerate() {
            let field = finding
                .pointer("/field")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("/findings[{i}]/field is not a string"))?;
            let id = finding
                .pointer("/findingId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("/findings[{i}]/findingId is not a string"))?;
            if let Some(prev) = previous {
                if prev >= (field, id) {
                    return Err(format!(
                        "§5.5: /findings must be unique on (field, findingId) and ascending; \
                         {prev:?} is followed by {:?}",
                        (field, id)
                    ));
                }
            }
            previous = Some((field, id));
        }
    }

    Ok(())
}

/// A REQUIRED string member, already type-checked by [`check_assessment`].
fn member_str<'a>(object: &'a Value, name: &str) -> Result<&'a str, String> {
    object
        .pointer(&format!("/{name}"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("/{name} is not a string"))
}

/// §5.5: unique, ascending lexical JSON-pointer order.
fn ordered_unique(object: &Value, name: &str) -> Result<Vec<String>, String> {
    let items = object
        .pointer(&format!("/{name}"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("/{name} is not an array"))?;
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let text = item
            .as_str()
            .ok_or_else(|| format!("/{name}[{i}] is not a string"))?;
        if let Some(previous) = out.last() {
            if previous.as_str() >= text {
                return Err(format!(
                    "§5.5: /{name} must be unique and in ascending lexical order; {previous:?} \
                     is followed by {text:?}"
                ));
            }
        }
        out.push(text.to_owned());
    }
    Ok(out)
}

// ---- The assessment as a relation: does it authorise THIS record.

/// The kind whose §5.3 set is this record's denominator.
///
/// A reduced record names it in `locatorKind` — §7 defines that field as *the
/// provenance V1 kind that was refused*. A complete projection is its own kind.
/// Returning the pair rather than just the set keeps the two cases visible at
/// the call site, because the outcome rule differs between them and collapsing
/// that was one of the escapes.
fn denominator_of(
    record: &crate::artifact::ValidatedArtifact,
) -> Result<(bool, Option<&'static RequiredFields>), String> {
    let kind = record
        .pointer("/sourceKind")
        .and_then(Value::as_str)
        .ok_or_else(|| "/sourceKind is absent or not a string".to_owned())?;

    if kind != "github-reduced-source-record" {
        return Ok((false, required_fields(kind)));
    }

    let locator_kind = record
        .pointer("/locatorKind")
        .and_then(Value::as_str)
        .ok_or_else(|| "/locatorKind is absent or not a string".to_owned())?;
    // A kind with no §5.3 entry has no required set, so its partition is
    // VACUOUSLY exhaustive and every rule below is satisfied by a record that
    // accounted for nothing. §5.3 places github-query-snapshot outside the gate
    // outright, so it can never be the kind a gate outcome refused.
    let required = required_fields(locator_kind).ok_or_else(|| {
        format!(
            "/locatorKind is {locator_kind:?}, which §5.3 gives no required field set. §7 \
             defines it as the provenance V1 kind that was REFUSED, and a kind outside the \
             gate was never offered to it — a record keyed on one has an empty denominator, \
             so its partition is vacuously exhaustive and accounts for nothing"
        )
    })?;
    Ok((true, Some(required)))
}

/// Clause 2 of the round's law, for the assessment/record pair.
///
/// Everything the previous round established still holds when this fails: the
/// assessment is retained, its bytes are the ones its digest names, and it is a
/// conforming §9 object. What it does not establish is that this particular
/// assessment permits keeping this particular record — and a perfectly
/// conformant assessment reading `BLOCK_SECRET` resolves exactly as well as one
/// reading `RETAIN`.
///
/// WHAT IS AND IS NOT CHECKED, AND WHY. §5.3's present-only fields join the
/// required set exactly when they are present in the decoded source, and the
/// record is the only thing a consumer holds. That splits the set in two rather
/// than putting all of it out of reach:
///
/// ```text
/// the record carries the field    determinable — the record says it was there
/// the record does not carry it    undeterminable — dropped here, or never sent
/// ```
///
/// The first half is required, by the coverage rule below. An earlier revision
/// stated the second half's argument over the whole set and left `conclusion`
/// on a check run — present on real evidence, never in `always` — as content a
/// decision could read and no gate was ever asked about.
///
/// The second half is still not demanded. Refusing on it would refuse
/// conformant records, and asserting it would be a check that cannot see what
/// it claims to. The ceiling is unchanged: everything assessed must be in
/// `always ∪ present_only`.
pub(crate) fn check_authorises(
    record: &crate::artifact::ValidatedArtifact,
    assessment: &crate::artifact::ValidatedArtifact,
    expected_policy: &str,
    expected_detector: &crate::ExpectedDetector,
) -> Result<(), String> {
    // §9.4'S RANGE, AND IT IS THE ONE PROPERTY THE SCHEMA'S SECURITY ARGUMENT
    // RESTS ON:
    //
    //     no field of an assessment is free text
    //     "A closed field cannot carry a secret out because its RANGE does not
    //      depend on the content inspected."
    //
    // `redactionPolicyVersion` is declared Text and admits every string, so a
    // producer could write a credential into it and the assessment carrying it
    // is canonicalized, digested and retained permanently as the authority for
    // a record a decision then reads.
    //
    // THE EXPECTED VALUE IS THE CALLER'S, AND THAT IS NOT A DETAIL. No contract
    // sentence registers a permitted value — §9 declares the member, §9.5
    // relates the record's to the assessment's, and neither names one — so a
    // literal here would be this implementation inventing the norm it enforces,
    // which is the direction G3 refused for `observedAt`. Taking it from the
    // assessment would be asking the party that would be leaking to nominate
    // its own range. §17.1's first consequence settles it: the expectation
    // arrives from outside the artifacts being checked.
    //
    // §9.5'S EQUALITY IS A DIFFERENT RULE AND SURVIVES BELOW. It relates two
    // producer-supplied values to each other, which constrains DISAGREEMENT and
    // not RANGE — `correction_g3.rs` cited it as this member's bound and
    // `correction_k1.rs`'s K1-B is the specimen that satisfies it exactly with
    // the same credential on both sides.
    // §9.4'S RANGE FOR THE DETECTOR BLOCK, and it is the same rule as the
    // policy version's, one member family over. All three members are declared
    // Text, which admits every string, so a credential written into any of them
    // rides into an assessment that is retained permanently as a record's
    // authority. The expectation is the caller's for K1's reason: no contract
    // sentence registers a detector id, version or configuration digest, so a
    // literal here would invent the norm, and reading it off the assessment
    // would ask the party that would be leaking to nominate its own range.
    //
    // WHAT THIS DOES NOT DO, stated here because the member names invite the
    // other reading: it does not resolve `configDigest`, does not bind any of
    // the three to anything that ran, and does not discharge DET-BIND. It
    // closes the RANGE. A digest-shaped string whose referenced configuration
    // nobody retained is still exactly that, and §23's residual is unchanged.
    for (member, expectation) in [
        ("id", expected_detector.id.as_str()),
        ("version", expected_detector.version.as_str()),
        ("configDigest", expected_detector.config_digest.as_str()),
    ] {
        let declared = assessment
            .str_at(&format!("/detector/{member}"))
            .ok_or_else(|| format!("/detector/{member} is not a string"))?;
        if declared != expectation {
            return Err(format!(
                "§9.4: the assessment's /detector/{member} is {declared:?} and this evaluation \
                 accepts {expectation:?}. Every field of an assessment is a closed vocabulary \
                 value, a structural identifier, a boolean, or a JSON pointer — a member that \
                 admits every string cannot carry that property, and this one is retained \
                 permanently as a record's authority"
            ));
        }
    }

    let assessment_policy = member_str_at(assessment, "redactionPolicyVersion")?;
    if assessment_policy != expected_policy {
        return Err(format!(
            "§9.4/§9.5: the assessment was made under redaction policy version \
             {assessment_policy:?} and this evaluation is made under {expected_policy:?}. The \
             expected version is the caller's and never the assessment's — a member whose range \
             is every string is free text, whatever else it agrees with"
        ));
    }

    let (is_reduced, required) = denominator_of(record)?;
    let always: &[&str] = required.map_or(&[], |r| r.always);
    let present_only: &[PresentOnly] = required.map_or(&[], |r| r.present_only);
    let in_required = |p: &str| always.contains(&p) || present_only.iter().any(|f| f.decoded == p);

    // §5.3'S FLOOR, IN WHICHEVER VOCABULARY THIS RECORD IS WRITTEN IN.
    //
    //     A present-only field joins the required set exactly when it is
    //     present in the decoded source. Absent means nothing to assess;
    //     present means it is retained and must therefore be assessed like any
    //     other.
    //
    // The doc comment above once argued the whole of this was undeterminable,
    // on the grounds that a consumer holds the record and not the decoded
    // source. That is true of a field the record does NOT carry, and it is
    // still the reason the absent half is not demanded. It is not true of one
    // the record DOES carry, and the argument for the ceiling was being reused
    // as an argument against the floor. The consequence was durable rather than
    // incidental: `conclusion` on a check run is present on real evidence,
    // never in the `always` set, and was therefore a field the gate was
    // structurally never asked about while a decision read its value.
    //
    // WHAT COUNTS AS THE RECORD DECLARING PRESENCE, and both readings are the
    // record's own statement rather than an inference about upstream:
    //
    //     complete projection   §8 lists these members OPTIONAL-IF-PRESENT, so
    //                           carrying one says the decoded source had it.
    //                           §8 and §5.3 both make `null` the same input as
    //                           absent, so a null member says nothing.
    //     reduced record        §7.1 partitions the required set and puts every
    //                           field in exactly one half, so a pointer in
    //                           either half is the record saying that field was
    //                           in the set — and a pointer in neither says it
    //                           never was.
    //
    // Read here with `pointer` and `get` rather than through the ordered
    // partition below, because this is a presence question and the ordering
    // rules are §5.5's; running them early would move which refusal a malformed
    // record reports without changing that it is refused.
    let present_now: Vec<&'static str> = present_only
        .iter()
        .filter(|f| {
            if is_reduced {
                let named_in = |member: &str| {
                    record
                        .pointer(&format!("/{member}"))
                        .is_some_and(|half| match half {
                            Value::Array(items) => {
                                items.iter().any(|i| i.as_str() == Some(f.decoded))
                            }
                            Value::Object(fields) => fields.contains_key(f.decoded),
                            _ => false,
                        })
                };
                named_in("retainedFields") || named_in("blockedFields")
            } else {
                record
                    .pointer(&format!("/{}", f.canonical))
                    .is_some_and(|v| !v.is_null())
            }
        })
        .map(|f| f.decoded)
        .collect();

    let assessed = ordered_unique_at(assessment, "assessedFields")?;
    let is_assessed = |p: &str| assessed.iter().any(|a| a == p);

    // §5.2 + §9.4: THE DENOMINATOR IS A CEILING AS WELL AS A FLOOR.
    //
    // The coverage rule below is one-directional — every `always` field must be
    // assessed — and one direction is not the rule. §5.2 makes §5.3 the
    // normative field set for a kind, and §9.4's whole argument is that an
    // assessment is safe because every one of its fields is a closed vocabulary
    // value, a boolean, a structural identifier or a JSON pointer, so no field's
    // RANGE depends on the content being inspected. `assessedFields` is a list
    // of pointers, and a pointer is only a structural identifier while it comes
    // from a set fixed in advance. An entry chosen from the inspected content —
    //
    //     "/zz/ghp_the_credential"
    //
    // is syntactically a pointer, sorts wherever its author needs it to under
    // §5.5, passes every other rule, and rides into a permanently retained
    // closed-schema artifact. §9.4 closed the schema against exactly this and
    // then left the one member whose values are a list open.
    //
    // The minimum invariant, and deliberately only the minimum: every assessed
    // field is in `always ∪ present_only` for the record's kind. Nothing here
    // says which of the two, and nothing here decides whether a present-only
    // field WAS present — that is §7.1's job and the R2/R3 pair's.
    //
    // A kind with no §5.3 entry has an empty universe, so nothing can be
    // legitimately assessed about it. Unreachable for the gated kinds — all five
    // §8 projections have entries and `denominator_of` already refuses a reduced
    // record whose locator kind has none — and stated rather than special-cased,
    // because the alternative is a branch that admits everything when the
    // denominator is missing.
    if let Some(outside) = assessed.iter().find(|p| !in_required(p)) {
        return Err(format!(
            "/assessedFields names {outside:?}, which §5.3 does not give this kind. §5.2 \
             fixes the denominator and §9.4 keeps an assessment safe by making every field's \
             value range independent of the content inspected — a pointer chosen from that \
             content is neither"
        ));
    }
    let flagged: Vec<&str> = assessment
        .pointer("/findings")
        .and_then(Value::as_array)
        .map(|f| {
            f.iter()
                .filter_map(|x| x.pointer("/field").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let outcome = member_str_at(assessment, "outcome")?;

    // §9: every field a finding names MUST be in the §5.3 required set AND in
    // assessedFields. Not pedantry about well-formedness — a detector can only
    // find something in a field it actually looked at, and §9 spells out the
    // exfiltration this admits: a finding on /made-up-field blocks nothing, /body
    // stays assessed and unflagged and therefore RETAINED, and the outcome reads
    // BLOCK_SECRET because findings is non-empty. Every other rule holds and the
    // secret is retained anyway.
    // §9 also requires every field a finding names to be in the §5.3 required
    // set, and that rule is NOT restated here. It has no reachable failure once
    // §7.1's two partition rules below are in force, and the derivation is short
    // enough to check: findings exist only under BLOCK_SECRET (§9.6's
    // computation), which for a source record means a reduced record; for a
    // reduced record a flagged pointer is either absent from `blockedFields`, and
    // the "flagged is blocked, always" rule refuses it, or present in it, and the
    // "partition stays inside §5.3" rule refuses it for being outside the set.
    // There is no third case. Mutation testing is what established this — deleting
    // the restatement changed no verdict — and a check with no reachable failure
    // is not evidence of the rule it is named after.
    for field in &flagged {
        if !is_assessed(field) {
            return Err(format!(
                "a finding names {field:?}, which is not in /assessedFields. A detector can \
                 only find something in a field it actually looked at"
            ));
        }
    }

    // §5.2: the denominator is normative, not declared. `coverageComplete` is the
    // record's own claim — "I assessed everything" — and checking it against the
    // set the same producer supplied is the self-certification §5.2 names.
    if assessment.pointer("/coverageComplete") == Some(&Value::Bool(true)) {
        if let Some(missing) = always.iter().find(|p| !is_assessed(p)) {
            return Err(format!(
                "/coverageComplete is true and {missing:?} is not in /assessedFields. §5.2 \
                 fixes the denominator in §5.3 and forbids taking it from the same producer"
            ));
        }
        // The denominator is `always ∪ the present-only fields this record
        // declares present`, and the second half is why the two loops are not
        // one: the pointer alone does not say which rule refused it, and
        // "§5.3 puts a field you kept in the set" is a different sentence from
        // "you did not assess a field that is always required".
        if let Some(missing) = present_now.iter().find(|p| !is_assessed(p)) {
            return Err(format!(
                "/coverageComplete is true and {missing:?} is not in /assessedFields. §5.3 \
                 puts a present-only field in the required set exactly when it is present, \
                 and this record carries it — present means it is retained and must therefore \
                 be assessed like any other"
            ));
        }
    }

    if !is_reduced {
        // §6.2 and §6.3: under BLOCK_SECRET or CANNOT_ASSESS a normal source
        // snapshot of any §8 sourceKind is FORBIDDEN — it would require the bytes,
        // which is the thing being refused.
        if outcome != "RETAIN" {
            return Err(format!(
                "the authorising assessment's outcome is {outcome:?}, and §6.2/§6.3 forbid a \
                 normal §8 source snapshot under it. A conforming assessment that refuses this \
                 content is not a permission to keep it"
            ));
        }
        return Ok(());
    }

    // ---- The reduced record. §7.1's partition is COMPUTED, not nominated.

    // §9.6: the retained assessment is the authority on its own outcome, and
    // anything else is an expectation checked against it, never a substitute.
    // Without this the self-certification returns one layer out — a record
    // reading CANNOT_ASSESS bound to an assessment reading BLOCK_SECRET, every
    // structural check passing, and the retained bytes disagreeing with what was
    // actually done.
    // §9.5: THE POLICY VERSION IS PART OF THE AUTHORISATION, not decoration.
    //
    // "every one of them that carries `redactionPolicyVersion` carries the same
    // value as the assessment that authorised it." Without this an assessment
    // made under one policy authorises a record claiming another, and every
    // other relation holds while it happens: the binding is retained and names
    // this record, the outcomes agree, the partition computes. The retained
    // trail then says a gate ran and does not say WHICH gate — which is the
    // difference between provenance and a receipt.
    //
    // Checked for the reduced record and not for a complete projection because
    // §8's projections carry no `redactionPolicyVersion`; there is no second
    // value to disagree. Scoped to what the artifacts actually hold rather than
    // asserted over a member that does not exist.
    let record_policy = member_str_at(record, "redactionPolicyVersion")?;
    if record_policy != assessment_policy {
        return Err(format!(
            "§9.5: the record's /redactionPolicyVersion is {record_policy:?} and its \
             authorising assessment was made under {assessment_policy:?}; a version field \
             unrelated to its neighbours is decoration on bytes about to be hashed"
        ));
    }

    let record_outcome = member_str_at(record, "outcome")?;
    if record_outcome != outcome {
        return Err(format!(
            "§9.6: the record's own /outcome is {record_outcome:?} and its authorising \
             assessment says {outcome:?}"
        ));
    }
    if outcome == "RETAIN" {
        return Err(
            "§7: a reduced source record exists because the source did NOT pass the gate as a \
             whole, and its outcome is BLOCK_SECRET or CANNOT_ASSESS"
                .to_owned(),
        );
    }
    if record.pointer("/coverageComplete") != assessment.pointer("/coverageComplete") {
        return Err(
            "the record's /coverageComplete disagrees with its authorising assessment's; the \
             assessment is the retained authority and the copy is an expectation checked \
             against it"
                .to_owned(),
        );
    }

    let blocked = ordered_unique_at(record, "blockedFields")?;

    // §7.4'S FLOOR, AND IT IS A PROVENANCE RULE RATHER THAN A TIDINESS ONE:
    //
    //     `blockedFields` MUST be non-empty. A record that blocked nothing is
    //     not a reduced record — it is a complete projection, and should be one.
    //
    // §7 defines this kind as the artifact that exists BECAUSE the source did
    // not pass the gate as a whole. Without the floor a producer emits an
    // object wearing that provenance — refused-source semantics, a
    // `locatorKind` naming the kind that was refused, an outcome of
    // BLOCK_SECRET or CANNOT_ASSESS — and hands over every field it was
    // supposed to have withheld. Every partition rule below is then satisfied
    // vacuously: nothing is in both halves, nothing required is in neither once
    // `retainedFields` carries it all, and no finding names an unblocked field
    // because there are no findings.
    //
    // REACHABLE ONLY UNDER `CANNOT_ASSESS`, and the derivation is worth stating
    // because it says what this check is actually holding. §9.6 computes
    // BLOCK_SECRET from the presence of findings, and §7.1 blocks a finding's
    // field always, so an empty blocked half under that outcome was already
    // refused by the finding rule. The two rules are independent — this one
    // does not become unreachable if that one moves — and `correction_k2.rs`'s
    // K2-B pins the pairing from the other side.
    if blocked.is_empty() {
        return Err(
            "/blockedFields is empty. §7.4: a record that blocked nothing is not a reduced \
             record — it is a complete projection, and should be one; this one claims the \
             refused-source kind while withholding nothing"
                .to_owned(),
        );
    }

    let is_blocked = |p: &str| blocked.iter().any(|b| b == p);
    let retained: Vec<String> = record
        .pointer("/retainedFields")
        .and_then(Value::as_object)
        .ok_or_else(|| "/retainedFields is absent or not an object".to_owned())?
        .keys()
        .cloned()
        .collect();

    // §7.1: retainedFields and blockedFields EXHAUSTIVELY partition the required
    // set — every field in exactly one, and nothing in neither.
    for pointer in retained.iter().chain(&blocked) {
        if !in_required(pointer) {
            return Err(format!(
                "the partition carries {pointer:?}, which is not in the §5.3 required set for \
                 this record's locatorKind. §5.2: the denominator is normative, not declared"
            ));
        }
    }
    if let Some(both) = retained.iter().find(|p| is_blocked(p)) {
        return Err(format!(
            "{both:?} is in both /retainedFields and /blockedFields; §7.1 puts every field in \
             exactly one"
        ));
    }
    if let Some(neither) = always
        .iter()
        .find(|p| !is_blocked(p) && !retained.iter().any(|r| r == *p))
    {
        return Err(format!(
            "{neither:?} is in neither /retainedFields nor /blockedFields, so the record does \
             not account for it. §7.1's partition is exhaustive, and an incomplete one reads \
             as evidence that nothing was blocked"
        ));
    }

    // §7.1: a field named by a finding is blocked. Always.
    if let Some(kept) = flagged.iter().find(|f| !is_blocked(f)) {
        return Err(format!(
            "a finding names {kept:?} and the record did not block it. §7.1: a field named by \
             a finding is blocked, always"
        ));
    }
    // §7.1: a field the detector never successfully assessed is blocked. Always —
    // an unassessed field is not "probably a timestamp", it is unexamined.
    if let Some(unexamined) = retained.iter().find(|p| !is_assessed(p)) {
        return Err(format!(
            "the record retains {unexamined:?}, which is not in /assessedFields. §7.1: a field \
             the detector never successfully assessed is blocked, always"
        ));
    }
    // §7.1: AND THE OTHER DIRECTION, which is not a second rule but the rest of
    // the same one.
    //
    //     blockedFields = flagged ∪ (required \ assessed)
    //
    // is an EQUALITY, and the two rules above are its two inclusions read one
    // way round: every flagged field is blocked, every retained field was
    // assessed. Neither says a blocked field had a reason to be. §7.1 says it
    // directly — "Retention is not discretionary in the other direction either.
    // A field that survives the computation is retained, so the record cannot be
    // thinned by judgement after the fact" — and until this check existed a
    // producer could withhold a field the computation retained with every other
    // rule still holding: the halves disjoint, nothing required in neither, the
    // finding's own field blocked, every retained field assessed.
    //
    // WHY THAT IS PROVENANCE AND NOT TIDINESS. The gate's output is a decision's
    // input, so a producer free to thin the record chooses which evidenced
    // decisions stay makeable, and does so without lying: the object it emits
    // passes every check. §7.1 removes that freedom by making both halves
    // COMPUTED from the assessment. One unchecked direction hands it back.
    //
    // THE DOMAIN IS J1'S, not `always`. A present-only field the record declares
    // present joins the required set and is assessed like any other, so a
    // producer could otherwise suppress `conclusion` — present on real evidence,
    // and the field a check decision is about. The check ranges over `blocked`
    // itself rather than over a reconstructed denominator, so there is no half
    // of §5.3 for it to miss: whatever is in `blockedFields` must have earned
    // its place, and `in_required` above has already refused anything outside
    // the kind's set.
    if let Some(thinned) = blocked
        .iter()
        .find(|b| is_assessed(b) && !flagged.iter().any(|f| f == b))
    {
        return Err(format!(
            "the record blocks {thinned:?}, which /assessedFields carries and no finding names. \
             §7.1: `blockedFields = flagged ∪ (required \\ assessed)` — retention is not \
             discretionary in the other direction either, and a field that survives the \
             computation is retained"
        ));
    }

    Ok(())
}

// ---- The same two readers, over a validated artifact.
//
// Deliberately thin wrappers rather than a generic: the whole point of
// `ValidatedArtifact` is that it is NOT interchangeable with `Value`, and a
// trait letting both satisfy one bound would hand that back.

fn member_str_at<'a>(
    artifact: &'a crate::artifact::ValidatedArtifact,
    name: &str,
) -> Result<&'a str, String> {
    artifact
        .str_at(&format!("/{name}"))
        .ok_or_else(|| format!("/{name} is not a string"))
}

fn ordered_unique_at(
    artifact: &crate::artifact::ValidatedArtifact,
    name: &str,
) -> Result<Vec<String>, String> {
    let items = artifact
        .pointer(&format!("/{name}"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("/{name} is not an array"))?;
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let text = item
            .as_str()
            .ok_or_else(|| format!("/{name}[{i}] is not a string"))?;
        if let Some(previous) = out.last() {
            if previous.as_str() >= text {
                return Err(format!(
                    "§5.5: /{name} must be unique and in ascending lexical order; \
                     {previous:?} is followed by {text:?}"
                ));
            }
        }
        out.push(text.to_owned());
    }
    Ok(out)
}
