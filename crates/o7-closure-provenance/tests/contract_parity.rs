//! The tables in `redaction.rs` are the contract's, or the build fails.
//!
//! Two transcriptions of one rule is one rule too many. The §9 member list and
//! the §5.3 required field sets are load-bearing — §5.2 makes the denominator
//! normative precisely so a consumer cannot take it from the producer, and a
//! consumer that takes it from a stale copy of the document has reintroduced the
//! same problem with a slower clock.
//!
//! So the expectation is the markdown, parsed. If somebody corrects §5.3 and not
//! the table, this fails; if somebody "fixes" the table to match code that
//! drifted, this fails too. Neither is reachable by editing one file.
//!
//! WHAT THIS DOES NOT PROVE. That the checks in `redaction.rs` are the right
//! checks — only that the sets they range over are the contract's sets. The law
//! is carried by the semantic witnesses; this is a transcription guard, and
//! documenting it as more than that is the E2 defect of the previous round.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `panic!` and `expect` sites below are this test's own parse failures. A
// contract section this parser cannot read must fail loudly — silently yielding
// an empty expected set would make every assertion below vacuously true, which
// is the failure-to-empty-set-to-green demon the whole crate is about.
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_provenance::artifact::{
    locator_shape, HEAD_READ_EVENT_AVAILABLE, HEAD_READ_EVENT_FAILED, LOCATOR_SHAPES,
    REDUCED_RECORD,
};
use o7_closure_provenance::redaction::{
    required_fields, MemberKind, ASSESSMENT_CONDITIONAL, ASSESSMENT_REQUIRED, REQUIRED_FIELDS,
};

const CONTRACT: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

/// The fenced block that follows a heading, by its first line.
fn block_starting(needle: &str) -> String {
    let at = CONTRACT
        .find(needle)
        .unwrap_or_else(|| panic!("the contract no longer contains {needle:?}"));
    let rest = &CONTRACT[at..];
    let open = rest
        .find("```text")
        .unwrap_or_else(|| panic!("no fenced block follows {needle:?}"));
    let body = &rest[at_end(rest, open)..];
    let close = body
        .find("```")
        .unwrap_or_else(|| panic!("unterminated fenced block after {needle:?}"));
    body[..close].to_owned()
}

fn at_end(text: &str, open: usize) -> usize {
    let after = open + "```text".len();
    // Skip to the end of the fence's own line.
    text[after..]
        .find('\n')
        .map_or(text.len(), |n| after + n + 1)
}

/// §5.3's per-kind `always` and `present-only` sets, read out of the contract.
fn contract_required_fields() -> Vec<(String, Vec<String>, Vec<String>)> {
    let block = block_starting("### 5.3 Required field set per source kind");
    let mut out: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let mut bucket: Option<bool> = None; // true = always, false = present-only

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            out.push((trimmed.to_owned(), Vec::new(), Vec::new()));
            bucket = None;
            continue;
        }
        let rest = if let Some(r) = trimmed.strip_prefix("always") {
            bucket = Some(true);
            r
        } else if let Some(r) = trimmed.strip_prefix("present-only") {
            bucket = Some(false);
            r
        } else {
            trimmed
        };
        let Some((_, always, present_only)) = out.last_mut() else {
            panic!("§5.3 lists pointers before naming a source kind");
        };
        for token in rest.split_whitespace() {
            if token == "(none)" {
                continue;
            }
            assert!(
                token.starts_with('/'),
                "§5.3 should list JSON pointers; found {token:?}"
            );
            match bucket {
                Some(true) => always.push(token.to_owned()),
                Some(false) => present_only.push(token.to_owned()),
                None => panic!("§5.3 lists {token:?} under no heading"),
            }
        }
    }
    out
}

#[test]
fn every_required_field_set_is_the_one_the_contract_states() {
    let contract = contract_required_fields();
    assert!(
        contract.len() >= 5,
        "§5.3 parsed to {} kinds, which is fewer than the contract has ever had — the parser \
         has stopped reading the document rather than the document having shrunk",
        contract.len()
    );

    for (kind, always, present_only) in &contract {
        let Some(table) = required_fields(kind) else {
            panic!("§5.3 defines a required set for {kind:?} and the table has no entry for it");
        };
        assert_eq!(
            table.always,
            always.as_slice(),
            "the `always` set for {kind:?} differs from §5.3"
        );
        assert_eq!(
            table.present_only,
            present_only.as_slice(),
            "the `present-only` set for {kind:?} differs from §5.3"
        );
    }

    let named: Vec<&str> = contract.iter().map(|(k, _, _)| k.as_str()).collect();
    for entry in REQUIRED_FIELDS {
        assert!(
            named.contains(&entry.locator_kind),
            "the table carries {:?}, which §5.3 does not define. An extra denominator is not a \
             harmless spare: §5.2 makes this set normative, and a kind the contract never gated \
             would be admitted with a required set nobody agreed to",
            entry.locator_kind
        );
    }
}

