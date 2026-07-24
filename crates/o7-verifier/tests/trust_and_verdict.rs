//! Adversarial tests for trust binding and the verdict authority (PR 3, slice 3 + the
//! opaque, non-forgeable adjudication capability).
//!
//! Proven here (pure — no process is spawned):
//!   * any drift in argv, cwd policy, repository identity, executable CONTENT,
//!     environment, exit policy, timeout, output budget, or BOUNDARY REQUIREMENT
//!     invalidates trust;
//!   * adjudication is against o7d's TRUST STORE and the evidence's OWN bound spec — it
//!     takes no late exit-policy or boundary argument;
//!   * a forged `trusted = true`, a structural (non-trust) digest, a swapped executable
//!     identity, a relaxed boundary requirement, and a revoked digest are ALL rejected;
//!   * verifier evidence can never accept itself — only `adjudicate` can, and only with an
//!     in-store digest, a satisfied bound requirement, and a clean in-policy completion.
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
    adjudicate, structural_command_digest, AttestedEnforcement, CwdPolicy, ExitPolicy,
    OutputLimits, RequiredBoundary, TrustAnchor, TrustStore, TrustedCommand, Verdict,
    VerifierEvidence, VerifierOutcome,
};
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
    command_boundary(exe, args, cwd, RequiredBoundary::RequireFullyEnforced)
}

fn command_boundary(
    exe: PathBuf,
    args: &[&str],
    cwd: CwdPolicy,
    boundary: RequiredBoundary,
) -> TrustedCommand {
    TrustedCommand {
        executable: exe,
        arguments: args.iter().map(OsString::from).collect(),
        cwd_policy: cwd,
        environment: BTreeMap::new(),
        timeout: Duration::from_secs(30),
        output_limits: OutputLimits::default(),
        exit_policy: ExitPolicy::exactly_zero(),
        boundary_requirement: boundary,
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

// The trust binding covers the WHOLE command: a different environment allowlist, a
// widened exit policy, a longer timeout, a larger output budget, or a RELAXED boundary
// requirement each invalidates trust.
#[test]
fn trust_drifts_on_environment_exit_policy_timeout_output_and_boundary() {
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

    // boundary-requirement drift (relax to unconfined).
    let mut boundary_drift = base.clone();
    boundary_drift.boundary_requirement = RequiredBoundary::AllowUnconfined;
    assert!(!store.is_trusted(&TrustAnchor::compute(&repo, &boundary_drift).unwrap()));

    // the exact original command is still trusted.
    assert!(store.is_trusted(&TrustAnchor::compute(&repo, &base).unwrap()));
}

// ---- adjudication: the opaque, non-forgeable capability ----

/// Build evidence carrying a FULL, self-consistent trust binding for `command` in `repo`,
/// plus a store that trusts it. Returns (evidence, store, anchor) so tests can revoke or
/// forge.
fn evidence_and_store(
    command: &TrustedCommand,
    repo: &CanonicalRepoId,
    exe_bytes: &[u8],
    outcome: VerifierOutcome,
    enforce: Option<AttestedEnforcement>,
) -> (VerifierEvidence, TrustStore, TrustAnchor) {
    let anchor = TrustAnchor::for_executable_bytes(repo, command, exe_bytes);
    let mut store = TrustStore::new();
    store.trust(&anchor);
    let ev = VerifierEvidence {
        outcome,
        trusted: true,
        boundary_enforcement: enforce,
        repo: repo.clone(),
        command: command.clone(),
        executable_identity: Some(anchor.executable_identity.clone()),
        trust_digest: Some(anchor.digest().clone()),
        structural_digest: structural_command_digest(repo, command),
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    (ev, store, anchor)
}

/// A trusted, fully-enforced, clean-exit evidence and the store that trusts it.
fn clean(
    outcome: VerifierOutcome,
    enforce: Option<AttestedEnforcement>,
) -> (VerifierEvidence, TrustStore) {
    let repo = repo_id(7);
    let cmd = command(
        PathBuf::from("/usr/bin/verify"),
        &["--all"],
        CwdPolicy::WorktreeRoot,
    );
    let (ev, store, _) = evidence_and_store(&cmd, &repo, b"exe-bytes", outcome, enforce);
    (ev, store)
}

#[test]
fn a_trusted_fully_enforced_clean_completion_is_accepted_only_by_o7d() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let (ev, store) = clean(VerifierOutcome::Completed { exit_code: 0 }, full);
    // The evidence is a candidate on its face, but only adjudication yields a verdict.
    assert!(ev.is_pass_candidate());
    assert_eq!(adjudicate(&ev, &store), Verdict::Accepted);
}

#[test]
fn a_forged_trusted_flag_cannot_self_accept_without_an_in_store_digest() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let (mut ev, store) = clean(VerifierOutcome::Completed { exit_code: 0 }, full);
    // Genuine evidence accepts against its store.
    assert!(adjudicate(&ev, &store).is_accepted());
    // The very same evidence — `trusted` still true — is rejected against an empty store:
    // the flag is self-description, never authority.
    assert!(ev.trusted);
    assert!(!adjudicate(&ev, &TrustStore::new()).is_accepted());
    // Flipping/forging the flag changes nothing either way.
    ev.trusted = true;
    assert!(!adjudicate(&ev, &TrustStore::new()).is_accepted());
}

#[test]
fn a_structural_digest_is_not_a_trust_digest() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let (mut ev, store) = clean(VerifierOutcome::Completed { exit_code: 0 }, full);
    // The structural digest (no executable content) differs from the trust digest.
    assert_ne!(Some(&ev.structural_digest), ev.trust_digest.as_ref());
    // Passing the structural digest off as the trust digest is rejected: it does not
    // re-derive from the bound command's full identity.
    ev.trust_digest = Some(ev.structural_digest.clone());
    assert!(matches!(adjudicate(&ev, &store), Verdict::Rejected(_)));
}

