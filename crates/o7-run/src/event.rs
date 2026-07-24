//! The canonical, versioned run-event contract.
//!
//! A run's truth is an append-only sequence of [`RunEvent`]s. Each event is chained to
//! its predecessor by digest, so post-run modification, truncation, reordering, and
//! substitution are all detectable by recomputation alone (see [`crate::replay`]). The
//! event payloads are the ONLY canonical source of the verdict — the reducer never reads
//! unstructured agent stdout.
//!
//! Digests are computed by explicit field FRAMING (length-prefixed), not by hashing a
//! serialized JSON blob, so they are byte-stable regardless of map ordering or
//! serializer whitespace — a prerequisite for byte-stable replay.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ids::{GateId, RunEventId, RunId};

/// The schema version this build writes. A reader that encounters a newer version must
/// refuse to replay it as if it understood it (forward-incompatible changes bump this).
pub const RUN_EVENT_SCHEMA_VERSION: u32 = 1;

/// A lowercase-hex SHA-256 digest. Used both for event-chain links and for referenced
/// artifact content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Digest256(String);

impl Digest256 {
    /// Hash raw bytes (artifact content, or any opaque blob).
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hex_lower(&hasher.finalize()))
    }

    /// The genesis link: the `previous_event_digest` of the FIRST event in a run. A fixed
    /// all-zero digest, so a stream that does not begin at genesis is detectable.
    #[must_use]
    pub fn genesis() -> Self {
        Self("0".repeat(64))
    }

    /// Wrap an already-computed hex digest (e.g. read back from storage).
    #[must_use]
    pub fn from_hex(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The kind of artifact an event references. Artifacts are referenced by digest, never
/// duplicated into the event stream, so the forensic files (`diff.patch`, `gate/*.log`,
/// sandbox reports, task spec) stay the single copy and replay validates them in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// The task specification the run was started from.
    Task,
    /// The captured patch (`diff.patch`).
    Diff,
    /// A gate's captured log (`gate/<id>.log`).
    GateLog,
    /// A machine-readable sandbox report.
    SandboxReport,
    /// The materialized worktree identity.
    Worktree,
}

/// A reference to an out-of-band artifact: WHERE it is plus the digest its content MUST
/// hash to. Replay resolves the locator and fails loudly on any mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    /// An opaque, storage-relative locator (e.g. `diff.patch`, `gate/build.log`). The
    /// pure core never interprets it; a resolver maps it to bytes.
    pub locator: String,
    /// The digest the resolved content must equal.
    pub digest: Digest256,
}

/// The outcome a gate reported. Distinct from the run [`crate::state::Verdict`]: a single
/// gate can `Warn` or be `NotApplicable`, but the run-level verdict is one of four states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// The gate ran and passed.
    Pass,
    /// The gate ran and reported a domain failure (e.g. a non-zero exit).
    Fail,
    /// The gate ran, passed, but surfaced a non-blocking warning.
    Warn,
    /// The gate was refused by policy/environment (e.g. confinement missing).
    Blocked,
    /// The gate does not apply in this environment (e.g. a Windows check on Linux).
    NotApplicable,
    /// The gate could not produce a trustworthy result (could not start, crashed). This
    /// is a harness error, distinct from a domain `Fail`.
    Error,
}

/// How an agent process ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExit {
    /// Exited normally with this status code.
    Exited { code: i32 },
    /// Terminated by this signal.
    Signalled { signal: i32 },
    /// Never started (a harness error → run-level `Error`).
    FailedToStart { reason: String },
}

/// The obligations declared for a run BEFORE it executes. This is what lets an
/// environment-specific gate be legitimately skipped (declared optional up front) while a
/// required gate that simply did not run is `BLOCKED` — the distinction cannot be invented
/// after the fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContract {
    /// The gates that MUST execute and pass for the run to `PASS`.
    pub required_gates: Vec<GateId>,
    /// Required gates explicitly declared optional FOR THIS ENVIRONMENT before execution.
    /// A gate here that is skipped / `NotApplicable` does not block; one absent here does.
    pub optional_in_env: Vec<GateId>,
    /// Whether a valid sandbox-evidence artifact is required for this run to `PASS`.
    pub requires_sandbox_evidence: bool,
    /// The runner environment tag (e.g. `linux`). Opaque to the reducer except for
    /// equality against a gate's declared support.
    pub runner_environment: String,
}

