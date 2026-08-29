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

use o7_closure_matcher::{Member, ValueKind, QUERY_SNAPSHOT_SCHEMAS, SOURCE_SCHEMAS};
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

// ---- §13's query snapshot, against the same document.

/// One member as §13's block names it: a full dotted path, plus the version
/// annotation if the document attached one.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ContractMember {
    path: String,
    required: bool,
    /// `Some(2)` for the member the contract marks `(schemaVersion 2 only)`.
    ///
    /// Parsed rather than hardcoded. Writing "version 2 adds implementationDigest"
    /// into this test would make the test agree with the code because both were
    /// written from the same belief, which is the failure mode the whole file
    /// exists to prevent.
    only_at_version: Option<i64>,
}

/// §13's block, read as dotted paths.
///
/// A different parse from [`contract_members`] on purpose: §8's shapes nest one
/// level and the root name is enough there, while §13 names `binding.repository`
/// REQUIRED and `binding.sha` OPTIONAL-IF-PRESENT, so collapsing to root names
/// would put `binding` in both sets and compare nothing.
fn contract_query_snapshot_members() -> Vec<ContractMember> {
    let doc = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/architecture/closure-source-provenance-v1.md"),
    )
    .expect("reading the provenance contract");

    let after = doc
        .split_once("`sourceKind: github-query-snapshot`")
        .expect("the contract defines the query snapshot")
        .1;
    let block = after
        .split_once("```text")
        .expect("a fenced block follows the kind")
        .1
        .split_once("```")
        .expect("the fenced block closes")
        .0;

    let mut members = Vec::new();
    let mut required = true;
    for line in block.lines() {
        let rest = match line.trim_start().strip_prefix("OPTIONAL-IF-PRESENT") {
            Some(rest) => {
                required = false;
                rest
            }
            None => match line.trim_start().strip_prefix("REQUIRED") {
                Some(rest) => {
                    required = true;
                    rest
                }
                None => line,
            },
        };

        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let split = tokens
            .iter()
            .position(|t| t.starts_with('('))
            .unwrap_or(tokens.len());
        let (names, annotation) = tokens.split_at(split);
        let only_at_version = version_annotation(&annotation.join(" "));

        for name in names {
            members.push(ContractMember {
                path: (*name).to_owned(),
                required,
                only_at_version,
            });
        }
    }
    assert!(
        members.len() > 10,
        "§13's block parsed to {} members, which is too few to be the block — the \
         parser and the document have diverged",
        members.len()
    );
    members
}

/// `(schemaVersion 2 only)` -> `Some(2)`.
fn version_annotation(annotation: &str) -> Option<i64> {
    let inner = annotation
        .trim()
        .strip_prefix("(schemaVersion ")?
        .strip_suffix(" only)")?;
    Some(
        inner
            .parse()
            .expect("a version annotation names an integer"),
    )
}

/// Every leaf of a declared shape, as a dotted path. Containers contribute their
/// leaves and not themselves, because the contract names `binding.repository` and
/// never a bare `binding`.
fn declared_paths(members: &[Member], prefix: &str, into: &mut Vec<(String, bool)>) {
    for member in members {
        let path = if prefix.is_empty() {
            member.name.to_owned()
        } else {
            format!("{prefix}.{}", member.name)
        };
        match member.kind {
            ValueKind::Object(nested) => declared_paths(nested, &path, into),
            _ => into.push((path, member.required)),
        }
    }
}

/// The registered query-snapshot shapes are the ones §13 states, at each version.
#[test]
fn every_query_snapshot_schema_is_the_one_the_contract_states() {
    let contract = contract_query_snapshot_members();
    assert!(
        !QUERY_SNAPSHOT_SCHEMAS.is_empty(),
        "no query-snapshot shape is registered, so from_canonical would refuse everything"
    );

    for schema in QUERY_SNAPSHOT_SCHEMAS {
        let version = schema.schema_version;

        // A member annotated `(schemaVersion N only)` belongs to version N and to
        // no other. That is what makes the two shapes closed rather than one
        // shape with an optional field.
        let want_required: BTreeSet<&str> = contract
            .iter()
            .filter(|m| m.required && m.only_at_version.is_none_or(|v| v == version))
            .map(|m| m.path.as_str())
            .collect();
        let want_optional: BTreeSet<&str> = contract
            .iter()
            .filter(|m| !m.required && m.only_at_version.is_none_or(|v| v == version))
            .map(|m| m.path.as_str())
            .collect();

        let mut declared = Vec::new();
        declared_paths(schema.members, "", &mut declared);
        let got_required: BTreeSet<&str> = declared
            .iter()
            .filter(|(_, required)| *required)
            .map(|(path, _)| path.as_str())
            .collect();
        let got_optional: BTreeSet<&str> = declared
            .iter()
            .filter(|(_, required)| !*required)
            .map(|(path, _)| path.as_str())
            .collect();

        assert_eq!(
            got_required, want_required,
            "github-query-snapshot v{version}: REQUIRED members disagree with §13"
        );
        assert_eq!(
            got_optional, want_optional,
            "github-query-snapshot v{version}: OPTIONAL-IF-PRESENT members disagree with §13"
        );
    }
}

