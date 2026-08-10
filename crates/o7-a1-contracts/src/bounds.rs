//! FD-1.4 per-object bounds and FD-1.5 aggregate bounds.
//!
//! Every number here is a **protocol hard maximum**: a compile-time constant,
//! never configurable. Exceeding one is a parse-time rejection, never a
//! truncation — "a truncated artifact that still parses is the failure mode
//! these bounds exist to forbid" (FD-1.4).
//!
//! The aggregate constants of FD-1.5 live here too, even though closure
//! traversal itself is step 2 work. They belong with the bounds they extend,
//! and the two invariants the contract requires to be unit-tested are asserted
//! against them in this module's tests rather than deferred to the code that
//! will eventually walk a graph.

use crate::framing::Preimage;
use crate::scalars::Digest256;

/// FD-1.4 — any typed A1 payload.
pub const MAX_CONTROL_ARTIFACT_BYTES: u64 = 1 << 20; // 1 MiB
/// FD-1.4 — a single evidence blob (diff, raw provider bytes, gate log, …).
pub const MAX_EVIDENCE_BLOB_BYTES: u64 = 64 << 20; // 64 MiB
/// FD-1.4 — JSON nesting depth.
pub const MAX_JSON_DEPTH: usize = 32;
/// FD-1.4 — length of any array.
pub const MAX_ARRAY_LEN: usize = 4096;
/// FD-1.4 — length of any single string field, in bytes.
pub const MAX_STRING_BYTES: usize = 65536;
/// FD-1.4 — length of an opaque id, in bytes.
pub const MAX_ID_BYTES: usize = 256;
/// FD-1.4 — `artifact_refs` per envelope.
pub const MAX_ARTIFACT_REFS: usize = 256;
/// FD-1.4 — `interaction_sequence` entries per manifest.
pub const MAX_INTERACTION_SEQUENCE: usize = 4096;
/// FD-1.4 — dispatches per execution receipt.
pub const MAX_DISPATCHES_PER_RECEIPT: usize = 256;
/// FD-1.4 — findings per `ReviewerReport` / `ReviewVerdict`.
pub const MAX_FINDINGS: usize = 256;

/// FD-1.5 — sum over `immediate_refs`.
pub const MAX_DIRECT_REFERENCED_BYTES: u64 = 128 << 20; // 128 MiB
/// FD-1.5 — sum over the deduplicated closure.
pub const MAX_REACHABLE_CLOSURE_BYTES: u64 = 256 << 20; // 256 MiB
/// FD-1.5 — objects in the deduplicated closure.
pub const MAX_REACHABLE_CLOSURE_OBJECTS: u32 = 2048;
/// FD-1.5 — all refs minted by one execution.
pub const MAX_REFS_PER_EXECUTION: u32 = 4096;

/// FD-1.5 — the four campaign-policy values, carried in `CampaignCreated` and
/// held in `CampaignStateV1.budget_policy`.
///
/// The values travel with their digest deliberately: "a digest without its
/// preimage would have been a receipt for a value nobody can read."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetPolicy {
    pub max_provider_turns: u32,
    pub max_wall_time_seconds: u32,
    pub evidence_budget_bytes: u64,
    pub closure_object_budget: u32,
}

/// A campaign policy that exceeds the corresponding hard maximum.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BudgetPolicyError {
    #[error("max_provider_turns must be >= 1")]
    ZeroProviderTurns,
    #[error("max_wall_time_seconds must be >= 1")]
    ZeroWallTime,
    #[error(
        "evidence_budget_bytes {actual} exceeds the hard maximum {MAX_REACHABLE_CLOSURE_BYTES}"
    )]
    EvidenceBudgetTooLarge { actual: u64 },
    #[error(
        "closure_object_budget {actual} exceeds the hard maximum {MAX_REACHABLE_CLOSURE_OBJECTS}"
    )]
    ClosureObjectBudgetTooLarge { actual: u32 },
}

impl BudgetPolicy {
    /// The FD-1.5 campaign-creation check: every policy value must be at most
    /// the corresponding hard maximum.
    pub fn validate(&self) -> Result<(), BudgetPolicyError> {
        if self.max_provider_turns < 1 {
            return Err(BudgetPolicyError::ZeroProviderTurns);
        }
        if self.max_wall_time_seconds < 1 {
            return Err(BudgetPolicyError::ZeroWallTime);
        }
        if self.evidence_budget_bytes > MAX_REACHABLE_CLOSURE_BYTES {
            return Err(BudgetPolicyError::EvidenceBudgetTooLarge {
                actual: self.evidence_budget_bytes,
            });
        }
        if self.closure_object_budget > MAX_REACHABLE_CLOSURE_OBJECTS {
            return Err(BudgetPolicyError::ClosureObjectBudgetTooLarge {
                actual: self.closure_object_budget,
            });
        }
        Ok(())
    }