/// §9's `RetentionAssessment` block, split into its REQUIRED and CONDITIONAL
/// halves.
fn contract_assessment_members() -> (Vec<String>, Vec<String>) {
    let block = block_starting("## 9. Detector provenance");
    let (mut required, mut conditional) = (Vec::new(), Vec::new());
    let mut in_conditional = false;

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "RetentionAssessment" {
            continue;
        }
        let rest = if let Some(r) = trimmed.strip_prefix("REQUIRED") {
            in_conditional = false;
            r
        } else if let Some(r) = trimmed.strip_prefix("CONDITIONAL") {
            in_conditional = true;
            r
        } else {
            trimmed
        };
        // The member is the first token; anything after it is the contract's
        // gloss on its admissible values, which the Member table carries as a
        // `OneOf` rather than as a name.
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        // `detector.id` and friends are one nested object in the table.
        let name = name.split_once('.').map_or(name, |(head, _)| head);
        let into = if in_conditional {
            &mut conditional
        } else {
            &mut required
        };
        if !into.iter().any(|m: &String| m == name) {
            into.push(name.to_owned());
        }
    }
    (required, conditional)
}

#[test]
fn the_assessment_shape_is_the_one_the_contract_states() {
    let (required, conditional) = contract_assessment_members();

    let table: Vec<&str> = ASSESSMENT_REQUIRED.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        required.iter().map(String::as_str).collect::<Vec<_>>(),
        "§9's REQUIRED members, in order, are not the table's"
    );

    let table: Vec<&str> = ASSESSMENT_CONDITIONAL.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        conditional.iter().map(String::as_str).collect::<Vec<_>>(),
        "§9's CONDITIONAL members are not the table's"
    );
}

/// §9.3's closed vocabularies are values, not member names, so they are checked
/// separately — and they are exactly where an unnoticed addition would be worst:
/// a fourth outcome silently admitted is a gate decision nobody wrote a rule for.
#[test]
fn the_closed_vocabularies_are_the_ones_the_contract_states() {
    let block = block_starting("### 9.3 Closed vocabularies");

    let expected = |name: &str| -> Vec<String> {
        let line = block
            .lines()
            .find(|l| l.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("§9.3 no longer defines {name}"));
        let mut values: Vec<String> = line
            .trim()
            .strip_prefix(name)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        // The vocabulary may wrap onto continuation lines, which carry only
        // values.
        let lines = block
            .lines()
            .skip_while(|l| !l.trim_start().starts_with(name))
            .skip(1);
        for next in lines {
            let t = next.trim();
            if t.is_empty() || !next.starts_with(' ') {
                break;
            }
            if t.split_whitespace()
                .all(|v| v.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
            {
                values.extend(t.split_whitespace().map(str::to_owned));
            } else {
                break;
            }
        }
        values
    };

    let table = |name: &str, members: &[o7_closure_provenance::redaction::Member]| -> Vec<String> {
        members
            .iter()
            .find(|m| m.name == name)
            .map(|m| match m.kind {
                MemberKind::OneOf(values) => values.iter().map(|v| (*v).to_owned()).collect(),
                _ => panic!("{name} is not a closed vocabulary in the table"),
            })
            .unwrap_or_else(|| panic!("the table has no {name}"))
    };

    assert_eq!(
        table("outcome", ASSESSMENT_REQUIRED),
        expected("outcome"),
        "§9.3's outcome vocabulary is not the table's"
    );
    assert_eq!(
        table("coverageFailureCode", ASSESSMENT_CONDITIONAL),
        expected("coverageFailureCode"),
        "§9.3's coverageFailureCode vocabulary is not the table's"
    );
    assert_eq!(
        table("representation", ASSESSMENT_REQUIRED),
        expected("representation"),
        "§9.3's representation vocabulary is not the table's"
    );
}

// ---- The three forms Slice A has no reason to define.
//
// The five §8 projections and the §13 query snapshot are NOT re-checked here:
// they are `o7-closure-matcher`'s tables, checked against the contract by that
// crate's `schema_parity.rs`, and `artifact.rs` reads them rather than copying
// them. Adding a second assertion over the same table here would create the
// second truth this file exists to prevent — one more place to update, and one
// more place to forget.

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

/// Members named in a contract block: the first whitespace-run-delimited field
/// of each line, when it reads as an identifier. Everything after it is the
/// document's gloss on the member, not another member.
fn members_in(block: &str, strip: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in block.lines() {
        let mut rest = line.trim();
        for prefix in strip {
            if let Some(r) = rest.strip_prefix(prefix) {
                rest = r.trim();
            }
        }
        if rest.is_empty() {
            continue;
        }
        for field in rest.split("  ") {
            let field = field.trim();
            if field.is_empty() {
                continue;
            }
            // An identifier is a member; anything else is prose.
            if field.chars().all(|c| c.is_ascii_alphanumeric())
                && field.chars().next().is_some_and(char::is_lowercase)
            {
                if !out.iter().any(|m| m == field) {
                    out.push(field.to_owned());
                }
            } else {
                break;
            }
        }
    }
    out
}

#[test]
fn the_reduced_record_shape_is_the_one_the_contract_states() {
    let block = block_starting("## 7. The reduced source record");
    let contract = members_in(
        &block,
        &[
            "sourceKind           github-reduced-source-record",
            "REQUIRED",
        ],
    );
    assert!(
        contract.len() >= 9,
        "§7 parsed to {} members, fewer than the contract has ever had: {contract:?}",
        contract.len()
    );
    let table: Vec<&str> = REDUCED_RECORD.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        contract.iter().map(String::as_str).collect::<Vec<_>>(),
        "§7's REQUIRED members, in order, are not the table's"
    );
}

