//! The declared candidate schemas are read back out of the contract.
//!
//! The defect this exists to prevent already happened once: §13.1 said a
//! candidate omitting "a field that kind requires" is inadmissible, the code
//! checked one field per kind, and both were written in the same push. Nothing
//! was checking that the document and the table agreed, so nothing noticed.
//!
//! So the expectation here is not a second copy of the key set. It is §8.2 and
//! §8.3 of `closure-source-provenance-v1.md`, parsed out of the REQUIRED and
//! OPTIONAL-IF-PRESENT blocks the document already had before this crate existed.
//! If someone edits the schema table without editing the contract, or the
//! contract without the table, this fails and names the difference.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own reading of
// the contract document that ships in this repository. A contract this cannot
// parse is a defect to report loudly, not one to skip past.

use o7_closure_matcher::{Member, ValueKind, SOURCE_SCHEMAS};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Pull the REQUIRED and OPTIONAL-IF-PRESENT names out of the fenced block that
/// follows `sourceKind: <kind>` in the contract.
fn contract_members(kind: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let doc = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/architecture/closure-source-provenance-v1.md"),
    )
    .expect("reading the provenance contract");

    let anchor = format!("`sourceKind: {kind}`");
    let after = doc
        .split_once(&anchor)
        .unwrap_or_else(|| panic!("the contract does not define {kind}"))
        .1;
    let block = after
        .split_once("```text")
        .expect("a fenced block follows the kind")
        .1
        .split_once("```")
        .expect("the fenced block closes")
        .0;

    let (mut required, mut optional) = (BTreeSet::new(), BTreeSet::new());
    let mut in_optional = false;
    for line in block.lines() {
        let rest = match line.trim_start().strip_prefix("OPTIONAL-IF-PRESENT") {
            Some(rest) => {
                in_optional = true;
                rest
            }
            None => match line.trim_start().strip_prefix("REQUIRED") {
                Some(rest) => {
                    in_optional = false;
                    rest
                }
                None => line,
            },
        };
        for token in rest.split_whitespace() {
            if token == "(none)" {
                continue;
            }
            // `user.id` etc. are members of the nested object; keep the root name.
            let name = token
                .split('.')
                .next()
                .expect("a token has a first segment");
            if in_optional {
                optional.insert(name.to_owned());
            } else {
                required.insert(name.to_owned());
            }
        }
    }
    (required, optional)
}

fn declared(members: &[Member]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut required = BTreeSet::new();
    let mut optional = BTreeSet::new();
    for member in members {
        if member.required {
            required.insert(member.name.to_owned());
        } else {
            optional.insert(member.name.to_owned());
        }
    }
    (required, optional)
}

#[test]
fn every_declared_schema_is_the_one_the_contract_states() {
    let mut checked = 0;
    for schema in SOURCE_SCHEMAS {
        let kind = schema.source_kind;
        let (want_required, want_optional) = contract_members(kind);
        let (got_required, got_optional) = declared(schema.members);

        assert_eq!(
            got_required, want_required,
            "{kind}: REQUIRED members disagree with the contract's §8 block"
        );
        assert_eq!(
            got_optional, want_optional,
            "{kind}: OPTIONAL-IF-PRESENT members disagree with the contract's §8 block"
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "expected both registered kinds, saw {checked}"
    );
}

/// The nested members the contract writes as `user.id` are actually checked,
/// rather than collapsing into a bare `user` that any object would satisfy.
#[test]
fn the_dotted_members_the_contract_names_are_checked_as_a_nested_shape() {
    let schema = SOURCE_SCHEMAS
        .iter()
        .find(|s| s.source_kind == "github-submitted-review")
        .expect("the review shape is registered");
    let user = schema
        .members
        .iter()
        .find(|m| m.name == "user")
        .expect("user is a member");
    let ValueKind::Object(nested) = user.kind else {
        panic!("user must be a nested shape, not a scalar");
    };
    let names: BTreeSet<&str> = nested.iter().map(|m| m.name).collect();
    assert_eq!(
        names,
        BTreeSet::from(["id", "login", "type"]),
        "§8.2 writes user.id, user.login and user.type"
    );
    assert!(nested.iter().all(|m| m.required));
}
