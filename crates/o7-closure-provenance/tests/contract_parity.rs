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
// `panic!` sites below are this test's own parse failures. A
// contract section this parser cannot read must fail loudly — silently yielding
// an empty expected set would make every assertion below vacuously true, which
// is the failure-to-empty-set-to-green demon the whole crate is about.
// Extent (checked by N1): 17 `panic` sites.
#![allow(clippy::panic)]

use o7_closure_provenance::artifact::{
    locator_shape, HEAD_READ_EVENT_AVAILABLE, HEAD_READ_EVENT_FAILED, LOCATOR_SHAPES,
    REDUCED_RECORD, RETENTION_BINDING,
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
        let decoded: Vec<&str> = table.present_only.iter().map(|f| f.decoded).collect();
        assert_eq!(
            decoded.as_slice(),
            present_only
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
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

/// §8's `OPTIONAL-IF-PRESENT` members for one `sourceKind`, read out of the
/// provenance contract.
///
/// §8's projection blocks name the same fields §5.3's present-only sets do, in
/// the canonical spelling. Two documents, one rule — which is exactly the shape
/// that drifts.
fn contract_optional_members(source_kind: &str) -> Vec<String> {
    let needle = format!("`sourceKind: {source_kind}`");
    let at = PROVENANCE
        .find(&needle)
        .unwrap_or_else(|| panic!("§8 no longer declares a projection for {source_kind:?}"));
    let rest = &PROVENANCE[at..];
    let open = rest
        .find("```text")
        .unwrap_or_else(|| panic!("no fenced block follows {needle:?}"));
    let body = &rest[open..];
    let close = body
        .find("```\n")
        .unwrap_or_else(|| panic!("unterminated fenced block after {needle:?}"));
    let block = &body[..close];
    let marker = block.find("OPTIONAL-IF-PRESENT").unwrap_or_else(|| {
        panic!("§8's block for {source_kind:?} names no OPTIONAL-IF-PRESENT row at all")
    });
    let tail = &block[marker + "OPTIONAL-IF-PRESENT".len()..];
    tail.lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .filter(|t| *t != "(none)")
        .map(str::to_owned)
        .collect()
}

/// The canonical half of every present-only entry is §8's, and §8's
/// `OPTIONAL-IF-PRESENT` row is exactly §5.3's present-only set.
///
/// WHY THIS EXISTS AND NOT A CAMEL-CASE FUNCTION. The two spellings look like a
/// transformation of one another, and a transformation would agree with itself
/// whatever the documents said. §5.3 fixes WHEN the field joins the required
/// set; §8 fixes what a complete projection calls it, which is the only place a
/// consumer can see that it is present. If the two documents ever disagree
/// about which fields are optional, the coverage rule in `check_authorises` is
/// ranging over a set neither one states.
#[test]
fn every_present_only_field_is_the_one_both_contracts_state() {
    for entry in REQUIRED_FIELDS {
        let canonical: Vec<&str> = entry.present_only.iter().map(|f| f.canonical).collect();
        let optional = contract_optional_members(entry.locator_kind);
        assert_eq!(
            canonical.as_slice(),
            optional
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>()
                .as_slice(),
            "the canonical present-only members for {:?} are not §8's OPTIONAL-IF-PRESENT row. \
             §5.3 says when a present-only field joins the required set and §8 says what a \
             complete projection calls it; a coverage rule ranging over a set the two \
             documents do not agree on is ranging over neither",
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

/// §9.2's `RetentionBinding` block.
///
/// The kind D1 was about. It is declared in full by §9.2 and again by §9.5, and
/// for three rounds it appeared in this crate exactly once — in a doc comment —
/// while `authorised` read two members straight out of whatever bytes a store
/// handed over. A table nobody checks against the document is how that stays
/// true after somebody adds the table.
#[test]
fn the_retention_binding_shape_is_the_one_the_contract_states() {
    let block = block_starting("Binding is a **separate retained object**");
    let contract = members_in(&block, &["RetentionBinding", "REQUIRED"]);
    assert_eq!(
        contract,
        [
            "schemaVersion",
            "sourceKind",
            "recordDigest",
            "assessmentDigest"
        ],
        "§9.2's RetentionBinding block is not the four members it has always had"
    );
    let table: Vec<&str> = RETENTION_BINDING.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        contract.iter().map(String::as_str).collect::<Vec<_>>(),
        "§9.2's members, in order, are not the table's"
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
/// THIS TEST USED TO ENCODE A READING, and it no longer has to. §8.1's blocks
/// named `role`, `acquisition`, `observedAt` and the member distinguishing the
/// two variants, and did not repeat `schemaVersion` and `sourceKind`; those were
/// supplied here on §7's authority — every canonical object is domain-separated
/// by its own content — and the doc comment recorded that as a reading so it
/// could be revisited rather than silently outlive the document.
///
/// It was revisited. §8.1 now lists both members in both blocks and declares
/// `sourceKind: github-head-read-event` outright, so nothing is added on this
/// side and this is a plain transcription check again. If a later revision drops
/// either member, the table stops matching the document instead of matching an
/// assumption about it.
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

    let expect_available = members_in(available, &["HeadReadEvent, acquisition = AVAILABLE"]);
    let table: Vec<&str> = HEAD_READ_EVENT_AVAILABLE.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        expect_available
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        "the AVAILABLE event shape differs from §8.1"
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

    let expect_failed: Vec<String> = members_in(failed, &[])
        .into_iter()
        .filter(|m| !absent.contains(&m.as_str()))
        .collect();
    let table: Vec<&str> = HEAD_READ_EVENT_FAILED.iter().map(|m| m.name).collect();
    assert_eq!(
        table,
        expect_failed.iter().map(String::as_str).collect::<Vec<_>>(),
        "the FAILED event shape differs from §8.1"
    );
}