/// One canonical run event's payload. The names are reviewable; the semantic distinctions
/// are load-bearing and must survive renaming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventKind {
    /// The run began; carries the pre-execution obligation contract. MUST be the first
    /// event of a run.
    RunStarted { contract: RunContract },
    /// The isolated worktree was materialized.
    WorktreeCreated { worktree: ArtifactRef },
    /// The agent process was spawned.
    AgentStarted,
    /// The agent process ended.
    AgentExited { exit: AgentExit },
    /// A patch was captured from the worktree.
    PatchCaptured { patch: ArtifactRef },
    /// A policy pre-check ran (e.g. path/network allowlist) with its allow/deny result.
    PolicyChecked { policy: String, allowed: bool },
    /// A gate began executing.
    GateStarted { gate: GateId },
    /// A gate finished, with its outcome, whether it is supported in this environment, and
    /// its captured log (if any).
    GateFinished {
        gate: GateId,
        outcome: GateOutcome,
        /// Whether this gate is applicable/supported in the run's environment. A required
        /// gate unsupported here reduces to `BLOCKED` unless declared optional-in-env.
        supported_in_env: bool,
        log: Option<ArtifactRef>,
    },
    /// A sandbox produced machine-readable confinement evidence, and whether it satisfies
    /// the run's policy.
    SandboxEvidenceCaptured {
        report: ArtifactRef,
        satisfies_policy: bool,
    },
    /// The run was sealed. MUST be the LAST event; the verdict is fixed here and no event
    /// may follow.
    RunSealed,
}

impl RunEventKind {
    /// A stable tag byte for digest framing (independent of serde renaming).
    #[must_use]
    pub(crate) fn tag(&self) -> u8 {
        match self {
            Self::RunStarted { .. } => 1,
            Self::WorktreeCreated { .. } => 2,
            Self::AgentStarted => 3,
            Self::AgentExited { .. } => 4,
            Self::PatchCaptured { .. } => 5,
            Self::PolicyChecked { .. } => 6,
            Self::GateStarted { .. } => 7,
            Self::GateFinished { .. } => 8,
            Self::SandboxEvidenceCaptured { .. } => 9,
            Self::RunSealed => 10,
        }
    }

    /// A stable human-readable name (telemetry / replay reports).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::WorktreeCreated { .. } => "worktree_created",
            Self::AgentStarted => "agent_started",
            Self::AgentExited { .. } => "agent_exited",
            Self::PatchCaptured { .. } => "patch_captured",
            Self::PolicyChecked { .. } => "policy_checked",
            Self::GateStarted { .. } => "gate_started",
            Self::GateFinished { .. } => "gate_finished",
            Self::SandboxEvidenceCaptured { .. } => "sandbox_evidence_captured",
            Self::RunSealed => "run_sealed",
        }
    }
}

/// A single canonical run event: the chained, digest-carrying envelope around a
/// [`RunEventKind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub event_id: RunEventId,
    pub schema_version: u32,
    pub run_id: RunId,
    /// Per-run monotonic sequence, starting at 0 for `RunStarted`.
    pub sequence: u64,
    /// The `event_digest` of the previous event, or [`Digest256::genesis`] for the first.
    pub previous_event_digest: Digest256,
    /// The digest of THIS event over all its fields except `event_digest` itself.
    pub event_digest: Digest256,
    /// Metadata only — NEVER the ordering key. Ordering is by `sequence`.
    pub timestamp_millis: i64,
    pub kind: RunEventKind,
}

impl RunEvent {
    /// Compute the canonical digest of this event from its fields (excluding the stored
    /// `event_digest`). Deterministic and total — pure field framing, no serialization,
    /// no panics.
    #[must_use]
    pub fn compute_digest(&self) -> Digest256 {
        let mut h = Sha256::new();
        h.update(b"o7-run-event\0v1\0");
        frame(&mut h, &self.schema_version.to_le_bytes());
        frame(&mut h, self.run_id.as_str().as_bytes());
        frame(&mut h, &self.sequence.to_le_bytes());
        frame(&mut h, self.previous_event_digest.as_str().as_bytes());
        frame(&mut h, &self.timestamp_millis.to_le_bytes());
        frame(&mut h, self.event_id.as_str().as_bytes());
        h.update([self.kind.tag()]);
        fold_kind(&mut h, &self.kind);
        Digest256(hex_lower(&h.finalize()))
    }

