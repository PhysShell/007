//! Adversarial tests for trust binding and the verdict authority (PR 3, slice 3).
//!
//! Proven here (pure — no process is spawned):
//!   * any drift in argv, cwd policy, repository identity, or executable CONTENT
//!     invalidates trust;
//!   * a not-run outcome is never a pass;
//!   * verifier evidence cannot accept itself — only o7d's `adjudicate` can, and only
//!     with trust, a satisfied boundary requirement, and a clean in-policy completion;
//!   * every non-completion outcome is rejected.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use o7_verifier::{
    adjudicate, AttestedEnforcement, CwdPolicy, ExitPolicy, OutputLimits, TrustAnchor, TrustStore,
    TrustedCommand, Verdict, VerifierEvidence, VerifierOutcome,
};
use o7_worker::BoundaryRequirement;
use o7_worktree::CanonicalRepoId;

fn repo_id(ino: u64) -> CanonicalRepoId {
    CanonicalRepoId {
        git_common_dir: PathBuf::from("/srv/repo/.git"),
        dev: 66,
        ino,
    }
}

fn write_exe(dir: &std::path::Path, name: &str, body: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    path
}

fn command(exe: PathBuf, args: &[&str], cwd: CwdPolicy) -> TrustedCommand {
    TrustedCommand {
        executable: exe,
        arguments: args.iter().map(OsString::from).collect(),
        cwd_policy: cwd,
        environment: BTreeMap::new(),
        timeout: Duration::from_secs(30),
        output_limits: OutputLimits::default(),
        exit_policy: ExitPolicy::exactly_zero(),
    }
}

#[test]
fn trust_drifts_on_argv_cwd_repo_and_executable_content() {
    let dir = tempfile::tempdir().unwrap();
    let exe = write_exe(dir.path(), "verify", b"#!/bin/sh\nexit 0\n");
    let repo = repo_id(1000);

    let cmd = command(exe.clone(), &["--all"], CwdPolicy::WorktreeRoot);
    let anchor = TrustAnchor::compute(&repo, &cmd).unwrap();
    let mut store = TrustStore::new();
    store.trust(&anchor);
    assert!(store.is_trusted(&anchor));

    // argv drift.
    let argv_drift = TrustAnchor::compute(
        &repo,
        &command(exe.clone(), &["--all", "--fast"], CwdPolicy::WorktreeRoot),
    )
    .unwrap();
    assert!(!store.is_trusted(&argv_drift), "argv drift stayed trusted");

    // cwd drift.
    let cwd_drift = TrustAnchor::compute(
        &repo,
        &command(
            exe.clone(),
            &["--all"],
            CwdPolicy::Absolute(PathBuf::from("/opt/tools")),
        ),
    )
    .unwrap();
    assert!(!store.is_trusted(&cwd_drift), "cwd drift stayed trusted");

    // repository-identity drift (different inode).
    let repo_drift = TrustAnchor::compute(
        &repo_id(2000),
        &command(exe.clone(), &["--all"], CwdPolicy::WorktreeRoot),
    )
    .unwrap();
    assert!(!store.is_trusted(&repo_drift), "repo drift stayed trusted");

    // executable-content drift: same path, swapped bytes.
    write_exe(dir.path(), "verify", b"#!/bin/sh\nrm -rf /\n");
    let exe_drift =
        TrustAnchor::compute(&repo, &command(exe, &["--all"], CwdPolicy::WorktreeRoot)).unwrap();
    assert!(
        !store.is_trusted(&exe_drift),
        "a swapped executable stayed trusted"
    );
}

#[test]
fn trust_can_be_revoked() {
    let dir = tempfile::tempdir().unwrap();
    let exe = write_exe(dir.path(), "verify", b"x");
    let repo = repo_id(1);
    let anchor = TrustAnchor::compute(&repo, &command(exe, &[], CwdPolicy::WorktreeRoot)).unwrap();
    let mut store = TrustStore::new();
    store.trust(&anchor);
    assert!(store.is_trusted(&anchor));
    store.revoke(anchor.digest());
    assert!(!store.is_trusted(&anchor));
}

// The trust binding covers the WHOLE command, not just repo+exe+argv+cwd: a different
// environment allowlist, a widened exit policy, a longer timeout, or a larger output
// budget each invalidates trust.
#[test]
fn trust_drifts_on_environment_exit_policy_timeout_and_output() {
    let dir = tempfile::tempdir().unwrap();
    let exe = write_exe(dir.path(), "verify", b"x");
    let repo = repo_id(1);
    let base = command(exe.clone(), &["--all"], CwdPolicy::WorktreeRoot);
    let anchor = TrustAnchor::compute(&repo, &base).unwrap();
    let mut store = TrustStore::new();
    store.trust(&anchor);
    assert!(store.is_trusted(&anchor));

    // environment drift.
    let mut env_drift = base.clone();
    env_drift
        .environment
        .insert(OsString::from("EVIL"), OsString::from("1"));
    assert!(!store.is_trusted(&TrustAnchor::compute(&repo, &env_drift).unwrap()));

    // exit-policy drift (widen to also accept 1).
    let mut policy_drift = base.clone();
    policy_drift.exit_policy = ExitPolicy::codes([0, 1]);
    assert!(!store.is_trusted(&TrustAnchor::compute(&repo, &policy_drift).unwrap()));

    // timeout drift.
    let mut timeout_drift = base.clone();
    timeout_drift.timeout = Duration::from_secs(31);
    assert!(!store.is_trusted(&TrustAnchor::compute(&repo, &timeout_drift).unwrap()));

    // output-budget drift.
    let mut output_drift = base.clone();
    output_drift.output_limits = OutputLimits {
        max_total_bytes: base.output_limits.max_total_bytes + 1,
    };
    assert!(!store.is_trusted(&TrustAnchor::compute(&repo, &output_drift).unwrap()));

    // the exact original command is still trusted.
    assert!(store.is_trusted(&TrustAnchor::compute(&repo, &base).unwrap()));
}

