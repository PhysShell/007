//! The versioned run STATE and the four-state verdict the reducer folds toward.
//!
//! # Verdict transition table (the load-bearing contract)
//!
//! A run reduces to exactly one of four verdicts, fixed at `RunSealed`:
//!
//! | Verdict   | Meaning                                                                    |
//! |-----------|----------------------------------------------------------------------------|
//! | `Pass`    | Every required obligation was actually discharged and passed.              |
//! | `Fail`    | A required gate ran and reported a domain failure.                         |
//! | `Blocked` | A required obligation could not be discharged (unrun / N/A / blocked /     |
//! |           | policy-denied / missing-required-sandbox-evidence) and was NOT waived.     |
//! | `Error`   | The harness could not produce a trustworthy result (gate/agent could not   |
//! |           | run, an agent boundary/observation/output/CLEANUP fault, a gate reported   |
//! |           | `Error`, a policy engine `Error`, or a required gate left unfinished).     |
//!
//! Precedence when several apply: **`Error` > `Fail` > `Blocked` > `Pass`**. `Error` wins
//! because nothing can be trusted; a concrete required-gate `Fail` is a definitive negative
//! and outranks mere incompleteness (`Blocked`); `Pass` requires the total absence of the
//! other three. The state RETAINS the failed AND the blocked obligations (not only the
//! collapsed verdict), so a reviewer can see every unmet obligation, not just the winner.
//!
//! `Pass` is reachable ONLY when every required, applicable gate executed and passed, every
//! required-and-not-waived obligation is discharged, and every sandbox requirement has
//! matching `Satisfied` evidence. A required-but-unexecuted gate can never be `Pass`.
//!
//! Structural violations of the stream itself (duplicate/out-of-order/impossible/post-seal/
//! wrong-run-id/duplicate-event-id/schema/…) are NOT verdicts — they make reduction fail
//! loudly (see [`crate::reduce`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::event::{
    AgentOutcome, ArtifactRef, Digest256, GateOutcome, PolicyOutcome, RunContract,
    SandboxEvidenceOutcome, RUN_EVENT_SCHEMA_VERSION,
};
use crate::ids::{GateId, RunEventId, RunId};

/// The version of the reduced-state normal form. Bumped when the state shape or the verdict
/// semantics change in a way that would alter a byte-stable replay.
pub const RUN_STATE_SCHEMA_VERSION: u32 = 1;

/// The overall run verdict — the whole point of the reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
    Blocked,
    Error,
}

/// Where a run is in its lifecycle. A verdict exists only once `Sealed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    /// No `RunStarted` seen yet.
    NotStarted,
    /// Between `RunStarted` and `RunSealed`.
    Running,
    /// `RunSealed` seen; the verdict is fixed and no further event is permitted.
    Sealed,
}

/// The progress of a single gate as folded from the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "progress", rename_all = "snake_case")]
pub enum GateProgress {
    /// `GateStarted` seen but not yet `GateFinished`. If the run seals here, a required gate
    /// is an unfinished obligation → `Error`.
    Started,
    /// `GateFinished` seen with this observed outcome.
    Finished { outcome: GateOutcome },
}

/// The agent lifecycle as folded from the stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "agent", rename_all = "snake_case")]
pub enum AgentLifecycle {
    /// No `AgentStarted`/`AgentExited` seen.
    NotObserved,
    /// `AgentStarted` seen, not yet exited.
    Started,
    /// `AgentExited` seen with this terminal outcome.
    Exited { outcome: AgentOutcome },
}

/// A recorded policy check, retained so a denial/fault is visible in the state, not only in
/// the collapsed verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyResult {
    pub policy: ArtifactRef,
    pub outcome: PolicyOutcome,
}

/// The pure, versioned reduced state. Serializes to a byte-stable normal form (`BTreeMap`/
/// `BTreeSet` keep ordering deterministic), so replaying the same stream yields an
/// identical serialization — the anchor for byte-stable replay and for
/// [`RunState::normalized_digest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub schema_version: u32,
    pub phase: RunPhase,
    /// The run id this state is bound to, learned at `RunStarted`. Every later event must
    /// carry the same id.
    pub run_id: Option<RunId>,
    /// The pre-execution obligation contract, learned at `RunStarted`.
    pub contract: Option<RunContract>,
    /// The task the run executes for (evidence, digest-verified in replay).
    pub task: Option<ArtifactRef>,
    /// The materialized worktree (evidence-only for the verdict).
    pub worktree: Option<ArtifactRef>,
    /// The captured patch (evidence-only for the verdict).
    pub patch: Option<ArtifactRef>,
    /// The agent lifecycle/terminal outcome (verdict-bearing on a fault).
    pub agent: AgentLifecycle,
    /// The last sequence folded, for monotonicity checks (`None` before any event).
    pub last_sequence: Option<u64>,
    /// The `event_digest` of the last folded event, for chain-link checks. `genesis` before
    /// any event.
    pub last_event_digest: Digest256,
    /// Event ids already seen, so a repeat is a structural error.
    pub seen_event_ids: BTreeSet<RunEventId>,
    /// Per-gate progress, keyed for deterministic ordering.
    pub gates: BTreeMap<GateId, GateProgress>,
    /// Policy checks in stream order (verdict-bearing on `Denied`/`Error`).
    pub policy_results: Vec<PolicyResult>,
    /// Captured sandbox evidence, keyed by a canonical `subject|policy_digest` string so a
    /// requirement is only satisfied by evidence that matches its subject AND policy.
    pub sandbox_evidence: BTreeMap<String, SandboxEvidenceOutcome>,
    /// The fixed verdict, present only once `Sealed`.
    pub verdict: Option<Verdict>,
}

impl RunState {
    /// The empty initial state, before any event is folded.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            schema_version: RUN_STATE_SCHEMA_VERSION,
            phase: RunPhase::NotStarted,
            run_id: None,
            contract: None,
            task: None,
            worktree: None,
            patch: None,
            agent: AgentLifecycle::NotObserved,
            last_sequence: None,
            last_event_digest: Digest256::genesis(),
            seen_event_ids: BTreeSet::new(),
            gates: BTreeMap::new(),
            policy_results: Vec::new(),
            sandbox_evidence: BTreeMap::new(),
            verdict: None,
        }
    }

    /// Whether the run has been sealed (a verdict is fixed).
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        matches!(self.phase, RunPhase::Sealed)
    }

    /// A digest over the byte-stable normalized state serialization. Stable across replays
    /// of the same stream, so it can be anchored in an external journal/signature to detect
    /// a fully-recomputed (chain-consistent) rewrite that the hash chain alone cannot.
    ///
    /// # Errors
    /// Returns `None` if the state cannot be serialized (it always can for well-formed
    /// state; kept fallible rather than panicking to honor the no-panic discipline).
    #[must_use]
    pub fn normalized_digest(&self) -> Option<Digest256> {
        let json = serde_json::to_vec(self).ok()?;
        let mut h = Sha256::new();
        h.update(b"o7-run-state\0v1\0");
        h.update(&json);
        Some(Digest256::of_bytes(&h.finalize()))
    }
}

impl Default for RunState {
    fn default() -> Self {
        Self::initial()
    }
}

/// The state normal form embeds the event contract by reference; keep the versions locked
/// together at build time.
const _: () = assert!(RUN_STATE_SCHEMA_VERSION == RUN_EVENT_SCHEMA_VERSION);