#[test]
fn a_swapped_executable_identity_is_rejected() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let repo = repo_id(7);
    let cmd = command(
        PathBuf::from("/usr/bin/verify"),
        &[],
        CwdPolicy::WorktreeRoot,
    );
    let (mut ev, store, _) = evidence_and_store(
        &cmd,
        &repo,
        b"real-bytes",
        VerifierOutcome::Completed { exit_code: 0 },
        full,
    );
    assert!(adjudicate(&ev, &store).is_accepted());
    // Swap the executable identity to a DIFFERENT binary's identity: the re-derived digest
    // no longer matches the claim / the store, so it is rejected (exe drift).
    let other = TrustAnchor::for_executable_bytes(&repo, &cmd, b"evil-bytes");
    ev.executable_identity = Some(other.executable_identity.clone());
    assert!(matches!(adjudicate(&ev, &store), Verdict::Rejected(_)));
}

#[test]
fn require_fully_enforced_cannot_be_reused_as_allow_unconfined() {
    let repo = repo_id(7);
    // Trusted for a FULLY-ENFORCED boundary.
    let strict = command(
        PathBuf::from("/usr/bin/verify"),
        &[],
        CwdPolicy::WorktreeRoot,
    );
    let (ev, store, _) = evidence_and_store(
        &strict,
        &repo,
        b"bytes",
        VerifierOutcome::Completed { exit_code: 0 },
        // ...but the run was NOT fully enforced.
        Some(AttestedEnforcement::None),
    );
    // Under the BOUND RequireFullyEnforced, a None attestation is rejected — and there is
    // no boundary argument to relax it with.
    assert!(matches!(adjudicate(&ev, &store), Verdict::Rejected(_)));

    // Forging the bound requirement down to AllowUnconfined does not help: the re-derived
    // digest differs from the trusted (strict) one, so it falls out of the store.
    let mut relaxed = ev.clone();
    relaxed.command.boundary_requirement = RequiredBoundary::AllowUnconfined;
    assert!(matches!(adjudicate(&relaxed, &store), Verdict::Rejected(_)));
}

