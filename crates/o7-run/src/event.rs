//! The canonical, versioned run-event contract.
//!
//! A run's truth is an append-only sequence of [`RunEvent`]s. Each event is chained to
//! its predecessor by digest. The chain detects any modification that was NOT accompanied
//! by a consistent recomputation of every downstream digest — deletion, truncation,
//! reordering, substitution, and in-place edits. It is NOT authenticity: an adversary who
//! can rewrite the WHOLE stream and recompute the chain is out of scope (see the
//! `#[non_goal]` on remote attestation in issue #43). To anchor a stream against such an
//! adversary, [`crate::replay::ReplayReport`] exposes the final event digest and the
//! normalized-state digest for an external journal/signature.
//!
//! Digests are computed by explicit field FRAMING (length-prefixed), not by hashing a
//! serialized JSON blob, so they are byte-stable regardless of map ordering or serializer
//! whitespace — a prerequisite for byte-stable replay. Values read back from storage are
//! UNTRUSTED: [`Digest256`] and the [`crate::ids`] newtypes validate their form on
//! deserialization so a malformed stored value cannot masquerade as a valid one.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ids::{GateId, RunEventId, RunId};

/// The schema version this build writes. A reader that encounters a different version must
/// refuse to replay it as if it understood it (see [`crate::reduce::ReduceError`]).
pub const RUN_EVENT_SCHEMA_VERSION: u32 = 1;

/// A validated lowercase-hex SHA-256 digest (exactly 64 hex chars). Used both for
/// event-chain links and for referenced artifact content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Digest256(String);

/// A string that is not a canonical 64-char lowercase-hex SHA-256 digest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a canonical sha-256 digest (need 64 lowercase hex chars): {value:?}")]
pub struct DigestFormatError {
    pub value: String,
}

impl Digest256 {
    /// Hash raw bytes (artifact content, or any opaque blob). Always canonical.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hex_lower(&hasher.finalize()))
    }

    /// The genesis link: the `previous_event_digest` of the FIRST event in a run. A fixed
    /// all-zero (canonical) digest, so a stream that does not begin at genesis is
    /// detectable.
    #[must_use]
    pub fn genesis() -> Self {
        Self("0".repeat(64))
    }

    /// Parse and validate a hex digest string (untrusted input path).
    ///
    /// # Errors
    /// [`DigestFormatError`] if `s` is not exactly 64 lowercase hex characters.
    pub fn parse(s: &str) -> Result<Self, DigestFormatError> {
        if is_canonical_hex(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(DigestFormatError {
                value: s.to_owned(),
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Wrap the raw 32 bytes of a SHA-256 finalize as a canonical digest, WITHOUT hashing
    /// again. Crate-internal so external callers still go through validated constructors;
    /// used to anchor a single domain-separated hash (never a double hash).
    #[must_use]
    pub(crate) fn from_sha256(raw: &[u8]) -> Self {
        Self(hex_lower(raw))
    }
}

impl fmt::Display for Digest256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Digest256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

fn is_canonical_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The kind of artifact an event references. Artifacts are referenced by digest, never
/// duplicated into the event stream, so the forensic files (`task.json`, `diff.patch`,
/// `gate/*.log`, sandbox reports) stay the single copy and replay validates them in place.
/// Each slot in [`RunEventKind`] requires a SPECIFIC kind — a mismatch is a structural
/// error, not a silent accept.
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
    /// A policy definition, bound by digest so a run proves WHICH policy it checked.
    Policy,
    /// Q-Deck R1 fifth corrective round: a durable, canonical receipt of the
    /// provider session identity a successful agent response returned —
    /// see [`RunEventKind::ProviderSessionCaptured`]. The receipt's own
    /// content (not defined by this crate — an untyped domain payload from
    /// the caller's point of view) is bound by digest exactly like any
    /// other artifact.
    ProviderSession,
    /// Q-Deck R1 fifth corrective round: a durable receipt binding this
    /// canonical record to the EXACT accepted Command that dispatched it
    /// (`command_id`/`conversation_id`/`parent_run_id`/`child_run_id`/a
    /// digest of the command text) — see
    /// [`RunEventKind::CommandBindingCaptured`].
    CommandBinding,
}

/// A reference to an out-of-band artifact: WHERE it is plus the digest its content MUST
/// hash to. Replay resolves the locator and fails loudly on any mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    /// An opaque, storage-relative locator (e.g. `diff.patch`, `gate/build.log`). The pure
    /// core never interprets it; a resolver maps it to bytes.
    pub locator: String,
    /// The digest the resolved content must equal.
    pub digest: Digest256,
}

/// The outcome a gate reported. Distinct from the run [`crate::state::Verdict`]: a single
/// gate can `Warn` or be `NotApplicable`, but the run-level verdict is one of four states.
/// A gate NEVER declares its own applicability here — that is fixed pre-run in the
/// [`RunContract`]; this is only the OBSERVED result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// The gate ran and passed.
    Pass,
    /// The gate ran and reported a domain failure (e.g. a non-zero exit).
    Fail,
    /// The gate ran, passed, but surfaced a non-blocking warning.
    Warn,
    /// The gate was refused by policy/environment at run time.
    Blocked,
    /// The gate reported that it does not apply in this environment.
    NotApplicable,
    /// The gate could not produce a trustworthy result (could not start, crashed). A
    /// harness error, distinct from a domain `Fail`.
    Error,
}

