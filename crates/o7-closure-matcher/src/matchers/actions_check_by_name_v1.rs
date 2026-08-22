//! `actions-check-by-name` version `1`.
//!
//! THIS FILE IS FROZEN. Its bytes ARE the identity of version `1`. See the
//! sibling `review_by_expected_author_login_v1.rs` header for the obligation and
//! why editing this file is not how a behaviour change is made.
//!
//! THE RULE. A candidate matches when it *is* an Actions check run and its `name`
//! equals `parameters.checkName`.
//!
//! SELECTION IS BY NAME, NOT BY OUTCOME. A failed run of the required check must
//! be selected, so the classifier can see that it failed. Filtering on
//! `conclusion` here would make a red gate indistinguishable from a gate that
//! never ran — `FINDING` silently rewritten as `CANNOT_CHECK`.

use serde_json::Value;

use crate::MatchError;

pub(crate) fn matches(candidate: &Value, parameters: &Value) -> Result<bool, MatchError> {
    let expected = parameters.get("checkName").and_then(Value::as_str).ok_or(
        MatchError::MalformedCandidate {
            why: "checkName is not a string",
        },
    )?;
    if candidate.pointer("/sourceKind").and_then(Value::as_str) != Some("github-actions-check") {
        return Ok(false);
    }
    Ok(candidate.pointer("/name").and_then(Value::as_str) == Some(expected))
}
