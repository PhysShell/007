//! `review-by-expected-author-login` version `1`.
//!
//! THIS FILE IS FROZEN. Its bytes ARE the identity of version `1`: the registry
//! embeds this file verbatim at compile time and pins its SHA-256. Any edit,
//! including a reformat or a comment, changes that digest and fails the binding.
//! A behaviour change is made by adding `..._v2.rs` and a new registry entry,
//! never by editing this file.
//!
//! (This header names no macro, because the lint in `tests/ambient_state.rs`
//! reads these bytes as text and cannot tell a doc comment from code.)
//!
//! THE RULE. A candidate matches when it *is* a submitted review and its author
//! login equals `parameters.expectedAuthorLogin`.
//!
//! WHAT IT DELIBERATELY DOES NOT LOOK AT. Not the review's `state`, not its
//! `commitId`. A matcher makes the objects a required observation is about
//! visible; the classifier decides whether any of them is admissible. A matcher
//! that dropped `COMMENTED` reviews or wrong-SHA reviews would leave an empty
//! matched subsequence, which provenance V1 §13 permits to mean `NotProduced` —
//! turning "reviewed, no verdict" and "reviewed the wrong commit" into "nobody
//! reviewed". That collapse is precisely what §13 exists to prevent, and the
//! RED-2 commit (782cbaf) is the record of it happening.
//!
//! WHY `sourceKind` AND NOT ONLY THE AUTHOR. The delivery-surface law recorded on
//! issue #147: a positive verdict delivered in an issue comment is not weak
//! review evidence, it is not review evidence at all, because an issue comment
//! has no field that can carry a subject binding. Classification follows the API
//! object's shape, never the author's identity.
//!
//! SELF-CONTAINED BY OBLIGATION. This file may import only `serde_json::Value`
//! and `crate::MatchError`, and defines exactly one function. A helper shared
//! with another matcher would be implementation this file's digest does not
//! cover — the hole RED-2 demonstrated, reopened one level down.

use serde_json::Value;

use crate::MatchError;

pub(crate) fn matches(candidate: &Value, parameters: &Value) -> Result<bool, MatchError> {
    let expected = parameters
        .get("expectedAuthorLogin")
        .and_then(Value::as_str)
        .ok_or(MatchError::MalformedCandidate {
            why: "expectedAuthorLogin is not a string",
        })?;
    if candidate.pointer("/sourceKind").and_then(Value::as_str) != Some("github-submitted-review") {
        return Ok(false);
    }
    Ok(candidate.pointer("/user/login").and_then(Value::as_str) == Some(expected))
}