// o7d adjudicates against the exit policy BOUND in the evidence — it cannot widen the
// accepted codes after the run, because `adjudicate` has no policy parameter.
#[test]
fn adjudication_uses_the_bound_exit_policy_not_a_late_one() {
    let full = Some(AttestedEnforcement::FullyEnforced);

    // A completion with code 1 under a bound policy of {0} is rejected.
    let mut strict = evidence(VerifierOutcome::Completed { exit_code: 1 }, true, full);
    strict.exit_policy = ExitPolicy::exactly_zero();
    assert!(!strict.is_pass_candidate());
    assert!(!adjudicate(&strict, BoundaryRequirement::RequireFullyEnforced).is_accepted());

    // The SAME completion is accepted only when the command's OWN bound policy admits 1.
    let mut lenient = evidence(VerifierOutcome::Completed { exit_code: 1 }, true, full);
    lenient.exit_policy = ExitPolicy::codes([0, 1]);
    assert!(lenient.is_pass_candidate());
    assert_eq!(
        adjudicate(&lenient, BoundaryRequirement::RequireFullyEnforced),
        Verdict::Accepted
    );
}

fn evidence(
    outcome: VerifierOutcome,
    trusted: bool,
    enforce: Option<AttestedEnforcement>,
) -> VerifierEvidence {
    VerifierEvidence {
        outcome,
        trusted,
        boundary_enforcement: enforce,
        command_digest: {
            let dir = tempfile::tempdir().unwrap();
            let exe = write_exe(dir.path(), "v", b"x");
            TrustAnchor::compute(&repo_id(1), &command(exe, &[], CwdPolicy::WorktreeRoot))
                .unwrap()
                .digest()
                .clone()
        },
        exit_policy: ExitPolicy::exactly_zero(),
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

#[test]
fn not_run_is_never_a_pass() {
    let ev = evidence(
        VerifierOutcome::NotRun {
            reason: "not trusted".to_owned(),
        },
        false,
        None,
    );
    assert!(!ev.is_pass_candidate());
    assert_eq!(
        adjudicate(&ev, BoundaryRequirement::RequireFullyEnforced),
        Verdict::Rejected(
            "boundary requirement RequireFullyEnforced not met: attested None".to_owned()
        )
    );
}

#[test]
fn evidence_cannot_self_accept_only_o7d_adjudicates() {
    // A clean completion is only a CANDIDATE — the evidence never yields a verdict.
    let clean = evidence(
        VerifierOutcome::Completed { exit_code: 0 },
        true,
        Some(AttestedEnforcement::FullyEnforced),
    );
    assert!(clean.is_pass_candidate());

    // Trusted + fully-enforced + in-policy completion → o7d accepts.
    assert_eq!(
        adjudicate(&clean, BoundaryRequirement::RequireFullyEnforced),
        Verdict::Accepted
    );

    // Remove trust: even a clean completion is rejected.
    let untrusted = evidence(
        VerifierOutcome::Completed { exit_code: 0 },
        false,
        Some(AttestedEnforcement::FullyEnforced),
    );
    assert!(untrusted.is_pass_candidate()); // the OUTCOME looks fine...
    assert!(
        !adjudicate(&untrusted, BoundaryRequirement::RequireFullyEnforced).is_accepted(),
        "an untrusted command was accepted"
    );

    // Weaken the boundary: RequireFullyEnforced with a None/Partial/absent attestation
    // is rejected — no fallback.
    for level in [
        None,
        Some(AttestedEnforcement::None),
        Some(AttestedEnforcement::Partial),
    ] {
        let ev = evidence(VerifierOutcome::Completed { exit_code: 0 }, true, level);
        assert!(
            !adjudicate(&ev, BoundaryRequirement::RequireFullyEnforced).is_accepted(),
            "accepted under insufficient enforcement {level:?}"
        );
    }
}

#[test]
fn every_non_completion_and_bad_exit_is_rejected() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let non_completions = [
        VerifierOutcome::NotRun { reason: "x".into() },
        VerifierOutcome::SpawnFailed { reason: "x".into() },
        VerifierOutcome::TimedOut,
        VerifierOutcome::Signalled { signal: 9 },
        VerifierOutcome::OutputLost { reason: "x".into() },
        VerifierOutcome::BoundaryUnavailable { reason: "x".into() },
        VerifierOutcome::Faulted { reason: "x".into() },
    ];
    for outcome in non_completions {
        let ev = evidence(outcome.clone(), true, full);
        assert!(
            !ev.is_pass_candidate(),
            "{} was a pass candidate",
            outcome.kind()
        );
        assert!(
            !adjudicate(&ev, BoundaryRequirement::RequireFullyEnforced).is_accepted(),
            "{} was accepted",
            outcome.kind()
        );
    }

    // A completion with an out-of-policy exit code is rejected too.
    let bad_exit = evidence(VerifierOutcome::Completed { exit_code: 1 }, true, full);
    assert!(!bad_exit.is_pass_candidate());
    assert!(!adjudicate(&bad_exit, BoundaryRequirement::RequireFullyEnforced).is_accepted());
}
