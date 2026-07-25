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
    ArtifactKind, ArtifactRef, Digest256, GateApplicability, GateObligation, GateOutcome,
    GateRequirement, RunContract, SandboxRequirement, RUN_EVENT_SCHEMA_VERSION,
};
use o7_run::ids::{GateId, RunEventId, RunId};
use o7_run::replay::{ArtifactError, ArtifactResolver};
use o7_run::{AgentOutcome, RunEvent, RunEventKind};

/// The run id every fixture stream is scoped to.
pub fn run_id() -> RunId {
    RunId::from_raw("run-fixture-1")
}

/// A gate id from a short name.
pub fn gate(name: &str) -> GateId {
    GateId::from_raw(name)
}

/// A required+applicable gate obligation.
pub fn req(name: &str) -> GateObligation {
    GateObligation {
        gate: gate(name),
        requirement: GateRequirement::Required,
        applicability: GateApplicability::Applicable,
    }
}

/// An optional gate obligation.
pub fn opt(name: &str) -> GateObligation {
    GateObligation {
        gate: gate(name),
        requirement: GateRequirement::Optional,
        applicability: GateApplicability::Applicable,
    }
}

/// A required gate WAIVED for the given environment (declared skippable up front).
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

/// A Linux contract from gate obligations, with no sandbox requirements.
pub fn contract(obligations: Vec<GateObligation>) -> RunContract {
    RunContract {
        gate_obligations: obligations,
        sandbox_requirements: Vec::new(),
        runner_environment: "linux".to_owned(),
    }
}

/// A Linux contract with explicit sandbox requirements.
pub fn contract_sandboxed(
    obligations: Vec<GateObligation>,
    sandbox: Vec<SandboxRequirement>,
) -> RunContract {
    RunContract {
        gate_obligations: obligations,
        sandbox_requirements: sandbox,
        runner_environment: "linux".to_owned(),
    }
}

/// The canonical bytes of the fixture task artifact (so a replay resolver can seed matching
/// content without drift).
pub const TASK_BYTES: &[u8] = b"{\"goal\":\"fixture\"}";

/// The task artifact every `RunStarted` binds to (its content is irrelevant to the reducer,
/// which only checks the KIND; replay resolves and hashes it).
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

/// An artifact reference whose digest matches `bytes`, paired with those bytes so a test can
/// seed a resolver with matching (or, deliberately, mismatching) content.
pub fn artifact(kind: ArtifactKind, locator: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        kind,
        locator: locator.to_owned(),
        digest: Digest256::of_bytes(bytes),
    }
}

/// A normal agent exit with code 0.
pub fn agent_ok() -> AgentOutcome {
    AgentOutcome::ExitedNormally { code: 0 }
}

/// Construct a single event with its `event_digest` correctly computed over its fields and
/// its `previous_event_digest` set to `prev`. The low-level primitive for hand-building
/// malformed streams.
pub fn make_event(run: &RunId, sequence: u64, prev: &Digest256, kind: RunEventKind) -> RunEvent {
    let mut event = RunEvent {
        event_id: RunEventId::from_raw(format!("ev-{sequence}")),
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

/// Construct an event with FULL control over `event_id` and `schema_version` (and a
/// correctly-computed `event_digest`). For structural tests that need to forge a duplicate
/// id or an unsupported schema while keeping the single event internally consistent.
pub fn event_with(
    run: &RunId,
    event_id: &str,
    schema: u32,
    sequence: u64,
    prev: &Digest256,
    kind: RunEventKind,
) -> RunEvent {
    let mut event = RunEvent {
        event_id: RunEventId::from_raw(event_id),
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

/// Build a correctly-chained stream from a list of event kinds: sequence 0..n, each
/// `previous_event_digest` linked to its predecessor's `event_digest` (genesis for the
/// first), and every `event_digest` computed. The canonical happy path.
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

/// A minimal well-formed stream: start with a single required+applicable gate, run the agent
/// to a clean exit, run the gate to `outcome`, then seal. Used by the verdict tests.
pub fn single_required_gate_stream(gate_name: &str, outcome: GateOutcome) -> Vec<RunEvent> {
    chained(vec![
        run_started(contract(vec![req(gate_name)])),
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

/// An in-memory [`ArtifactResolver`] mapping locator → current bytes. Seed it with matching
/// bytes for a clean replay, or with different bytes to simulate a post-seal edit.
#[derive(Default)]
pub struct MapResolver {
    map: HashMap<String, Vec<u8>>,
}

impl MapResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the CURRENT bytes stored at `locator` (what replay will resolve and hash).
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