/// The outcome of a policy pre-check. `Denied` (a normal policy denial) is deliberately
/// distinct from `Error` (a broken policy engine) — collapsing them into a boolean would
/// hide a fault as a denial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    Allowed,
    Denied,
    Error,
}

/// Which execution a sandbox obligation/evidence is about. Confinement evidence is bound to
/// a SUBJECT so one blanket "confined: true" cannot satisfy every requirement. `Ord` so it
/// can key a typed sandbox-evidence identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "subject", rename_all = "snake_case")]
pub enum ExecutionSubject {
    /// The agent process.
    Agent,
    /// A specific gate.
    Gate { gate: GateId },
    /// The trusted verifier.
    Verifier,
}

/// The outcome of validating a sandbox report against its policy. `Violated` (confinement
/// breached) is distinct from `Error` (the report could not be validated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxEvidenceOutcome {
    /// The report satisfies the required policy.
    Satisfied,
    /// The report shows the policy was violated.
    Violated,
    /// The report could not be validated (a harness error).
    Error,
}

/// A required sandbox obligation declared BEFORE execution: some subject must run under a
/// specific policy (bound by digest) and produce satisfying evidence for the run to `Pass`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxRequirement {
    pub subject: ExecutionSubject,
    pub policy_digest: Digest256,
}

/// Whether a gate is required or optional for the run's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateRequirement {
    /// Must be discharged (executed and passed) or the run is not `Pass`.
    Required,
    /// Informational; never blocks the verdict.
    Optional,
}

/// Whether a gate applies in THIS run's environment. A `Waived` gate was declared skippable
/// for this environment BEFORE execution — the only way a required gate may be legitimately
/// skipped. This is fixed at `RunStarted`; nothing observed at run time can grant it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "applicability", rename_all = "snake_case")]
pub enum GateApplicability {
    /// The gate applies and must be run.
    Applicable,
    /// The gate is waived for this environment, with an auditable reason.
    Waived { environment: String, reason: String },
}

/// One pre-declared gate obligation. A gate appears in EXACTLY one obligation, so the
/// contradictory "required, except when optional" overlap is unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateObligation {
    pub gate: GateId,
    pub requirement: GateRequirement,
    pub applicability: GateApplicability,
}

/// Whether a policy check is required for the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyRequirement {
    /// Must be checked and `Allowed` (or the run is not `Pass`).
    Required,
    /// Informational; never blocks the verdict.
    Optional,
}

/// A subject a policy can PROTECT — restricted to subjects that have a canonical START event
/// (`AgentStarted`, `GateStarted`). Deliberately NOT [`ExecutionSubject`], which includes
/// `Verifier`: there is no `VerifierStarted` event, so a policy could otherwise promise a
/// temporal ordering the reducer physically cannot enforce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protected", rename_all = "snake_case")]
pub enum PolicyProtectedSubject {
    /// The agent (its start is `AgentStarted`).
    Agent,
    /// A specific gate (its start is `GateStarted`).
    Gate { gate: GateId },
}

/// A pre-declared policy obligation: which policy (bound by digest) must be checked, whether
/// it is required, and which subjects it PROTECTS. A protected subject may not start before
/// this policy has been checked `Allowed` — so a stream cannot faithfully record that the
/// horse left before the barn door was inspected. A non-empty `protects` is permitted ONLY
/// on a `Required` policy (an optional pre-check cannot both be skippable and gate a start).
/// Matched to a `PolicyChecked` event by the policy artifact's digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyObligation {
    /// The policy definition (must be `ArtifactKind::Policy`).
    pub policy: ArtifactRef,
    pub requirement: PolicyRequirement,
    /// Subjects whose start must be preceded by an `Allowed` check of this policy.
    pub protects: Vec<PolicyProtectedSubject>,
}

