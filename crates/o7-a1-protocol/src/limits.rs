//! The frozen v1 limits profile (contract §5). Violation is REJECT,
//! never truncation; a streaming reader stops at cap + 1 so the limit
//! check cannot happen after the bytes are already in memory.

/// Max nesting depth of a canonical value.
pub const MAX_NESTING_DEPTH: u32 = 32;
/// Max artifact refs per artifact.
pub const MAX_ARTIFACT_REFS: usize = 256;
/// Max items in any one collection.
pub const MAX_COLLECTION_ITEMS: usize = 4096;
/// Max one inline string/byte-string.
pub const MAX_INLINE_STRING_BYTES: usize = 64 * 1024;
/// Max aggregate inline prose per artifact.
pub const MAX_AGGREGATE_PROSE_BYTES: usize = 256 * 1024;

/// Per-kind canonical artifact byte ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindLimit {
    /// Stable kind name (matches the §11.1 envelope-kind list).
    pub kind: &'static str,
    /// Hard byte ceiling for the canonical artifact.
    pub max_bytes: usize,
}

const KIB: usize = 1024;
const MIB: usize = 1024 * 1024;

/// The frozen per-kind table (contract §5).
pub const KIND_LIMITS: &[KindLimit] = &[
    KindLimit {
        kind: "work_order",
        max_bytes: 256 * KIB,
    },
    KindLimit {
        kind: "coder_report",
        max_bytes: 512 * KIB,
    },
    KindLimit {
        kind: "candidate_admission_receipt",
        max_bytes: 256 * KIB,
    },
    KindLimit {
        kind: "review_request",
        max_bytes: 512 * KIB,
    },
    KindLimit {
        kind: "reviewer_report",
        max_bytes: 512 * KIB,
    },
    KindLimit {
        kind: "review_verdict",
        max_bytes: 512 * KIB,
    },
    KindLimit {
        kind: "corrective_directive",
        max_bytes: 256 * KIB,
    },
    KindLimit {
        kind: "provider_invocation_receipt",
        max_bytes: 256 * KIB,
    },
    KindLimit {
        kind: "interaction_manifest",
        max_bytes: 2 * MIB,
    },
    KindLimit {
        kind: "campaign_feed_item",
        max_bytes: 128 * KIB,
    },
    KindLimit {
        kind: "human_attention_request",
        max_bytes: 256 * KIB,
    },
    KindLimit {
        kind: "human_command_request",
        max_bytes: 64 * KIB,
    },
    KindLimit {
        kind: "human_decision",
        max_bytes: 128 * KIB,
    },
];

/// Max one provider evidence blob.
pub const MAX_EVIDENCE_BLOB_BYTES: usize = 64 * MIB;
/// Max aggregate evidence per execution.
pub const MAX_EVIDENCE_PER_EXECUTION_BYTES: usize = 128 * MIB;

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn kind_limits_are_unique_and_nonzero() {
        let names: BTreeSet<&str> = KIND_LIMITS.iter().map(|k| k.kind).collect();
        assert_eq!(names.len(), KIND_LIMITS.len());
        assert!(KIND_LIMITS.iter().all(|k| k.max_bytes > 0));
    }
}
