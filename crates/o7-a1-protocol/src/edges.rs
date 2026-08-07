//! The closed reference universe and the frozen per-kind allowed-edge
//! matrix (contract §11.1–§11.4). Edge tags are wire semantics: the
//! `ref_manifest` sorts by `(edge kind tag, target digest)` into a
//! protocol digest, so the tag registry carries uniqueness and
//! known-answer tests, and renaming a carrying field is a supersede-path
//! wire change.

use serde::{Deserialize, Serialize};

/// The 15 envelope-bearing message kinds (contract §11.1 class 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    WorkOrder,
    CoderReport,
    CandidateAdmissionReceipt,
    ReviewRequest,
    ReviewerReport,
    ReviewVerdict,
    CorrectiveDirective,
    ProviderInvocationReceipt,
    InteractionManifest,
    CampaignFeedItem,
    HumanAttentionRequest,
    HumanCommandRequest,
    HumanDecision,
    CampaignRunBinding,
    ArtifactImported,
}

/// Node classes of the closed universe (contract §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Node {
    /// Class 1 — envelope-bearing kind.
    Kind(MessageKind),
    /// Class 2/5 — a typed CAS blob (terminal; no outgoing edges).
    CasBlob,
    /// Class 3 — external wrapper into A0/R1 canonical records
    /// (terminal from the A1 DAG's perspective).
    ExternalWrapper,
    /// Class 4 — A2-only attention transition records.
    AttentionTransition,
}

/// Edge classification (contract §11.3–§11.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeClass {
    /// Within one round's derivation flow — the subgraph that must
    /// topologically sort at KIND level.
    Intra,
    /// Crossing rounds/chains/attention lineage — instance-acyclic by
    /// create-before-reference (+ strictly-lower `round_ordinal` where
    /// one applies).
    Causal,
}

/// One frozen edge: `from` may directly carry a digest reference tagged
/// `tag` to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Source node.
    pub from: Node,
    /// Stable snake_case tag — equal to the field path carrying the ref.
    pub tag: &'static str,
    /// Target node.
    pub to: Node,
    /// Classification.
    pub class: EdgeClass,
}

use EdgeClass::{Causal, Intra};
use MessageKind as K;
use Node::{CasBlob, ExternalWrapper, Kind};

const fn e(from: MessageKind, tag: &'static str, to: Node, class: EdgeClass) -> Edge {
    Edge {
        from: Kind(from),
        tag,
        to,
        class,
    }
}

/// The frozen §11.3 matrix, EXHAUSTIVE per kind, including
/// producer-binding and cause edges. The global causation rule (every
/// envelope kind carries one `causation` edge, class `causal`, target
/// any committed envelope kind) is represented by [`CAUSATION_TAG`] and
/// enforced at the manifest layer, not repeated per row.
pub const CAUSATION_TAG: &str = "causation.blob_ref";