/// Whether a canonical run must run an agent. Declared up front so an agent cannot be made
/// optional by omission — a run that loses both agent events during integration is caught,
/// not silently passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "agent", rename_all = "snake_case")]
pub enum AgentObligation {
    /// Exactly one terminal agent outcome is required; a clean `ExitedNormally { code: 0 }`
    /// is neutral, anything else is `Error`.
    Required,
    /// No agent runs in this canonical run (e.g. an evidence-only or gate-only run), with an
    /// auditable reason. Any agent event is then a structural error.
    NotUsed { reason: String },
}

/// The obligations declared for a run BEFORE it executes: which gates must pass, which
/// policies must be checked, which confinement must be evidenced, whether an agent runs, and
/// the environment. Requirement/applicability are fixed here — they cannot be invented after
/// the fact from observed events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContract {
    /// Every gate obligation, keyed by a unique gate id (duplicates are a structural error).
    pub gate_obligations: Vec<GateObligation>,
    /// Every policy obligation, keyed by a unique policy digest (duplicates are a structural
    /// error).
    pub policy_obligations: Vec<PolicyObligation>,
    /// Sandbox obligations that must each be satisfied by matching evidence (duplicates are a
    /// structural error).
    pub sandbox_requirements: Vec<SandboxRequirement>,
    /// Whether an agent must run.
    pub agent: AgentObligation,
    /// The runner environment tag (e.g. `linux`). Opaque to the reducer except for matching a
    /// gate waiver's declared environment.
    pub runner_environment: String,
}

/// How the agent execution ended. This mirrors the frozen o7-worker `WorkerResult`
/// categories exactly — collapsing them would erase load-bearing distinctions. In
/// particular a `CleanupFailure` (owned process set not proven gone) MUST force the run to
/// `Error` even when the leader exited 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AgentOutcome {
    /// The leader exited with this code.
    ExitedNormally { code: i32 },
    /// The leader was terminated by this signal.
    ExitedBySignal { signal: i32 },
    /// Cancelled; the whole owned group drained within the grace period.
    CancelledGracefully,
    /// Cancelled; at least one member required a force-kill.
    CancelledForcefully,
    /// The process never started (bad spec/policy, spawn error, boundary unmet).
    FailedToStart { reason: String },
    /// A boundary mechanism failed during the run.
    BoundaryFailure { reason: String },
    /// The authoritative observation sink failed; the worker was stopped.
    ObservationFailure { reason: String },
    /// A stdout/stderr read failed; output faithfulness was lost.
    OutputFailure { reason: String },
    /// The owned group could not be proven gone; a failure even if the leader exited
    /// cleanly.
    CleanupFailure { reason: String },
}

