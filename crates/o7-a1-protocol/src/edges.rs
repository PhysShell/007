//! The closed reference universe and the frozen per-kind allowed-edge
//! matrix (contract §11.1–§11.4). Review T3: targets are EXACT — a
//! specific envelope kind, a specific CAS `ContentKind`, a specific
//! external wrapper kind, a specific transition kind, or the one
//! contract-sanctioned open target (`AnyCommittedEnvelope`, the
//! CampaignFeedItem rule). The full registry is pinned by a literal
//! known-answer digest, so ANY edit to any row is a visible
//! supersede-path wire change.

use serde::{Deserialize, Serialize};

use crate::cas::ContentKind;

/// The 15 envelope-bearing message kinds (contract §11.1 class 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
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

/// Class-3 external wrapper kinds (contract §11.1, into A0/R1 records).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ExternalRefKind {
    CandidateStateReceipt,
    CandidateMaterialization,
    RunContractCandidateState,
    WorktreeMaterialization,
    RunArtifactSource,
    EstablishedNonDispatchEvidence,
}

/// Class-4 attention transition kinds (appended by A2; contract §8.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum TransitionKind {
    AttentionAcknowledged,
    AttentionResolved,
    AttentionSuperseded,
}

/// An edge SOURCE: an envelope kind or a transition record kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeSource {
    /// Envelope-bearing kind.
    Kind(MessageKind),
    /// Attention transition record.
    Transition(TransitionKind),
}

/// An edge TARGET — exact, per review T3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeTarget {
    /// A specific envelope-bearing kind.
    Envelope(MessageKind),
    /// A specific CAS content kind (terminal).
    Cas(ContentKind),
    /// A specific external wrapper kind (terminal for the A1 DAG).
    External(ExternalRefKind),
    /// The one sanctioned open target: any already-committed
    /// envelope-bearing artifact (CampaignFeedItem §11.3; causation).
    AnyCommittedEnvelope,
    /// Any IMPORTABLE kind from the closed CAS registry — sanctioned
    /// for EXACTLY one edge, `ArtifactImported.cas_object_ref`
    /// (T3-R1, narrowed by W4: `canonical-message-blob` is NOT
    /// importable — a canonical message arises only through acceptance
    /// construction, never import; `crate::cas::is_importable` is the
    /// authority).
    AnyImportableCas,
}

/// Edge classification (contract §11.3–§11.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeClass {
    /// Within one round's derivation flow — must topologically sort at
    /// KIND level.
    Intra,
    /// Crossing rounds/chains/attention lineage — instance-acyclic by
    /// create-before-reference (+ strictly-lower ordinal where
    /// applicable).
    Causal,
}

/// One frozen edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Source.
    pub from: EdgeSource,
    /// Stable snake_case tag equal to the carrying field path.
    pub tag: &'static str,
    /// Exact target.
    pub to: EdgeTarget,
    /// Classification.
    pub class: EdgeClass,
}

use ContentKind as C;
use EdgeClass::{Causal, Intra};
use EdgeSource::{Kind, Transition};
use EdgeTarget::{AnyCommittedEnvelope, AnyImportableCas, Cas, Envelope, External};
use ExternalRefKind as X;
use MessageKind as K;
use TransitionKind as T;

const fn e(from: MessageKind, tag: &'static str, to: EdgeTarget, class: EdgeClass) -> Edge {
    Edge {
        from: Kind(from),
        tag,
        to,
        class,
    }
}

const fn t(from: TransitionKind, tag: &'static str, to: EdgeTarget, class: EdgeClass) -> Edge {
    Edge {
        from: Transition(from),
        tag,
        to,
        class,
    }
}

/// The global causation edge (contract §3): every envelope kind carries
/// exactly one, class `causal`, target [`EdgeTarget::AnyCommittedEnvelope`]
/// (or `CampaignGenesis`, first WorkOrder only — no edge).
pub const CAUSATION_TAG: &str = "causation.blob_ref";

