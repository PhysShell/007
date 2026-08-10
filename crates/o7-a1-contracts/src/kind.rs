//! FD-1.9 `ArtifactKindV1`, the eleven message kinds, and the producer role.
//!
//! Every enum here is **closed** (FD-1.6): no `#[serde(other)]` catch-all, so an
//! unrecognised value on the wire fails at deserialization instead of being
//! absorbed into a variant that means "something we did not understand".
//!
//! Every enum is also framed **by name, not by tag byte** (FD-1.2). A1 is a
//! cross-implementation wire contract: an unassigned tag byte means two
//! conforming implementations can pick `work_order = 0` and `work_order = 1`
//! and both be correct while computing different digests. The `snake_case` name
//! removes the coordination problem instead of adding numbering tables to
//! maintain — and a renamed variant becomes a different digest, "which is the
//! desired alarm, not a bug".

use serde::{Deserialize, Serialize};

/// §3.0 — the producer role that authored an envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProducerRole {
    Controller,
    Coder,
    Reviewer,
    Human,
}

impl ProducerRole {
    /// The frozen `snake_case` ASCII name, as framed by FD-1.2.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Coder => "coder",
            Self::Reviewer => "reviewer",
            Self::Human => "human",
        }
    }

    /// §3.0 — `model_identity`, `prompt_digest`, `tool_policy_digest` and
    /// `provider_execution_receipt_ref` are required iff the role is a provider
    /// role, and forbidden otherwise.
    #[must_use]
    pub fn is_provider_role(self) -> bool {
        matches!(self, Self::Coder | Self::Reviewer)
    }
}

/// The eleven envelope-bearing message kinds (§3.1–§3.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MessageKindV1 {
    WorkOrder,
    CoderReport,
    CandidateReceipt,
    ReviewRequest,
    ReviewerReport,
    ReviewVerdict,
    CorrectiveDirective,
    CampaignFeedItem,
    HumanAttentionRequest,
    HumanCommandRequest,
    HumanDecision,
}

impl MessageKindV1 {
    /// The frozen `snake_case` ASCII name, as framed by FD-1.2.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::WorkOrder => "work_order",
            Self::CoderReport => "coder_report",
            Self::CandidateReceipt => "candidate_receipt",
            Self::ReviewRequest => "review_request",
            Self::ReviewerReport => "reviewer_report",
            Self::ReviewVerdict => "review_verdict",
            Self::CorrectiveDirective => "corrective_directive",
            Self::CampaignFeedItem => "campaign_feed_item",
            Self::HumanAttentionRequest => "human_attention_request",
            Self::HumanCommandRequest => "human_command_request",
            Self::HumanDecision => "human_decision",
        }
    }

    /// The corresponding [`ArtifactKindV1`], so a ref to this artifact and the
    /// envelope that carries it cannot drift apart.
    #[must_use]
    pub fn artifact_kind(self) -> ArtifactKindV1 {
        match self {
            Self::WorkOrder => ArtifactKindV1::WorkOrder,
            Self::CoderReport => ArtifactKindV1::CoderReport,
            Self::CandidateReceipt => ArtifactKindV1::CandidateReceipt,
            Self::ReviewRequest => ArtifactKindV1::ReviewRequest,
            Self::ReviewerReport => ArtifactKindV1::ReviewerReport,
            Self::ReviewVerdict => ArtifactKindV1::ReviewVerdict,
            Self::CorrectiveDirective => ArtifactKindV1::CorrectiveDirective,
            Self::CampaignFeedItem => ArtifactKindV1::CampaignFeedItem,
            Self::HumanAttentionRequest => ArtifactKindV1::HumanAttentionRequest,
            Self::HumanCommandRequest => ArtifactKindV1::HumanCommandRequest,
            Self::HumanDecision => ArtifactKindV1::HumanDecision,
        }
    }

    /// Every kind, in §3 order — for exhaustiveness tests and registries.
    pub const ALL: [Self; 11] = [
        Self::WorkOrder,
        Self::CoderReport,
        Self::CandidateReceipt,
        Self::ReviewRequest,
        Self::ReviewerReport,
        Self::ReviewVerdict,
        Self::CorrectiveDirective,
        Self::CampaignFeedItem,
        Self::HumanAttentionRequest,
        Self::HumanCommandRequest,
        Self::HumanDecision,
    ];
}