/// One canonical run event's payload.
///
/// EVENT CLASSIFICATION (also enforced by `tests/reducer_transitions.rs`):
/// - **Verdict-bearing:** `RunStarted` (obligations), `AgentExited` (any non-`ExitedNormally
///   {code:0}` outcome forces `Error`), `PolicyChecked`, `GateFinished`,
///   `SandboxEvidenceCaptured`, `RunSealed` (finalizes).
/// - **Lifecycle/structural:** `AgentStarted`, `GateStarted`, `RunSealed`.
/// - **Evidence-only (verdict-neutral, but its artifact IS digest-verified in replay):**
///   `WorktreeCreated`, `PatchCaptured`.
///
/// A gate's or policy's verdict effect depends on its declared OBLIGATION, not the event
/// alone. Only a `Required` obligation can move the verdict off `Pass`; an `Optional` gate or
/// policy NEVER blocks, for ANY outcome (a required-`Denied`/`Error` policy → `Blocked`/
/// `Error`, but the same outcome on an optional policy is neutral; likewise every outcome of
/// an optional gate is neutral).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventKind {
    /// The run began; carries the pre-execution obligation contract and the task it runs
    /// for (bound by digest). MUST be the first event.
    RunStarted {
        contract: RunContract,
        task: ArtifactRef,
    },
    /// The isolated worktree was materialized (evidence-only for the verdict).
    WorktreeCreated { worktree: ArtifactRef },
    /// The agent process was spawned.
    AgentStarted,
    /// The agent execution ended, with its terminal outcome.
    AgentExited { outcome: AgentOutcome },
    /// A patch was captured from the worktree (evidence-only for the verdict).
    PatchCaptured { patch: ArtifactRef },
    /// A policy pre-check ran, bound to the policy it evaluated (by digest), with its
    /// outcome.
    PolicyChecked {
        policy: ArtifactRef,
        outcome: PolicyOutcome,
    },
    /// A gate began executing.
    GateStarted { gate: GateId },
    /// A gate finished, with its observed outcome and captured log (if any). Applicability
    /// is NOT here — it is fixed in the contract.
    GateFinished {
        gate: GateId,
        outcome: GateOutcome,
        log: Option<ArtifactRef>,
    },
    /// Confinement evidence for a specific subject/policy, and whether it satisfies the
    /// policy.
    SandboxEvidenceCaptured {
        subject: ExecutionSubject,
        policy_digest: Digest256,
        report: ArtifactRef,
        outcome: SandboxEvidenceOutcome,
    },
    /// Q-Deck R1 fifth corrective round: a durable, canonical receipt of the
    /// provider session identity a successful agent response returned —
    /// evidence-only (never verdict-bearing), at most once per run, bound
    /// by digest like any other artifact. Its ABSENCE from an otherwise
    /// sealed stream is not an error — a run whose agent call never
    /// returned a session (or predates this event existing) simply has no
    /// canonical session evidence, and stays honestly non-continuable
    /// (`docs/q-deck/r1-command.md` §6/§7). Replaces `meta.json` as the
    /// authority recovery trusts for session backfill: `meta.json` is not
    /// part of this digest-chained stream, so nothing about a clean replay
    /// ever proved it belonged to this run.
    ProviderSessionCaptured { receipt: ArtifactRef },
    /// Q-Deck R1 fifth corrective round: a durable receipt binding this
    /// canonical record to the EXACT accepted Command that dispatched it —
    /// evidence-only, at most once per run, bound by digest. Only a
    /// command-continuation child run ever carries one; a plain `o7 run`
    /// has no command to bind. A sealed record's own `task` artifact digest
    /// independently corroborates the command TEXT (§9.5); this receipt is
    /// what corroborates the command's IDENTITY (`command_id`,
    /// `conversation_id`, `parent_run_id`, `child_run_id`) against tamper
    /// or confusion between commands.
    CommandBindingCaptured { binding: ArtifactRef },
    /// The run was sealed. MUST be the LAST event; the verdict is fixed here.
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
            Self::ProviderSessionCaptured { .. } => 11,
            Self::CommandBindingCaptured { .. } => 12,
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
            Self::ProviderSessionCaptured { .. } => "provider_session_captured",
            Self::CommandBindingCaptured { .. } => "command_binding_captured",
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
    /// `event_digest`). Deterministic and total — pure field framing, no serialization, no
    /// panics.
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
        RunEventKind::RunStarted { contract, task } => {
            fold_contract(h, contract);
            fold_artifact(h, task);
        }
        RunEventKind::WorktreeCreated { worktree } => fold_artifact(h, worktree),
        RunEventKind::AgentStarted | RunEventKind::RunSealed => {}
        RunEventKind::AgentExited { outcome } => fold_agent_outcome(h, outcome),
        RunEventKind::PatchCaptured { patch } => fold_artifact(h, patch),
        RunEventKind::PolicyChecked { policy, outcome } => {
            fold_artifact(h, policy);
            h.update([policy_outcome_tag(*outcome)]);
        }
        RunEventKind::GateStarted { gate } => frame(h, gate.as_str().as_bytes()),
        RunEventKind::GateFinished { gate, outcome, log } => {
            frame(h, gate.as_str().as_bytes());
            h.update([gate_outcome_tag(*outcome)]);
            fold_optional_artifact(h, log.as_ref());
        }
        RunEventKind::SandboxEvidenceCaptured {
            subject,
            policy_digest,
            report,
            outcome,
        } => {
            fold_subject(h, subject);
            frame(h, policy_digest.as_str().as_bytes());
            fold_artifact(h, report);
            h.update([sandbox_outcome_tag(*outcome)]);
        }
        RunEventKind::ProviderSessionCaptured { receipt } => fold_artifact(h, receipt),
        RunEventKind::CommandBindingCaptured { binding } => fold_artifact(h, binding),
    }
}

fn fold_contract(h: &mut Sha256, contract: &RunContract) {
    frame(h, &(contract.gate_obligations.len() as u64).to_le_bytes());
    for o in &contract.gate_obligations {
        frame(h, o.gate.as_str().as_bytes());
        h.update([gate_requirement_tag(o.requirement)]);
        fold_applicability(h, &o.applicability);
    }
    frame(h, &(contract.policy_obligations.len() as u64).to_le_bytes());
    for p in &contract.policy_obligations {
        fold_artifact(h, &p.policy);
        h.update([policy_requirement_tag(p.requirement)]);
        frame(h, &(p.protects.len() as u64).to_le_bytes());
        for s in &p.protects {
            fold_protected_subject(h, s);
        }
    }
    frame(
        h,
        &(contract.sandbox_requirements.len() as u64).to_le_bytes(),
    );
    for r in &contract.sandbox_requirements {
        fold_subject(h, &r.subject);
        frame(h, r.policy_digest.as_str().as_bytes());
    }
    fold_agent_obligation(h, &contract.agent);
    frame(h, contract.runner_environment.as_bytes());
}

