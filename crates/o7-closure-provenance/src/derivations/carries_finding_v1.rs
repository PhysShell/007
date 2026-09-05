//! `review-carries-finding/1`.
//!
//! Provenance V1 §18's worked example, as a re-executable function:
//!
//! ```text
//! review carries a finding
//!     because review_comment.pullRequestReviewId == review.stableId
//! ```
//!
//! Two sources in order: the submitted review, then the review comment. The
//! rule reads exactly two fields and nothing else — no body text, no state, no
//! author. A derivation that consulted the review's `state` would be deciding
//! whether a verdict counts, which is the classifier's job and not an
//! acquisition fact.
//!
//! This file's bytes ARE the identity of `review-carries-finding/1`. Editing it
//! requires a new version, never a new digest on this one.

use serde_json::Value;

/// `Some(bool)` when both sources carry the fields the rule reads; `None` when
/// either does not.
///
/// `None` rather than `false`: a review comment whose `pullRequestReviewId` did
/// not survive projection has not established that the review carries no
/// finding — it has established nothing, and reporting that as `false` is the
/// absent-signal-as-negative-result error this whole effort exists to refuse.
pub(crate) fn derive(sources: &[Value]) -> Option<Value> {
    let review = sources.first()?;
    let comment = sources.get(1)?;

    let review_id = review.pointer("/stableId")?.as_str()?;
    let owning_review = comment.pointer("/pullRequestReviewId")?.as_str()?;

    Some(Value::Bool(review_id == owning_review))
}