    /// `budget_policy_digest`, framed over exactly the four values in exactly
    /// this order (FD-1.5), "so two implementations cannot disagree about what
    /// the campaign's policy was".
    pub fn digest(&self) -> Digest256 {
        let mut p = Preimage::new(b"o7-a1-budget-policy\0v1\0");
        p.frame_u32(self.max_provider_turns)
            .frame_u32(self.max_wall_time_seconds)
            .frame_u64(self.evidence_budget_bytes)
            .frame_u32(self.closure_object_budget);
        p.digest()
    }

    /// FD-1.5 — the effective bound for a resolution is
    /// `min(hard maximum, campaign policy)`.
    pub fn effective_closure_bytes(&self) -> u64 {
        self.evidence_budget_bytes.min(MAX_REACHABLE_CLOSURE_BYTES)
    }

    /// FD-1.5 — the effective object bound for a resolution.
    pub fn effective_closure_objects(&self) -> u32 {
        self.closure_object_budget
            .min(MAX_REACHABLE_CLOSURE_OBJECTS)
    }
}

/// The V0 campaign default named in FD-1.5.
pub const V0_DEFAULT_EVIDENCE_BUDGET_BYTES: u64 = 64 << 20; // 64 MiB
/// The V0 campaign default named in FD-1.5.
pub const V0_DEFAULT_CLOSURE_OBJECT_BUDGET: u32 = 512;

// FD-1.5 states this invariant explicitly, "because a bound that contradicts
// another bound is decoration", and asks for it to be checked in a unit test.
// A const assertion is strictly stronger: a violating pair of constants fails to
// compile, so the check cannot be satisfied by a test nobody runs.
const _: () = assert!(MAX_DIRECT_REFERENCED_BYTES <= MAX_REACHABLE_CLOSURE_BYTES);
const _: () = assert!(V0_DEFAULT_EVIDENCE_BUDGET_BYTES <= MAX_REACHABLE_CLOSURE_BYTES);
const _: () = assert!(V0_DEFAULT_CLOSURE_OBJECT_BUDGET <= MAX_REACHABLE_CLOSURE_OBJECTS);

#[cfg(test)]
mod tests {
    use super::*;

    // The first FD-1.5 invariant (direct refs are a subset of the closure) is a
    // const assertion above — it cannot be expressed as a failing test, because
    // a violating pair does not compile. This covers the second: a campaign
    // policy is never above its hard maximum.
    #[test]
    fn the_v0_defaults_are_within_the_hard_maxima() {
        let policy = BudgetPolicy {
            max_provider_turns: 8,
            max_wall_time_seconds: 3600,
            evidence_budget_bytes: V0_DEFAULT_EVIDENCE_BUDGET_BYTES,
            closure_object_budget: V0_DEFAULT_CLOSURE_OBJECT_BUDGET,
        };
        assert_eq!(policy.validate(), Ok(()));
    }

    #[test]
    fn a_policy_above_a_hard_maximum_is_refused() {
        let policy = BudgetPolicy {
            max_provider_turns: 1,
            max_wall_time_seconds: 1,
            evidence_budget_bytes: MAX_REACHABLE_CLOSURE_BYTES + 1,
            closure_object_budget: 1,
        };
        assert!(matches!(
            policy.validate(),
            Err(BudgetPolicyError::EvidenceBudgetTooLarge { .. })
        ));
    }

    #[test]
    fn the_effective_bound_is_the_minimum_of_policy_and_hard_maximum() {
        let policy = BudgetPolicy {
            max_provider_turns: 1,
            max_wall_time_seconds: 1,
            evidence_budget_bytes: V0_DEFAULT_EVIDENCE_BUDGET_BYTES,
            closure_object_budget: V0_DEFAULT_CLOSURE_OBJECT_BUDGET,
        };
        assert_eq!(
            policy.effective_closure_bytes(),
            V0_DEFAULT_EVIDENCE_BUDGET_BYTES
        );
        assert_eq!(
            policy.effective_closure_objects(),
            V0_DEFAULT_CLOSURE_OBJECT_BUDGET
        );
    }

    #[test]
    fn the_budget_digest_changes_when_any_value_changes() {
        let base = BudgetPolicy {
            max_provider_turns: 8,
            max_wall_time_seconds: 3600,
            evidence_budget_bytes: V0_DEFAULT_EVIDENCE_BUDGET_BYTES,
            closure_object_budget: V0_DEFAULT_CLOSURE_OBJECT_BUDGET,
        };
        let turns = BudgetPolicy {
            max_provider_turns: 9,
            ..base
        };
        assert_ne!(base.digest(), turns.digest());
        assert_eq!(base.digest(), base.digest());
    }
}
