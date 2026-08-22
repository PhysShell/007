//! The five §8 source-snapshot shapes, transcribed from
//! `docs/architecture/closure-source-provenance-v1.md`.
//!
//! Every candidate is validated against the shape of the kind **it declares**,
//! which is why all five live here rather than one per matcher. A matcher names
//! only the surface it scores; a candidate of another surface still has to be a
//! well-formed object of that surface, or a malformed foreign object gets scored
//! `false` and joins an absence claim.
//!
//! `tests/schema_parity.rs` parses the REQUIRED and OPTIONAL-IF-PRESENT blocks
//! out of §8 and fails if any table here disagrees with the document, so these
//! are a transcription that is checked rather than a second source of truth.

use crate::{CandidateSchema, Member, ValueKind};

const fn req(name: &'static str) -> Member {
    Member {
        name,
        required: true,
        kind: ValueKind::Text,
    }
}
const fn opt(name: &'static str) -> Member {
    Member {
        name,
        required: false,
        kind: ValueKind::Text,
    }
}
const fn int(name: &'static str) -> Member {
    Member {
        name,
        required: true,
        kind: ValueKind::Integer,
    }
}
const fn opt_int(name: &'static str) -> Member {
    Member {
        name,
        required: false,
        kind: ValueKind::Integer,
    }
}

/// `user.id`, `user.login`, `user.type` — the same nested shape wherever §8
/// writes it. `user.type` is REQUIRED because it is what separates `Bot` from
/// `User` without string-matching a login.
const USER: &[Member] = &[req("id"), req("login"), req("type")];
const fn user() -> Member {
    Member {
        name: "user",
        required: true,
        kind: ValueKind::Object(USER),
    }
}

/// §8.1
const PULL_REQUEST_HEAD_V1: &[Member] = &[
    int("schemaVersion"),
    req("sourceKind"),
    req("repository"),
    req("pullRequest"),
    req("headSha"),
    req("headRef"),
    req("headRepoFullName"),
    opt("updatedAt"),
];

/// §8.3
const ACTIONS_CHECK_V1: &[Member] = &[
    int("schemaVersion"),
    req("sourceKind"),
    req("stableId"),
    req("name"),
    req("headSha"),
    req("status"),
    opt("conclusion"),
    opt("startedAt"),
    opt("completedAt"),
];

/// §8.2
const SUBMITTED_REVIEW_V1: &[Member] = &[
    int("schemaVersion"),
    req("sourceKind"),
    req("stableId"),
    user(),
    req("authorAssociation"),
    req("state"),
    req("body"),
    req("submittedAt"),
    req("commitId"),
];

/// §8.4
const REVIEW_COMMENT_V1: &[Member] = &[
    int("schemaVersion"),
    req("sourceKind"),
    req("stableId"),
    req("pullRequestReviewId"),
    user(),
    req("authorAssociation"),
    req("body"),
    req("commitId"),
    req("originalCommitId"),
    req("path"),
    req("createdAt"),
    req("updatedAt"),
    opt("inReplyToId"),
    opt_int("line"),
    opt_int("originalLine"),
    opt("side"),
    opt_int("startLine"),
];

/// §8.5
const ISSUE_COMMENT_V1: &[Member] = &[
    int("schemaVersion"),
    req("sourceKind"),
    req("stableId"),
    user(),
    req("authorAssociation"),
    req("body"),
    req("createdAt"),
    req("updatedAt"),
];

/// Every surface §8 defines. A `sourceKind` absent from this table is evidence
/// whose shape is unknown, and is refused rather than scored.
pub(crate) const ALL: &[CandidateSchema] = &[
    CandidateSchema {
        source_kind: "github-pull-request-head",
        schema_version: 1,
        members: PULL_REQUEST_HEAD_V1,
    },
    CandidateSchema {
        source_kind: "github-actions-check",
        schema_version: 1,
        members: ACTIONS_CHECK_V1,
    },
    CandidateSchema {
        source_kind: "github-submitted-review",
        schema_version: 1,
        members: SUBMITTED_REVIEW_V1,
    },
    CandidateSchema {
        source_kind: "github-review-comment",
        schema_version: 1,
        members: REVIEW_COMMENT_V1,
    },
    CandidateSchema {
        source_kind: "github-issue-comment",
        schema_version: 1,
        members: ISSUE_COMMENT_V1,
    },
];
