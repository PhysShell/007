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
    ArtifactKind, ArtifactRef, Digest256, GateOutcome, RunContract, RUN_EVENT_SCHEMA_VERSION,
};
use o7_run::ids::{GateId, RunEventId, RunId};
use o7_run::replay::{ArtifactError, ArtifactResolver};
use o7_run::{RunEvent, RunEventKind};

/// The run id every fixture stream is scoped to.
pub fn run_id() -> RunId {
    RunId::from_raw("run-fixture-1")
}

/// A gate id from a short name.
pub fn gate(name: &str) -> GateId {
    GateId::from_raw(name)
}

/// A required-gate contract for the given required gates, in a Linux environment, with no
/// sandbox-evidence requirement and nothing declared optional-in-env.
pub fn contract(required: &[&str]) -> RunContract {
    RunContract {
        required_gates: required.iter().map(|g| gate(g)).collect(),
        optional_in_env: Vec::new(),
        requires_sandbox_evidence: false,
        runner_environment: "linux".to_owned(),
    }
}

/// An artifact reference whose digest matches `bytes`, paired with those bytes so a test
/// can seed a resolver with matching (or, deliberately, mismatching) content.
pub fn artifact(kind: ArtifactKind, locator: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        kind,
        locator: locator.to_owned(),
        digest: Digest256::of_bytes(bytes),
    }
}

/// Construct a single event with its `event_digest` correctly computed over its fields and
/// its `previous_event_digest` set to `prev`. This is the low-level primitive; the
/// structural tests use it to hand-build malformed streams.
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

/// A minimal well-formed stream: start with `contract`, run a single gate to `outcome`
/// (supported in-env), then seal. Used by the verdict tests.
pub fn single_gate_stream(
    contract: RunContract,
    gate_name: &str,
    outcome: GateOutcome,
) -> Vec<RunEvent> {
    let g = gate(gate_name);
    chained(vec![
        RunEventKind::RunStarted { contract },
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            exit: o7_run::event::AgentExit::Exited { code: 0 },
        },
        RunEventKind::GateStarted { gate: g.clone() },
        RunEventKind::GateFinished {
            gate: g,
            outcome,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::RunSealed,
    ])
}

/// An in-memory [`ArtifactResolver`] mapping locator → current bytes. Seed it with
/// matching bytes for a clean replay, or with different bytes to simulate a post-seal edit.
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