/// Every frozen non-causation edge.
pub const EDGES: &[Edge] = &[
    // WorkOrder
    e(K::WorkOrder, "goal.contract_blob", CasBlob, Intra),
    e(K::WorkOrder, "input.correspondence_ref", CasBlob, Intra),
    e(K::WorkOrder, "input.state_refs", ExternalWrapper, Intra),
    // CoderReport (raw)
    e(
        K::CoderReport,
        "producer.invocation_receipt_ref",
        Kind(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(K::CoderReport, "claims.evidence_refs.gate", CasBlob, Intra),
    e(K::CoderReport, "claims.evidence_refs.diff", CasBlob, Intra),
    // CandidateAdmissionReceipt
    e(
        K::CandidateAdmissionReceipt,
        "coder_report_ref",
        Kind(K::CoderReport),
        Intra,
    ),
    e(
        K::CandidateAdmissionReceipt,
        "coder_run_binding_ref.blob_ref",
        Kind(K::CampaignRunBinding),
        Intra,
    ),
    e(
        K::CandidateAdmissionReceipt,
        "candidate_state_ref",
        ExternalWrapper,
        Intra,
    ),
    // ReviewRequest — NO separate CoderReport edge (§8.4)
    e(
        K::ReviewRequest,
        "admission_receipt_ref",
        Kind(K::CandidateAdmissionReceipt),
        Intra,
    ),
    e(K::ReviewRequest, "contract_blob", CasBlob, Intra),
    e(K::ReviewRequest, "evidence_refs.gate", CasBlob, Intra),
    e(K::ReviewRequest, "evidence_refs.diff", CasBlob, Intra),
    // ReviewerReport (raw)
    e(
        K::ReviewerReport,
        "producer.invocation_receipt_ref",
        Kind(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(K::ReviewerReport, "evidence_refs.gate", CasBlob, Intra),
    // ReviewVerdict
    e(
        K::ReviewVerdict,
        "reviewer_report_ref",
        Kind(K::ReviewerReport),
        Intra,
    ),
    e(
        K::ReviewVerdict,
        "findings.evidence_refs.gate",
        CasBlob,
        Intra,
    ),
    e(
        K::ReviewVerdict,
        "reviewed_candidate_state_ref",
        ExternalWrapper,
        Intra,
    ),
    // CorrectiveDirective
    e(
        K::CorrectiveDirective,
        "verdict_ref",
        Kind(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::CorrectiveDirective,
        "input.state_refs",
        ExternalWrapper,
        Intra,
    ),
    // ProviderInvocationReceipt (Controller-produced, §11.2)
    e(
        K::ProviderInvocationReceipt,
        "request.canonical_request_ref",
        CasBlob,
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "outcome.normalized_output_ref",
        CasBlob,
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "interaction_manifest_ref",
        Kind(K::InteractionManifest),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "model_route_ref",
        CasBlob,
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "campaign_run_binding_ref.blob_ref",
        Kind(K::CampaignRunBinding),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "execution_cause.prior_verdict_ref",
        Kind(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.prior_receipt_ref",
        Kind(K::ProviderInvocationReceipt),
        Causal,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.evidence",
        ExternalWrapper,
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.evidence.classification_record_ref",
        CasBlob,
        Intra,
    ),
    // InteractionManifest
    e(
        K::InteractionManifest,
        "sequence.raw_provider_event_refs",
        CasBlob,
        Intra,
    ),
    e(
        K::InteractionManifest,
        "sequence.tool_argument_refs",
        CasBlob,
        Intra,
    ),
    e(
        K::InteractionManifest,
        "sequence.tool_result_refs",
        CasBlob,
        Intra,
    ),
    // CampaignFeedItem — informative projection feed
    e(
        K::CampaignFeedItem,
        "subject_refs",
        Kind(K::WorkOrder),
        Causal,
    ),
    e(
        K::CampaignFeedItem,
        "subject_refs.receipt",
        Kind(K::ProviderInvocationReceipt),
        Causal,
    ),
    e(
        K::CampaignFeedItem,
        "subject_refs.admission",
        Kind(K::CandidateAdmissionReceipt),
        Causal,
    ),
    e(
        K::CampaignFeedItem,
        "subject_refs.verdict",
        Kind(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::CampaignFeedItem,
        "subject_refs.attention",
        Kind(K::HumanAttentionRequest),
        Causal,
    ),
    // HumanAttentionRequest
    e(
        K::HumanAttentionRequest,
        "evidence_refs.receipt",
        Kind(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.gate",
        CasBlob,
        Intra,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.admission",
        Kind(K::CandidateAdmissionReceipt),
        Causal,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.verdict",
        Kind(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::HumanAttentionRequest,
        "candidate_state_ref",
        ExternalWrapper,
        Intra,
    ),
    // HumanCommandRequest (raw)
    e(
        K::HumanCommandRequest,
        "producer.authenticated_principal_ref",
        CasBlob,
        Intra,
    ),
    // HumanDecision
    e(
        K::HumanDecision,
        "source.blob_ref",
        Kind(K::HumanCommandRequest),
        Intra,
    ),
    e(
        K::HumanDecision,
        "producer.authenticated_principal_ref",
        CasBlob,
        Intra,
    ),
    e(
        K::HumanDecision,
        "attention_ref",
        Kind(K::HumanAttentionRequest),
        Intra,
    ),
    // ArtifactImported
    e(K::ArtifactImported, "cas_object_ref", CasBlob, Intra),
    e(K::ArtifactImported, "source", ExternalWrapper, Intra),
    // CampaignRunBinding
    e(
        K::CampaignRunBinding,
        "input.correspondence_ref",
        CasBlob,
        Intra,
    ),
    e(
        K::CampaignRunBinding,
        "input.state_refs",
        ExternalWrapper,
        Intra,
    ),
];

/// Attention transition records (class 4; appended by A2).
pub const TRANSITION_EDGES: &[(&str, &str)] = &[
    ("attention_acknowledged", "attention_ref+decision_ref"),
    ("attention_resolved", "attention_ref+decision_ref"),
    (
        "attention_superseded",
        "attention_ref+superseding_attention_ref",
    ),
];

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// Contract §11.1: edge tags form a closed registry — unique per
    /// (source, tag).
    #[test]
    fn edge_tags_are_unique_per_source() {
        let mut seen = BTreeSet::new();
        for edge in EDGES {
            assert!(
                seen.insert((edge.from, edge.tag)),
                "duplicate edge tag {:?} on {:?}",
                edge.tag,
                edge.from
            );
            assert_ne!(
                edge.tag, CAUSATION_TAG,
                "causation is the global rule, not a row"
            );
        }
    }

    /// Contract §11.4 item 1: the `intra` subgraph over KINDS must
    /// topologically sort; failure to sort is a build failure.
    #[test]
    fn intra_subgraph_topologically_sorts() {
        let mut adj: BTreeMap<MessageKind, Vec<MessageKind>> = BTreeMap::new();
        let mut indeg: BTreeMap<MessageKind, usize> = BTreeMap::new();
        for edge in EDGES {
            if edge.class != Intra {
                continue;
            }
            let (Kind(from), Kind(to)) = (edge.from, edge.to) else {
                continue; // CAS/wrapper targets are terminal.
            };
            adj.entry(from).or_default().push(to);
            *indeg.entry(to).or_default() += 1;
            indeg.entry(from).or_default();
        }
        let mut queue: Vec<MessageKind> = indeg
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut sorted = 0usize;
        while let Some(k) = queue.pop() {
            sorted += 1;
            for next in adj.get(&k).cloned().unwrap_or_default() {
                let d = indeg.entry(next).or_default();
                *d -= 1;
                if *d == 0 {
                    queue.push(next);
                }
            }
        }
        assert_eq!(sorted, indeg.len(), "intra kind-subgraph has a cycle");
    }

    /// The causal edges that §11.4 exempts from the kind-level sort are
    /// exactly the known set — a new causal edge is a supersede-path
    /// contract change, so this test pins the list.
    #[test]
    fn causal_edges_are_the_frozen_set() {
        let causal: BTreeSet<(&str, &str)> = EDGES
            .iter()
            .filter(|edge| edge.class == Causal)
            .map(|edge| {
                let Kind(from) = edge.from else {
                    panic!("causal edge from non-kind")
                };
                (kind_name(from), edge.tag)
            })
            .collect();
        let expected: BTreeSet<(&str, &str)> = [
            ("corrective_directive", "verdict_ref"),
            (
                "provider_invocation_receipt",
                "execution_cause.prior_verdict_ref",
            ),
            (
                "provider_invocation_receipt",
                "cause.safe_redrive.prior_receipt_ref",
            ),
            ("campaign_feed_item", "subject_refs"),
            ("campaign_feed_item", "subject_refs.receipt"),
            ("campaign_feed_item", "subject_refs.admission"),
            ("campaign_feed_item", "subject_refs.verdict"),
            ("campaign_feed_item", "subject_refs.attention"),
            ("human_attention_request", "evidence_refs.admission"),
            ("human_attention_request", "evidence_refs.verdict"),
        ]
        .into_iter()
        .collect();
        assert_eq!(causal, expected);
    }

    fn kind_name(k: MessageKind) -> &'static str {
        match k {
            MessageKind::WorkOrder => "work_order",
            MessageKind::CoderReport => "coder_report",
            MessageKind::CandidateAdmissionReceipt => "candidate_admission_receipt",
            MessageKind::ReviewRequest => "review_request",
            MessageKind::ReviewerReport => "reviewer_report",
            MessageKind::ReviewVerdict => "review_verdict",
            MessageKind::CorrectiveDirective => "corrective_directive",
            MessageKind::ProviderInvocationReceipt => "provider_invocation_receipt",
            MessageKind::InteractionManifest => "interaction_manifest",
            MessageKind::CampaignFeedItem => "campaign_feed_item",
            MessageKind::HumanAttentionRequest => "human_attention_request",
            MessageKind::HumanCommandRequest => "human_command_request",
            MessageKind::HumanDecision => "human_decision",
            MessageKind::CampaignRunBinding => "campaign_run_binding",
            MessageKind::ArtifactImported => "artifact_imported",
        }
    }

    /// Contract §5: every envelope kind with a byte ceiling appears in
    /// the kind-limit table under its snake_case name (two registries
    /// cannot drift apart silently).
    #[test]
    fn kind_limits_cover_the_limit_bearing_kinds() {
        let limited: BTreeSet<&str> = crate::limits::KIND_LIMITS.iter().map(|k| k.kind).collect();
        for kind in [
            K::WorkOrder,
            K::CoderReport,
            K::CandidateAdmissionReceipt,
            K::ReviewRequest,
            K::ReviewerReport,
            K::ReviewVerdict,
            K::CorrectiveDirective,
            K::ProviderInvocationReceipt,
            K::InteractionManifest,
            K::CampaignFeedItem,
            K::HumanAttentionRequest,
            K::HumanCommandRequest,
            K::HumanDecision,
        ] {
            assert!(
                limited.contains(kind_name(kind)),
                "no limit row for {kind:?}"
            );
        }
    }
}