/// The contract's version annotation is load-bearing, and the two shapes really
/// do differ by exactly it.
///
/// Without this, both tables could carry `matcher.implementationDigest` — or
/// neither could — and the parity check above would still pass against a
/// contract whose annotation this test had quietly stopped reading.
#[test]
fn the_two_versions_differ_by_exactly_the_annotated_member() {
    let contract = contract_query_snapshot_members();
    let annotated: Vec<&ContractMember> = contract
        .iter()
        .filter(|m| m.only_at_version.is_some())
        .collect();
    assert_eq!(
        annotated.len(),
        1,
        "§13 annotates exactly one member with a version; found {annotated:?}"
    );

    let mut paths = Vec::new();
    for schema in QUERY_SNAPSHOT_SCHEMAS {
        let mut declared = Vec::new();
        declared_paths(schema.members, "", &mut declared);
        paths.push((
            schema.schema_version,
            declared
                .into_iter()
                .map(|(path, _)| path)
                .collect::<BTreeSet<String>>(),
        ));
    }
    let [(v1, first), (v2, second)] = paths.as_slice() else {
        panic!(
            "§13 defines two versions; the registry holds {}",
            paths.len()
        );
    };

    let [annotated] = annotated.as_slice() else {
        panic!("asserted exactly one above");
    };
    let annotated_path = &annotated.path;
    let annotated_version = annotated.only_at_version.expect("filtered on Some");
    let (with, without) = if annotated_version == *v2 {
        (second, first)
    } else {
        (first, second)
    };
    let difference: Vec<&String> = with.difference(without).collect();
    assert_eq!(
        difference,
        vec![annotated_path],
        "v{v1} and v{v2} must differ by exactly {annotated_path}, the member §13 \
         annotates; a shape that borrows anything else from its neighbour is not closed"
    );
}

/// The admissible `enumeration` values are the contract's, not this crate's.
///
/// This is the member the P1 turned on, and the one place where a value set —
/// not merely a key set — decides admissibility. Writing that set into the code
/// alone would make the code the authority on what §13 permits, which is the
/// shape of defect this whole file exists to prevent; so §13 states the set and
/// the registered shape is checked against it.
#[test]
fn the_enumeration_states_are_the_ones_the_contract_states() {
    let doc = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/architecture/closure-source-provenance-v1.md"),
    )
    .expect("reading the provenance contract");

    let line = doc
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("enumeration states"))
        .expect("§13 names the closed set of enumeration states");
    let want: BTreeSet<&str> = line.split_whitespace().collect();
    assert!(
        want.len() >= 2,
        "a one-state enumeration cannot distinguish anything; §13 parsed to {want:?}"
    );

    let mut checked = 0;
    for schema in QUERY_SNAPSHOT_SCHEMAS {
        let member = schema
            .members
            .iter()
            .find(|m| m.name == "enumeration")
            .expect("enumeration is a member of every version");
        let ValueKind::OneOf(states) = member.kind else {
            panic!(
                "enumeration must be a closed value set, not {:?} — a type check admits \
                 every string, and admissibility turns on the value",
                member.kind
            );
        };
        assert_eq!(
            states.iter().copied().collect::<BTreeSet<&str>>(),
            want,
            "github-query-snapshot v{}: the enumeration states disagree with §13",
            schema.schema_version
        );
        checked += 1;
    }
    assert_eq!(checked, QUERY_SNAPSHOT_SCHEMAS.len());
}
