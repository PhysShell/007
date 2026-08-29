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