#[test]
fn revocation_makes_prior_evidence_reject() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let repo = repo_id(7);
    let cmd = command(
        PathBuf::from("/usr/bin/verify"),
        &[],
        CwdPolicy::WorktreeRoot,
    );
    let (ev, mut store, anchor) = evidence_and_store(
        &cmd,
        &repo,
        b"bytes",
        VerifierOutcome::Completed { exit_code: 0 },
        full,
    );
    assert!(adjudicate(&ev, &store).is_accepted());
    // Revoke the digest: the SAME prior evidence now rejects.
    store.revoke(anchor.digest());
    assert!(!adjudicate(&ev, &store).is_accepted());
}

// o7d adjudicates against the exit policy BOUND in the evidence's command — it cannot
// widen the accepted codes after the run, because `adjudicate` has no policy parameter.
#[test]
fn adjudication_uses_the_bound_exit_policy_not_a_late_one() {
    let full = Some(AttestedEnforcement::FullyEnforced);
    let repo = repo_id(7);

    // Bound policy {0}: a completion with code 1 is rejected.
    let strict = command(
        PathBuf::from("/usr/bin/verify"),
        &[],
        CwdPolicy::WorktreeRoot,
    );
    let (ev, store, _) = evidence_and_store(
        &strict,
        &repo,
        b"bytes",
        VerifierOutcome::Completed { exit_code: 1 },
        full,
    );
    assert!(!ev.is_pass_candidate());
    assert!(!adjudicate(&ev, &store).is_accepted());

    // Bound policy {0,1}: the SAME completion is accepted — but ONLY because THAT command
    // (with the wider policy) is the one in the store.
    let mut lenient_cmd = strict.clone();
    lenient_cmd.exit_policy = ExitPolicy::codes([0, 1]);
    let (ev, store, _) = evidence_and_store(
        &lenient_cmd,
        &repo,
        b"bytes",
        VerifierOutcome::Completed { exit_code: 1 },
        full,
    );
    assert!(ev.is_pass_candidate());
    assert_eq!(adjudicate(&ev, &store), Verdict::Accepted);
}

#[test]
fn not_run_and_missing_binding_are_never_a_pass() {
    // A not-run outcome with a full binding still rejects (the outcome is not a completion).
    let (ev, store) = clean(
        VerifierOutcome::NotRun {
            reason: "not trusted".to_owned(),
        },
        Some(AttestedEnforcement::FullyEnforced),
    );
    assert!(!ev.is_pass_candidate());
    assert!(matches!(adjudicate(&ev, &store), Verdict::Rejected(_)));

    // Evidence with NO trust binding (never bound an executable) rejects outright.
    let (mut ev, store) = clean(
        VerifierOutcome::Completed { exit_code: 0 },
        Some(AttestedEnforcement::FullyEnforced),
    );
    ev.executable_identity = None;
    ev.trust_digest = None;
    assert!(!adjudicate(&ev, &store).is_accepted());
}

#[test]
fn insufficient_enforcement_is_rejected_with_no_fallback() {
    // A command bound to RequireFullyEnforced with a None/Partial/absent attestation is
    // rejected — no fallback to a weaker boundary.
    for level in [
        None,
        Some(AttestedEnforcement::None),
        Some(AttestedEnforcement::Partial),
    ] {
        let (ev, store) = clean(VerifierOutcome::Completed { exit_code: 0 }, level);
        assert!(
            !adjudicate(&ev, &store).is_accepted(),
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
        let (ev, store) = clean(outcome.clone(), full);
        assert!(
            !ev.is_pass_candidate(),
            "{} was a pass candidate",
            outcome.kind()
        );
        assert!(
            !adjudicate(&ev, &store).is_accepted(),
            "{} was accepted",
            outcome.kind()
        );
    }

    // A completion with an out-of-policy exit code is rejected too.
    let (bad_exit, store) = clean(VerifierOutcome::Completed { exit_code: 1 }, full);
    assert!(!bad_exit.is_pass_candidate());
    assert!(!adjudicate(&bad_exit, &store).is_accepted());
}