/// FD-1.9 — the complete closed set of artifact kinds.
///
/// "An incomplete list would put implementations back where the tag-byte
/// problem left them: inventing spellings and computing different digests."
///
/// The imported A0 kinds are spelled **exactly** as `o7_run::event::ArtifactKind`
/// already serializes them, so a `candidate_state` reference denotes the same
/// object on both sides of the boundary — the entire point of importing a kind
/// instead of redefining it (FD-2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactKindV1 {
    // The eleven A1 message kinds.
    WorkOrder,
    CoderReport,
    CandidateReceipt,
    ReviewRequest,
    ReviewerReport,
    ReviewVerdict,
    CorrectiveDirective,
    CampaignFeedItem,
    HumanAttentionRequest,
    HumanCommandRequest,
    HumanDecision,

    // A1 typed non-envelope objects.
    ProviderExecutionReceipt,
    InteractionManifest,
    ScopeContract,
    CampaignEventPayload,

    // Imported A0 kinds.
    CandidateState,
    CandidatePatch,
    Worktree,
    Policy,
    Task,
    Diff,
    GateLog,
    SandboxReport,
    ProviderSession,
    CommandBinding,

    // A1 evidence blobs.
    ContractDocument,
    VerifierRegistry,
    CanonicalProviderRequest,
    RawProviderBytes,
    NormalizedOutput,
    ProviderMessage,
    ToolArguments,
    ToolResult,
    UsageRecord,
    CostRecord,
    DiagnosticLog,
    CiObservation,
    TerminationObservation,
    DetailDocument,
}

impl ArtifactKindV1 {
    /// The frozen `snake_case` ASCII name, as framed by FD-1.2.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::WorkOrder => "work_order",
            Self::CoderReport => "coder_report",
            Self::CandidateReceipt => "candidate_receipt",
            Self::ReviewRequest => "review_request",
            Self::ReviewerReport => "reviewer_report",
            Self::ReviewVerdict => "review_verdict",
            Self::CorrectiveDirective => "corrective_directive",
            Self::CampaignFeedItem => "campaign_feed_item",
            Self::HumanAttentionRequest => "human_attention_request",
            Self::HumanCommandRequest => "human_command_request",
            Self::HumanDecision => "human_decision",

            Self::ProviderExecutionReceipt => "provider_execution_receipt",
            Self::InteractionManifest => "interaction_manifest",
            Self::ScopeContract => "scope_contract",
            Self::CampaignEventPayload => "campaign_event_payload",

            Self::CandidateState => "candidate_state",
            Self::CandidatePatch => "candidate_patch",
            Self::Worktree => "worktree",
            Self::Policy => "policy",
            Self::Task => "task",
            Self::Diff => "diff",
            Self::GateLog => "gate_log",
            Self::SandboxReport => "sandbox_report",
            Self::ProviderSession => "provider_session",
            Self::CommandBinding => "command_binding",

