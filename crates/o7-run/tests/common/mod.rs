//! Shared fixtures for the reducer/replay acceptance tests: builders that produce
//! correctly-chained canonical event streams (and the primitives to deliberately break
//! them), plus an in-memory artifact resolver for injecting tampered content.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;

use o7_run::event::{
    AgentObligation, ArtifactKind, ArtifactRef, Digest256, ExecutionSubject, GateApplicability,
    GateObligation, GateOutcome, GateRequirement, PolicyObligation, PolicyRequirement, RunContract,
    SandboxRequirement, RUN_EVENT_SCHEMA_VERSION,
};
use o7_run::ids::{GateId, RunEventId, RunId};
use o7_run::replay::{ArtifactError, ArtifactResolver};
use o7_run::{AgentOutcome, RunEvent, RunEventKind};

// ---- id helpers (validated constructors; `from_raw` no longer exists) ----

pub fn run_id() -> RunId {
    RunId::new("run-fixture-1").expect("valid run id")
}

pub fn gate(name: &str) -> GateId {
    GateId::new(name).expect("valid gate id")
}

fn event_id(raw: impl Into<String>) -> RunEventId {
    RunEventId::new(raw).expect("valid event id")
}

// ---- gate obligations ----

pub fn req(name: &str) -> GateObligation {
    GateObligation {
        gate: gate(name),
        requirement: GateRequirement::Required,
        applicability: GateApplicability::Applicable,
    }
}

pub fn opt(name: &str) -> GateObligation {
    GateObligation {
        gate: gate(name),
        requirement: GateRequirement::Optional,
        applicability: GateApplicability::Applicable,
    }
}

pub fn waived(name: &str, environment: &str, reason: &str) -> GateObligation {
    GateObligation {
        gate: gate(name),
        requirement: GateRequirement::Required,
        applicability: GateApplicability::Waived {
            environment: environment.to_owned(),
            reason: reason.to_owned(),
        },
    }
}

// ---- contracts ----

/// The fully-explicit contract builder; the convenience helpers below wrap it.
pub fn contract_full(
    gates: Vec<GateObligation>,
    policies: Vec<PolicyObligation>,
    sandbox: Vec<SandboxRequirement>,
    agent: AgentObligation,
) -> RunContract {
    RunContract {
        gate_obligations: gates,
        policy_obligations: policies,
        sandbox_requirements: sandbox,
        agent,
        runner_environment: "linux".to_owned(),
    }
}

/// A gate-only contract with NO agent (so streams without agent events are legitimate).
pub fn contract(gates: Vec<GateObligation>) -> RunContract {
    contract_full(gates, Vec::new(), Vec::new(), not_used())
}

/// A gate contract that REQUIRES an agent (for streams that emit agent events).
pub fn contract_agent(gates: Vec<GateObligation>) -> RunContract {
    contract_full(gates, Vec::new(), Vec::new(), AgentObligation::Required)
}

/// A contract with policy obligations and no agent.
pub fn contract_policy(gates: Vec<GateObligation>, policies: Vec<PolicyObligation>) -> RunContract {
    contract_full(gates, policies, Vec::new(), not_used())
}

/// A contract with sandbox requirements and no agent.
pub fn contract_sandboxed(
    gates: Vec<GateObligation>,
    sandbox: Vec<SandboxRequirement>,
) -> RunContract {
    contract_full(gates, Vec::new(), sandbox, not_used())
}

pub fn not_used() -> AgentObligation {
    AgentObligation::NotUsed {
        reason: "no agent in this fixture".to_owned(),
    }
}

// ---- policy / sandbox helpers ----

pub fn policy_artifact() -> ArtifactRef {
    artifact(ArtifactKind::Policy, "policy.json", b"{\"net\":\"deny\"}")
}

pub fn required_policy(policy: ArtifactRef, protects: Vec<ExecutionSubject>) -> PolicyObligation {
    PolicyObligation {
        policy,
        requirement: PolicyRequirement::Required,
        protects,
    }
}

pub fn sandbox_policy_digest() -> Digest256 {
    Digest256::of_bytes(b"agent-confinement-policy-v1")
}

pub fn agent_sandbox_requirement() -> SandboxRequirement {
    SandboxRequirement {
        subject: ExecutionSubject::Agent,
        policy_digest: sandbox_policy_digest(),
    }
}

