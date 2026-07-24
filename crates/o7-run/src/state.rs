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
//! | `Blocked` | A required obligation could not be discharged (unrun / N/A / unsupported / |
//! |           | missing-required-sandbox-evidence) and was NOT declared optional up front. |
//! | `Error`   | The harness could not produce a trustworthy result (gate/agent could not   |
//! |           | run, or a gate reported `Error`). Distinct from a domain `Fail`.           |
//!
//! Precedence when several apply: **`Error` > `Fail` > `Blocked` > `Pass`**. `Error`
//! wins because nothing can be trusted; a concrete required-gate `Fail` is a definitive
//! negative and outranks mere incompleteness (`Blocked`); `Pass` requires the total
//! absence of the other three. Critically, `Pass` is reachable ONLY when every required
//! gate executed and passed — a required-but-unexecuted gate can never be `Pass`.
//!
//! Structural violations of the stream itself (duplicate/out-of-order/impossible/post-seal
//! events) are NOT verdicts — they make reduction fail loudly (see [`crate::reduce`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{Digest256, GateOutcome, RunContract, RUN_EVENT_SCHEMA_VERSION};
use crate::ids::GateId;

/// The version of the reduced-state normal form. Bumped when the state shape or the
/// verdict semantics change in a way that would alter a byte-stable replay.
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
#[serde(rename_all = "snake_case")]
pub enum GateProgress {
    /// `GateStarted` seen but not yet `GateFinished`. If the run seals here, the gate is
    /// an unfinished obligation → `Error` for a required gate.
    Started,
    /// `GateFinished` seen with this outcome and environment-support flag.
    Finished {
        outcome: GateOutcome,
        supported_in_env: bool,
    },
}

/// The pure, versioned reduced state. Serializes to a byte-stable normal form (a
/// `BTreeMap` keeps gate order deterministic), so replaying the same stream yields an
/// identical serialization — the anchor for byte-stable replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub schema_version: u32,
    pub phase: RunPhase,
    /// The pre-execution obligation contract, learned at `RunStarted`.
    pub contract: Option<RunContract>,
    /// The last sequence folded, for monotonicity checks (`None` before any event).
    pub last_sequence: Option<u64>,
    /// The `event_digest` of the last folded event, for chain-link checks. `genesis`
    /// before any event.
    pub last_event_digest: Digest256,
    /// Per-gate progress, keyed for deterministic ordering.
    pub gates: BTreeMap<GateId, GateProgress>,
    /// Whether valid, policy-satisfying sandbox evidence has been captured.
    pub sandbox_evidence_ok: bool,
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
            contract: None,
            last_sequence: None,
            last_event_digest: Digest256::genesis(),
            gates: BTreeMap::new(),
            sandbox_evidence_ok: false,
            verdict: None,
        }
    }

    /// Whether the run has been sealed (a verdict is fixed).
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        matches!(self.phase, RunPhase::Sealed)
    }
}

impl Default for RunState {
    fn default() -> Self {
        Self::initial()
    }
}

/// Confirm the schema constants are wired together at build time (the state normal form
/// embeds the event contract by reference).
const _: () = assert!(RUN_STATE_SCHEMA_VERSION == RUN_EVENT_SCHEMA_VERSION);