#[test]
fn every_locator_shape_is_the_one_the_contract_states() {
    let block = block_starting("### 7.3 The locator is identity, not surviving evidence");
    let mut rows: Vec<(String, Vec<String>)> = Vec::new();
    for line in block.lines() {
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else { continue };
        if !kind.starts_with("github-") {
            continue;
        }
        rows.push((kind.to_owned(), fields.map(str::to_owned).collect()));
    }
    assert_eq!(
        rows.len(),
        5,
        "§7.3 parsed to {} locator rows; the contract defines five gated kinds",
        rows.len()
    );

    for (kind, members) in &rows {
        let Some(shape) = locator_shape(kind) else {
            panic!("§7.3 defines a locator for {kind:?} and the table has no entry");
        };
        let table: Vec<&str> = shape.members.iter().map(|m| m.name).collect();
        assert_eq!(
            table,
            members.iter().map(String::as_str).collect::<Vec<_>>(),
            "the locator shape for {kind:?} differs from §7.3"
        );
    }

    let named: Vec<&str> = rows.iter().map(|(k, _)| k.as_str()).collect();
    for shape in LOCATOR_SHAPES {
        assert!(
            named.contains(&shape.locator_kind),
            "the table carries a locator for {:?}, which §7.3 does not define",
            shape.locator_kind
        );
    }
}

/// §8.1's two `HeadReadEvent` shapes.
///
/// THE ONE READING THIS TEST ENCODES, stated so it is checkable rather than
/// assumed: §8.1's blocks name `role`, `acquisition`, `observedAt` and the
/// member that distinguishes the two variants, and do NOT repeat `schemaVersion`
/// and `sourceKind`. Those are required anyway by provenance V1 §7, which
/// obliges every canonical object to be domain-separated by its own content —
/// the reading §9 states outright for the assessment, applied to the object §8.1
/// defines. If a later revision of §8.1 lists them, this test keeps passing; if
/// it says they are absent, this test fails and the reading gets revisited
/// rather than silently outliving the document.
#[test]
fn the_head_read_event_shapes_are_the_ones_the_contract_states() {
    let block = {
        let at = PROVENANCE
            .find("HeadReadEvent, acquisition = AVAILABLE")
            .unwrap_or_else(|| panic!("§8.1 no longer defines HeadReadEvent"));
        let rest = &PROVENANCE[at..];
        let close = rest
            .find("```")
            .unwrap_or_else(|| panic!("unterminated §8.1 event block"));
        rest[..close].to_owned()
    };
    let (available, failed) = block
        .split_once("HeadReadEvent, acquisition = FAILED")
        .unwrap_or_else(|| panic!("§8.1 no longer defines the FAILED variant"));

    let universal = ["schemaVersion", "sourceKind"];

    let mut expect_available: Vec<String> = universal.iter().map(|m| (*m).to_owned()).collect();
    expect_available.extend(members_in(
        available,
        &["HeadReadEvent, acquisition = AVAILABLE"],
    ));
    let table: Vec<&str> = HEAD_READ_EVENT_AVAILABLE.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        expect_available
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "the AVAILABLE event shape differs from §8.1 plus §7's universal members"
    );

    // MUST BE ABSENT is a rule about the key set, and a closed key set is how it
    // is enforced: the member simply is not in the table.
    let absent: Vec<&str> = failed
        .lines()
        .filter(|l| l.contains("MUST BE ABSENT"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(
        absent,
        ["snapshotDigest"],
        "§8.1's FAILED variant no longer excludes exactly snapshotDigest"
    );

    let mut expect_failed: Vec<String> = universal.iter().map(|m| (*m).to_owned()).collect();
    expect_failed.extend(
        members_in(failed, &[])
            .into_iter()
            .filter(|m| !absent.contains(&m.as_str())),
    );
    let table: Vec<&str> = HEAD_READ_EVENT_FAILED.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        expect_failed.iter().map(String::as_str).collect::<Vec<_>>(),
        "the FAILED event shape differs from §8.1 plus §7's universal members"
    );
}