// ---- artifacts ----

/// The canonical bytes of the fixture task artifact (so a replay resolver can seed matching
/// content without drift).
pub const TASK_BYTES: &[u8] = b"{\"goal\":\"fixture\"}";

pub fn task() -> ArtifactRef {
    artifact(ArtifactKind::Task, "task.json", TASK_BYTES)
}

/// A `RunStarted` kind carrying `contract` and the default task binding.
pub fn run_started(contract: RunContract) -> RunEventKind {
    RunEventKind::RunStarted {
        contract,
        task: task(),
    }
}

/// An artifact reference whose digest matches `bytes`.
pub fn artifact(kind: ArtifactKind, locator: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        kind,
        locator: locator.to_owned(),
        digest: Digest256::of_bytes(bytes),
    }
}

pub fn agent_ok() -> AgentOutcome {
    AgentOutcome::ExitedNormally { code: 0 }
}

// ---- event / stream construction ----

/// A single event with a correctly-computed `event_digest` and its `previous_event_digest`
/// set to `prev`. The low-level primitive for hand-building malformed streams.
pub fn make_event(run: &RunId, sequence: u64, prev: &Digest256, kind: RunEventKind) -> RunEvent {
    let mut event = RunEvent {
        event_id: event_id(format!("ev-{sequence}")),
        schema_version: RUN_EVENT_SCHEMA_VERSION,
        run_id: run.clone(),
        sequence,
        previous_event_digest: prev.clone(),
        event_digest: Digest256::genesis(),
        timestamp_millis: sequence as i64,
        kind,
    };
    event.event_digest = event.compute_digest();
    event
}

/// An event with FULL control over `event_id` and `schema_version` (with a correctly-computed
/// digest) — for forging a duplicate id or an unsupported schema while keeping the single
/// event internally consistent.
pub fn event_with(
    run: &RunId,
    id: &str,
    schema: u32,
    sequence: u64,
    prev: &Digest256,
    kind: RunEventKind,
) -> RunEvent {
    let mut event = RunEvent {
        event_id: event_id(id),
        schema_version: schema,
        run_id: run.clone(),
        sequence,
        previous_event_digest: prev.clone(),
        event_digest: Digest256::genesis(),
        timestamp_millis: sequence as i64,
        kind,
    };
    event.event_digest = event.compute_digest();
    event
}

/// Build a correctly-chained stream: sequence 0..n, each `previous_event_digest` linked to
/// its predecessor's `event_digest` (genesis for the first), every digest computed.
pub fn chained(kinds: Vec<RunEventKind>) -> Vec<RunEvent> {
    let run = run_id();
    let mut prev = Digest256::genesis();
    let mut events = Vec::with_capacity(kinds.len());
    for (i, kind) in kinds.into_iter().enumerate() {
        let event = make_event(&run, i as u64, &prev, kind);
        prev = event.event_digest.clone();
        events.push(event);
    }
    events
}

/// A minimal well-formed stream: a required+applicable gate, a required agent that exits
/// cleanly, the gate run to `outcome`, then seal. Used by the verdict tests.
pub fn single_required_gate_stream(gate_name: &str, outcome: GateOutcome) -> Vec<RunEvent> {
    chained(vec![
        run_started(contract_agent(vec![req(gate_name)])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::GateStarted {
            gate: gate(gate_name),
        },
        RunEventKind::GateFinished {
            gate: gate(gate_name),
            outcome,
            log: None,
        },
        RunEventKind::RunSealed,
    ])
}

/// An in-memory [`ArtifactResolver`] mapping locator → current bytes.
#[derive(Default)]
pub struct MapResolver {
    map: HashMap<String, Vec<u8>>,
}

impl MapResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, locator: &str, bytes: &[u8]) {
        self.map.insert(locator.to_owned(), bytes.to_vec());
    }
}

impl ArtifactResolver for MapResolver {
    fn resolve(&self, artifact: &ArtifactRef) -> Result<Vec<u8>, ArtifactError> {
        self.map
            .get(&artifact.locator)
            .cloned()
            .ok_or_else(|| ArtifactError {
                locator: artifact.locator.clone(),
                reason: "not present in resolver".to_owned(),
            })
    }
}