/// The frozen §11.3 matrix — EXHAUSTIVE, exact targets, including
/// producer-binding, cause, and class-4 transition edges.
pub const EDGES: &[Edge] = &[
    // WorkOrder
    e(
        K::WorkOrder,
        "goal.contract_blob",
        Cas(C::ContractBlob),
        Intra,
    ),
    e(
        K::WorkOrder,
        "input.initial.correspondence_ref",
        Cas(C::WorktreeCorrespondenceEvidenceBlob),
        Intra,
    ),
    e(
        K::WorkOrder,
        "input.initial.run_contract_ref",
        External(X::RunContractCandidateState),
        Intra,
    ),
    e(
        K::WorkOrder,
        "input.initial.worktree_ref",
        External(X::WorktreeMaterialization),
        Intra,
    ),
    e(
        K::WorkOrder,
        "input.continued.candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    e(
        K::WorkOrder,
        "input.continued.materialization_ref",
        External(X::CandidateMaterialization),
        Intra,
    ),
    // CoderReport (raw)
    e(
        K::CoderReport,
        "producer.invocation_receipt_ref",
        Envelope(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(
        K::CoderReport,
        "claims.evidence_refs.gate",
        Cas(C::GateEvidenceBlob),
        Intra,
    ),
    e(
        K::CoderReport,
        "claims.evidence_refs.diff",
        Cas(C::DiffEvidenceBlob),
        Intra,
    ),
    // CandidateAdmissionReceipt
    e(
        K::CandidateAdmissionReceipt,
        "coder_report_ref",
        Envelope(K::CoderReport),
        Intra,
    ),
    e(
        K::CandidateAdmissionReceipt,
        "coder_run_binding_ref.blob_ref",
        Envelope(K::CampaignRunBinding),
        Intra,
    ),
    e(
        K::CandidateAdmissionReceipt,
        "candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    // ReviewRequest — NO separate CoderReport edge (§8.4)
    e(
        K::ReviewRequest,
        "admission_receipt_ref",
        Envelope(K::CandidateAdmissionReceipt),
        Intra,
    ),
    e(
        K::ReviewRequest,
        "contract_blob",
        Cas(C::ContractBlob),
        Intra,
    ),
    e(
        K::ReviewRequest,
        "evidence_refs.gate",
        Cas(C::GateEvidenceBlob),
        Intra,
    ),
    e(
        K::ReviewRequest,
        "evidence_refs.diff",
        Cas(C::DiffEvidenceBlob),
        Intra,
    ),
    // ReviewerReport (raw)
    e(
        K::ReviewerReport,
        "producer.invocation_receipt_ref",
        Envelope(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(
        K::ReviewerReport,
        "evidence_refs.gate",
        Cas(C::GateEvidenceBlob),
        Intra,
    ),
    // ReviewVerdict
    e(
        K::ReviewVerdict,
        "reviewer_report_ref",
        Envelope(K::ReviewerReport),
        Intra,
    ),
    e(
        K::ReviewVerdict,
        "findings.evidence_refs.gate",
        Cas(C::GateEvidenceBlob),
        Intra,
    ),
    e(
        K::ReviewVerdict,
        "reviewed_candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    // CorrectiveDirective
    e(
        K::CorrectiveDirective,
        "verdict_ref",
        Envelope(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::CorrectiveDirective,
        "input.continued.candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    e(
        K::CorrectiveDirective,
        "input.continued.materialization_ref",
        External(X::CandidateMaterialization),
        Intra,
    ),
    // ProviderInvocationReceipt (Controller-produced, §11.2)
    e(
        K::ProviderInvocationReceipt,
        "request.canonical_request_ref",
        Cas(C::CanonicalRequestBlob),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "outcome.normalized_output_ref",
        Cas(C::NormalizedOutputBlob),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "interaction_manifest_ref",
        Envelope(K::InteractionManifest),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "model_route_ref",
        Cas(C::ModelRouteBlob),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "campaign_run_binding_ref.blob_ref",
        Envelope(K::CampaignRunBinding),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "execution_cause.prior_verdict_ref",
        Envelope(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.prior_run_binding_ref",
        Envelope(K::CampaignRunBinding),
        Causal,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.evidence",
        External(X::EstablishedNonDispatchEvidence),
        Intra,
    ),
    e(
        K::ProviderInvocationReceipt,
        "cause.safe_redrive.evidence.classification_record_ref",
        Cas(C::NonDispatchClassificationBlob),
        Intra,
    ),
    // InteractionManifest
    e(
        K::InteractionManifest,
        "sequence.raw_provider_event_refs",
        Cas(C::RawProviderEventBlob),
        Intra,
    ),
    e(
        K::InteractionManifest,
        "sequence.tool_argument_refs",
        Cas(C::ToolArgumentBlob),
        Intra,
    ),
    e(
        K::InteractionManifest,
        "sequence.tool_result_refs",
        Cas(C::ToolResultBlob),
        Intra,
    ),
    // CampaignFeedItem — the contract's own open rule, honestly typed
    e(
        K::CampaignFeedItem,
        "subject_refs",
        AnyCommittedEnvelope,
        Causal,
    ),
    // HumanAttentionRequest
    e(
        K::HumanAttentionRequest,
        "evidence_refs.receipt",
        Envelope(K::ProviderInvocationReceipt),
        Intra,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.gate",
        Cas(C::GateEvidenceBlob),
        Intra,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.admission",
        Envelope(K::CandidateAdmissionReceipt),
        Causal,
    ),
    e(
        K::HumanAttentionRequest,
        "evidence_refs.verdict",
        Envelope(K::ReviewVerdict),
        Causal,
    ),
    e(
        K::HumanAttentionRequest,
        "candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    // HumanCommandRequest (raw)
    e(
        K::HumanCommandRequest,
        "producer.authenticated_principal_ref",
        Cas(C::AuthenticatedPrincipalRecord),
        Intra,
    ),
    // HumanDecision
    e(
        K::HumanDecision,
        "source.blob_ref",
        Envelope(K::HumanCommandRequest),
        Intra,
    ),
    e(
        K::HumanDecision,
        "producer.authenticated_principal_ref",
        Cas(C::AuthenticatedPrincipalRecord),
        Intra,
    ),
    e(
        K::HumanDecision,
        "attention_ref",
        Envelope(K::HumanAttentionRequest),
        Intra,
    ),
    // ArtifactImported
    e(
        K::ArtifactImported,
        "cas_object_ref",
        AnyImportableCas,
        Intra,
    ),
    e(
        K::ArtifactImported,
        "source",
        External(X::RunArtifactSource),
        Intra,
    ),
    // CampaignRunBinding
    e(
        K::CampaignRunBinding,
        "input.initial.correspondence_ref",
        Cas(C::WorktreeCorrespondenceEvidenceBlob),
        Intra,
    ),
    e(
        K::CampaignRunBinding,
        "input.initial.run_contract_ref",
        External(X::RunContractCandidateState),
        Intra,
    ),
    e(
        K::CampaignRunBinding,
        "input.initial.worktree_ref",
        External(X::WorktreeMaterialization),
        Intra,
    ),
    e(
        K::CampaignRunBinding,
        "input.continued.candidate_state_ref",
        External(X::CandidateStateReceipt),
        Intra,
    ),
    e(
        K::CampaignRunBinding,
        "input.continued.materialization_ref",
        External(X::CandidateMaterialization),
        Intra,
    ),
    // Class-4 transitions (§8.7; appended by A2)
    t(
        T::AttentionAcknowledged,
        "attention_ref",
        Envelope(K::HumanAttentionRequest),
        Intra,
    ),
    t(
        T::AttentionAcknowledged,
        "decision_ref",
        Envelope(K::HumanDecision),
        Intra,
    ),
    t(
        T::AttentionResolved,
        "attention_ref",
        Envelope(K::HumanAttentionRequest),
        Intra,
    ),
    t(
        T::AttentionResolved,
        "decision_ref",
        Envelope(K::HumanDecision),
        Intra,
    ),
    t(
        T::AttentionSuperseded,
        "attention_ref",
        Envelope(K::HumanAttentionRequest),
        Intra,
    ),
    t(
        T::AttentionSuperseded,
        "superseding_attention_ref",
        Envelope(K::HumanAttentionRequest),
        Causal,
    ),
];

/// Producer class required per envelope kind (contract §11.2) — the
/// wire-shape half of "invalid producer combination is unrepresentable"
/// (review T4). `Provider` rows also name the role the receipt chain
/// must yield (a classifier obligation, not a binding field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProducerClass {
    /// Controller-derived.
    Controller,
    /// Provider raw report; the receipt chain must yield this role.
    Provider(&'static str),
    /// Human raw command.
    Human,
}

/// The frozen §11.2 mapping.
#[must_use]
pub fn required_producer(kind: MessageKind) -> ProducerClass {
    match kind {
        K::CoderReport => ProducerClass::Provider("coder"),
        K::ReviewerReport => ProducerClass::Provider("reviewer"),
        K::HumanCommandRequest => ProducerClass::Human,
        _ => ProducerClass::Controller,
    }
}

/// Whether the kind is round-scoped (must carry the §2.3 pair).
#[must_use]
pub fn is_round_scoped(kind: MessageKind) -> bool {
    !matches!(
        kind,
        K::CampaignFeedItem | K::HumanCommandRequest | K::HumanDecision | K::ArtifactImported
    )
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn source_name(s: EdgeSource) -> String {
        format!("{s:?}")
    }

    #[test]
    fn edge_tags_are_unique_per_source() {
        let mut seen = BTreeSet::new();
        for edge in EDGES {
            assert!(
                seen.insert((source_name(edge.from), edge.tag)),
                "duplicate edge tag {:?} on {:?}",
                edge.tag,
                edge.from
            );
            assert_ne!(edge.tag, CAUSATION_TAG);
        }
    }

    /// Review T3: the FULL registry — (source, tag, target, class) rows
    /// plus the causation rule — is pinned by a literal digest. Any
    /// edit to any row is a visible supersede-path wire change.
    #[test]
    fn full_registry_known_answer() {
        let mut rows: Vec<String> = EDGES
            .iter()
            .map(|edge| {
                format!(
                    "{:?}|{}|{:?}|{:?}",
                    edge.from, edge.tag, edge.to, edge.class
                )
            })
            .collect();
        rows.push(format!(
            "GLOBAL|{CAUSATION_TAG}|AnyCommittedEnvelope|Causal"
        ));
        rows.sort();
        let joined = rows.join("\n");
        assert_eq!(
            crate::ids::BlobDigest::of_bytes(joined.as_bytes()).as_str(),
            REGISTRY_KAT,
            "edge registry changed — this is a supersede-path wire change; \
             re-pin ONLY alongside the contract amendment"
        );
    }

    const REGISTRY_KAT: &str = "53c074cc8e650751e3a95ff0badbf4b3e9d3b1391352ef94e227a15336a14ef7";

    #[test]
    fn intra_subgraph_topologically_sorts() {
        let mut adj: BTreeMap<MessageKind, Vec<MessageKind>> = BTreeMap::new();
        let mut indeg: BTreeMap<MessageKind, usize> = BTreeMap::new();
        for edge in EDGES {
            if edge.class != Intra {
                continue;
            }
            let (Kind(from), Envelope(to)) = (edge.from, edge.to) else {
                continue; // terminal targets and transition sources
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

    #[test]
    fn any_committed_envelope_is_feed_and_causation_only() {
        for edge in EDGES {
            if edge.to == AnyCommittedEnvelope {
                assert_eq!(edge.from, Kind(K::CampaignFeedItem), "open target leaked");
                assert_eq!(edge.class, Causal);
            }
        }
    }

    /// Re-review T3-R1/W4: the open-within-closed-registry CAS target
    /// is sanctioned for exactly one edge and must never leak.
    #[test]
    fn any_importable_cas_is_artifact_import_only() {
        let holders: Vec<_> = EDGES.iter().filter(|e| e.to == AnyImportableCas).collect();
        assert_eq!(holders.len(), 1, "AnyImportableCas leaked");
        let only = holders.first().unwrap();
        assert_eq!(only.from, Kind(K::ArtifactImported));
        assert_eq!(only.tag, "cas_object_ref");
        assert_eq!(only.class, Intra);
    }

    #[test]
    fn producer_mapping_and_round_scope_are_frozen() {
        assert_eq!(
            required_producer(K::CoderReport),
            ProducerClass::Provider("coder")
        );
        assert_eq!(
            required_producer(K::ProviderInvocationReceipt),
            ProducerClass::Controller,
            "the receipt is Controller-produced (§11.2) — a Provider \
             receipt would eat its own tail"
        );
        assert_eq!(
            required_producer(K::HumanCommandRequest),
            ProducerClass::Human
        );
        assert!(is_round_scoped(K::WorkOrder));
        assert!(!is_round_scoped(K::HumanCommandRequest));
    }

    /// Contract §5: every limit-bearing kind appears in the limits table
    /// under its snake_case serde name.
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
            let name = serde_json::to_value(kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            assert!(limited.contains(name.as_str()), "no limit row for {kind:?}");
        }
    }
}