    /// Whether the stored `event_digest` matches a recomputation over the event's own
    /// fields — i.e. this single event has not been tampered with in place.
    #[must_use]
    pub fn digest_is_consistent(&self) -> bool {
        self.event_digest == self.compute_digest()
    }
}

/// Fold a [`RunEventKind`]'s fields into `h` with explicit framing.
fn fold_kind(h: &mut Sha256, kind: &RunEventKind) {
    match kind {
        RunEventKind::RunStarted { contract } => fold_contract(h, contract),
        RunEventKind::WorktreeCreated { worktree } => fold_artifact(h, worktree),
        RunEventKind::AgentStarted | RunEventKind::RunSealed => {}
        RunEventKind::AgentExited { exit } => fold_exit(h, exit),
        RunEventKind::PatchCaptured { patch } => fold_artifact(h, patch),
        RunEventKind::PolicyChecked { policy, allowed } => {
            frame(h, policy.as_bytes());
            h.update([u8::from(*allowed)]);
        }
        RunEventKind::GateStarted { gate } => frame(h, gate.as_str().as_bytes()),
        RunEventKind::GateFinished {
            gate,
            outcome,
            supported_in_env,
            log,
        } => {
            frame(h, gate.as_str().as_bytes());
            h.update([gate_outcome_tag(*outcome)]);
            h.update([u8::from(*supported_in_env)]);
            match log {
                Some(a) => {
                    h.update([1u8]);
                    fold_artifact(h, a);
                }
                None => h.update([0u8]),
            }
        }
        RunEventKind::SandboxEvidenceCaptured {
            report,
            satisfies_policy,
        } => {
            fold_artifact(h, report);
            h.update([u8::from(*satisfies_policy)]);
        }
    }
}

fn fold_contract(h: &mut Sha256, contract: &RunContract) {
    frame(h, &(contract.required_gates.len() as u64).to_le_bytes());
    for g in &contract.required_gates {
        frame(h, g.as_str().as_bytes());
    }
    frame(h, &(contract.optional_in_env.len() as u64).to_le_bytes());
    for g in &contract.optional_in_env {
        frame(h, g.as_str().as_bytes());
    }
    h.update([u8::from(contract.requires_sandbox_evidence)]);
    frame(h, contract.runner_environment.as_bytes());
}

fn fold_artifact(h: &mut Sha256, a: &ArtifactRef) {
    h.update([artifact_kind_tag(a.kind)]);
    frame(h, a.locator.as_bytes());
    frame(h, a.digest.as_str().as_bytes());
}

fn fold_exit(h: &mut Sha256, exit: &AgentExit) {
    match exit {
        AgentExit::Exited { code } => {
            h.update([1u8]);
            frame(h, &code.to_le_bytes());
        }
        AgentExit::Signalled { signal } => {
            h.update([2u8]);
            frame(h, &signal.to_le_bytes());
        }
        AgentExit::FailedToStart { reason } => {
            h.update([3u8]);
            frame(h, reason.as_bytes());
        }
    }
}

fn artifact_kind_tag(kind: ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::Task => 1,
        ArtifactKind::Diff => 2,
        ArtifactKind::GateLog => 3,
        ArtifactKind::SandboxReport => 4,
        ArtifactKind::Worktree => 5,
    }
}

fn gate_outcome_tag(outcome: GateOutcome) -> u8 {
    match outcome {
        GateOutcome::Pass => 1,
        GateOutcome::Fail => 2,
        GateOutcome::Warn => 3,
        GateOutcome::Blocked => 4,
        GateOutcome::NotApplicable => 5,
        GateOutcome::Error => 6,
    }
}

fn frame(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        if let (Some(&hi), Some(&lo)) =
            (HEX.get(usize::from(b >> 4)), HEX.get(usize::from(b & 0x0f)))
        {
            out.push(char::from(hi));
            out.push(char::from(lo));
        }
    }
    out
}