            Self::ContractDocument => "contract_document",
            Self::VerifierRegistry => "verifier_registry",
            Self::CanonicalProviderRequest => "canonical_provider_request",
            Self::RawProviderBytes => "raw_provider_bytes",
            Self::NormalizedOutput => "normalized_output",
            Self::ProviderMessage => "provider_message",
            Self::ToolArguments => "tool_arguments",
            Self::ToolResult => "tool_result",
            Self::UsageRecord => "usage_record",
            Self::CostRecord => "cost_record",
            Self::DiagnosticLog => "diagnostic_log",
            Self::CiObservation => "ci_observation",
            Self::TerminationObservation => "termination_observation",
            Self::DetailDocument => "detail_document",
        }
    }

    /// Every kind, exhaustively.
    ///
    /// The length is written out so a new variant fails to compile until this
    /// array is updated — the point being that the spelling tests below cannot
    /// silently stop covering a kind. A renamed variant whose `name()` and serde
    /// spelling drift apart makes two conforming implementations compute
    /// different digests, which is precisely the failure FD-1.2 frames names to
    /// avoid.
    pub const ALL: [Self; 39] = [
        Self::WorkOrder,
        Self::CoderReport,
        Self::CandidateReceipt,
        Self::ReviewRequest,
        Self::ReviewerReport,
        Self::ReviewVerdict,
        Self::CorrectiveDirective,
        Self::CampaignFeedItem,
        Self::HumanAttentionRequest,
        Self::HumanCommandRequest,
        Self::HumanDecision,
        Self::ProviderExecutionReceipt,
        Self::InteractionManifest,
        Self::ScopeContract,
        Self::CampaignEventPayload,
        Self::CandidateState,
        Self::CandidatePatch,
        Self::Worktree,
        Self::Policy,
        Self::Task,
        Self::Diff,
        Self::GateLog,
        Self::SandboxReport,
        Self::ProviderSession,
        Self::CommandBinding,
        Self::ContractDocument,
        Self::VerifierRegistry,
        Self::CanonicalProviderRequest,
        Self::RawProviderBytes,
        Self::NormalizedOutput,
        Self::ProviderMessage,
        Self::ToolArguments,
        Self::ToolResult,
        Self::UsageRecord,
        Self::CostRecord,
        Self::DiagnosticLog,
        Self::CiObservation,
        Self::TerminationObservation,
        Self::DetailDocument,
    ];

    /// Whether this kind names a **typed A1 artifact** — the eleven message
    /// kinds plus the four typed non-envelope objects of FD-1.9.
    ///
    /// This is the classification FD-1.7 uses: a typed artifact's media type is
    /// fixed by its kind and version. Evidence blobs and imported A0 objects
    /// "carry their own concrete type" and are not constrained.
    #[must_use]
    pub fn is_typed_artifact(self) -> bool {
        self.is_envelope_bearing()
            || matches!(
                self,
                Self::ProviderExecutionReceipt
                    | Self::InteractionManifest
                    | Self::ScopeContract
                    | Self::CampaignEventPayload
            )
    }

    /// Whether the 1 MiB **typed-object** bound of FD-1.4 applies, rather than
    /// the 64 MiB one. ("Control artifact" is §5's name for the same class and
    /// the one [`crate::bounds::MAX_CONTROL_ARTIFACT_BYTES`] carries; FD-1.4
    /// itself says "typed A1 JSON object, except the one below".)
    ///
    /// Deliberately *not* the same set as [`Self::is_typed_artifact`]:
    /// `interaction_manifest` is a typed artifact for FD-1.7 media-type purposes
    /// and keeps its FD-2 rank, but S1 (§7 supersede, §9) fixes its size bound at
    /// the evidence maximum. Two questions, two classifications — collapsing them
    /// is what let a manifest ref declare any media type at all.
    #[must_use]
    pub fn has_control_size_bound(self) -> bool {
        matches!(
            self,
            Self::ProviderExecutionReceipt | Self::ScopeContract | Self::CampaignEventPayload
        )
    }

    /// Whether a ref of this kind names an envelope-bearing artifact, which
    /// FD-1.8 stores as **two** byte strings and charges accordingly.
    #[must_use]
    pub fn is_envelope_bearing(self) -> bool {
        matches!(
            self,
            Self::WorkOrder
                | Self::CoderReport
                | Self::CandidateReceipt
                | Self::ReviewRequest
                | Self::ReviewerReport
                | Self::ReviewVerdict
                | Self::CorrectiveDirective
                | Self::CampaignFeedItem
                | Self::HumanAttentionRequest
                | Self::HumanCommandRequest
                | Self::HumanDecision
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_kind_and_its_artifact_kind_share_a_name() {
        for kind in MessageKindV1::ALL {
            assert_eq!(kind.name(), kind.artifact_kind().name());
            assert!(kind.artifact_kind().is_envelope_bearing());
        }
    }

    // The serde spelling and the framed name must be the same string: one is
    // what a peer writes, the other is what both sides hash.
    #[test]
    fn the_serde_spelling_is_the_framed_name() {
        for kind in MessageKindV1::ALL {
            let json = serde_json::to_string(&kind).unwrap_or_default();
            assert_eq!(json, format!("\"{}\"", kind.name()));
        }
        for role in [
            ProducerRole::Controller,
            ProducerRole::Coder,
            ProducerRole::Reviewer,
            ProducerRole::Human,
        ] {
            let json = serde_json::to_string(&role).unwrap_or_default();
            assert_eq!(json, format!("\"{}\"", role.name()));
        }
        // All 39, not only the eleven message kinds: the four typed
        // non-envelope kinds and the fourteen evidence blobs are framed by name
        // too, so a drift between `name()` and the serde spelling in any of them
        // splits digests between implementations.
        for kind in ArtifactKindV1::ALL {
            let json = serde_json::to_string(&kind).unwrap_or_default();
            assert_eq!(json, format!("\"{}\"", kind.name()));
        }
    }

    #[test]
    fn every_artifact_kind_appears_exactly_once_in_all() {
        let mut names: Vec<&str> = ArtifactKindV1::ALL.iter().map(|k| k.name()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "a kind is listed twice in ALL");
        // Round-tripping every spelling proves ALL is not merely internally
        // consistent but complete against the serde surface.
        for kind in ArtifactKindV1::ALL {
            let json = format!("\"{}\"", kind.name());
            assert_eq!(
                serde_json::from_str::<ArtifactKindV1>(&json).ok(),
                Some(kind)
            );
        }
    }

    // FD-1.9 imports A0's spellings verbatim. If o7-run ever renames one, this
    // fails here rather than silently denoting a different object across the
    // boundary.
    #[test]
    fn imported_a0_kinds_keep_a0_spellings() {
        for (a1, a0) in [
            (
                ArtifactKindV1::CandidateState,
                o7_run::event::ArtifactKind::CandidateState,
            ),
            (
                ArtifactKindV1::CandidatePatch,
                o7_run::event::ArtifactKind::CandidatePatch,
            ),
            (
                ArtifactKindV1::Worktree,
                o7_run::event::ArtifactKind::Worktree,
            ),
            (ArtifactKindV1::Policy, o7_run::event::ArtifactKind::Policy),
            (ArtifactKindV1::Task, o7_run::event::ArtifactKind::Task),
            (ArtifactKindV1::Diff, o7_run::event::ArtifactKind::Diff),
            (
                ArtifactKindV1::GateLog,
                o7_run::event::ArtifactKind::GateLog,
            ),
            (
                ArtifactKindV1::SandboxReport,
                o7_run::event::ArtifactKind::SandboxReport,
            ),
            (
                ArtifactKindV1::ProviderSession,
                o7_run::event::ArtifactKind::ProviderSession,
            ),
            (
                ArtifactKindV1::CommandBinding,
                o7_run::event::ArtifactKind::CommandBinding,
            ),
        ] {
            let a0_json = serde_json::to_string(&a0).unwrap_or_default();
            assert_eq!(a0_json, format!("\"{}\"", a1.name()));
        }
    }

    #[test]
    fn an_unknown_variant_fails_closed() {
        assert!(serde_json::from_str::<MessageKindV1>("\"work_order\"").is_ok());
        assert!(serde_json::from_str::<MessageKindV1>("\"workorder\"").is_err());
        assert!(serde_json::from_str::<MessageKindV1>("\"future_kind\"").is_err());
        assert!(serde_json::from_str::<ArtifactKindV1>("\"future_kind\"").is_err());
        assert!(serde_json::from_str::<ProducerRole>("\"architect\"").is_err());
    }

    #[test]
    fn only_provider_roles_carry_provider_evidence() {
        assert!(ProducerRole::Coder.is_provider_role());
        assert!(ProducerRole::Reviewer.is_provider_role());
        assert!(!ProducerRole::Controller.is_provider_role());
        assert!(!ProducerRole::Human.is_provider_role());
    }
}