fn fold_applicability(h: &mut Sha256, a: &GateApplicability) {
    match a {
        GateApplicability::Applicable => h.update([1u8]),
        GateApplicability::Waived {
            environment,
            reason,
        } => {
            h.update([2u8]);
            frame(h, environment.as_bytes());
            frame(h, reason.as_bytes());
        }
    }
}

fn fold_subject(h: &mut Sha256, s: &ExecutionSubject) {
    match s {
        ExecutionSubject::Agent => h.update([1u8]),
        ExecutionSubject::Gate { gate } => {
            h.update([2u8]);
            frame(h, gate.as_str().as_bytes());
        }
        ExecutionSubject::Verifier => h.update([3u8]),
    }
}

fn fold_protected_subject(h: &mut Sha256, s: &PolicyProtectedSubject) {
    match s {
        PolicyProtectedSubject::Agent => h.update([1u8]),
        PolicyProtectedSubject::Gate { gate } => {
            h.update([2u8]);
            frame(h, gate.as_str().as_bytes());
        }
    }
}

fn fold_artifact(h: &mut Sha256, a: &ArtifactRef) {
    h.update([artifact_kind_tag(a.kind)]);
    frame(h, a.locator.as_bytes());
    frame(h, a.digest.as_str().as_bytes());
}

fn fold_optional_artifact(h: &mut Sha256, a: Option<&ArtifactRef>) {
    match a {
        Some(a) => {
            h.update([1u8]);
            fold_artifact(h, a);
        }
        None => h.update([0u8]),
    }
}

fn fold_agent_outcome(h: &mut Sha256, outcome: &AgentOutcome) {
    match outcome {
        AgentOutcome::ExitedNormally { code } => {
            h.update([1u8]);
            frame(h, &code.to_le_bytes());
        }
        AgentOutcome::ExitedBySignal { signal } => {
            h.update([2u8]);
            frame(h, &signal.to_le_bytes());
        }
        AgentOutcome::CancelledGracefully => h.update([3u8]),
        AgentOutcome::CancelledForcefully => h.update([4u8]),
        AgentOutcome::FailedToStart { reason } => {
            h.update([5u8]);
            frame(h, reason.as_bytes());
        }
        AgentOutcome::BoundaryFailure { reason } => {
            h.update([6u8]);
            frame(h, reason.as_bytes());
        }
        AgentOutcome::ObservationFailure { reason } => {
            h.update([7u8]);
            frame(h, reason.as_bytes());
        }
        AgentOutcome::OutputFailure { reason } => {
            h.update([8u8]);
            frame(h, reason.as_bytes());
        }
        AgentOutcome::CleanupFailure { reason } => {
            h.update([9u8]);
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
        ArtifactKind::Policy => 6,
        ArtifactKind::ProviderSession => 7,
        ArtifactKind::CommandBinding => 8,
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

fn policy_outcome_tag(outcome: PolicyOutcome) -> u8 {
    match outcome {
        PolicyOutcome::Allowed => 1,
        PolicyOutcome::Denied => 2,
        PolicyOutcome::Error => 3,
    }
}

fn sandbox_outcome_tag(outcome: SandboxEvidenceOutcome) -> u8 {
    match outcome {
        SandboxEvidenceOutcome::Satisfied => 1,
        SandboxEvidenceOutcome::Violated => 2,
        SandboxEvidenceOutcome::Error => 3,
    }
}

fn gate_requirement_tag(requirement: GateRequirement) -> u8 {
    match requirement {
        GateRequirement::Required => 1,
        GateRequirement::Optional => 2,
    }
}

fn policy_requirement_tag(requirement: PolicyRequirement) -> u8 {
    match requirement {
        PolicyRequirement::Required => 1,
        PolicyRequirement::Optional => 2,
    }
}

fn fold_agent_obligation(h: &mut Sha256, agent: &AgentObligation) {
    match agent {
        AgentObligation::Required => h.update([1u8]),
        AgentObligation::NotUsed { reason } => {
            h.update([2u8]);
            frame(h, reason.as_bytes());
        }
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
