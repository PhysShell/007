//! Known-answer and contract tests for the sandbox wire protocol: the length-bounded
//! versioned frame, the report's rejection of dishonest shapes, the validated binding
//! newtypes, and the order-independent, frozen policy digest.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use o7_sandbox_protocol::frame::{decode, encode, FrameError, MAX_REPORT_BYTES};
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{Enforcement, NetworkPolicy, SandboxPolicy, SandboxPolicyError};
use o7_sandbox_protocol::report::{ReportError, SandboxReport};
use o7_sandbox_protocol::SCHEMA_VERSION;

fn a_digest(seed: &str) -> Digest256 {
    Digest256::of_bytes(seed.as_bytes())
}

fn a_nonce() -> LaunchNonce {
    LaunchNonce::from_bytes([0xab; 16])
}

fn full_report() -> SandboxReport {
    SandboxReport {
        schema_version: SCHEMA_VERSION,
        backend: BackendIdentity::new("sandboy-linux", "0.1.0").unwrap(),
        policy_digest: a_digest("policy"),
        backend_digest: a_digest("backend"),
        launch_nonce: a_nonce(),
        launch_spec_digest: a_digest("launch"),
        filesystem: Enforcement::Enforced,
        network: Enforcement::Enforced,
        env: Enforcement::Enforced,
        process_tree: Enforcement::Enforced,
        timeout: Enforcement::Enforced,
    }
}

fn policy() -> SandboxPolicy {
    SandboxPolicy {
        worktree: PathBuf::from("/work/run-1"),
        allow_exec: vec![PathBuf::from("/proc"), PathBuf::from("/usr/bin")],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH"), OsString::from("HOME")],
        timeout: Duration::from_secs(600),
    }
}

// --- frame codec ---

#[test]
fn a_frame_round_trips() {
    let report = full_report();
    let bytes = encode(&report).expect("encodes");
    let back = decode(&bytes).expect("decodes");
    assert_eq!(report, back);
}

#[test]
fn a_truncated_frame_is_rejected() {
    let bytes = encode(&full_report()).unwrap();
    let cut = &bytes[..bytes.len() - 3];
    assert!(matches!(decode(cut), Err(FrameError::Truncated { .. })));
    // Even fewer than the length prefix.
    assert!(matches!(
        decode(&[0u8, 1]),
        Err(FrameError::Truncated { .. })
    ));
}

#[test]
fn trailing_bytes_after_one_frame_are_rejected() {
    let mut bytes = encode(&full_report()).unwrap();
    bytes.push(0x00); // a second, partial "frame"
    assert!(matches!(decode(&bytes), Err(FrameError::Trailing { .. })));
}

#[test]
fn an_oversized_declared_length_is_rejected_without_buffering() {
    // A 4-byte prefix claiming more than the max, with no body present: must be rejected on
    // the declared length alone, never by trying to read it.
    let mut bytes = ((MAX_REPORT_BYTES + 1) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(b"{}");
    assert!(matches!(decode(&bytes), Err(FrameError::TooLarge { .. })));
}

// --- report honesty ---

#[test]
fn a_boolean_secure_field_is_rejected() {
    let json = br#"{"schema_version":1,"backend":{"name":"x","version":"1"},"backend_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","policy_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","launch_nonce":"abababababababababababababababab","launch_spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"enforced","secure":true}"#;
    assert!(matches!(
        SandboxReport::parse(json),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn a_missing_dimension_is_rejected() {
    let json = br#"{"schema_version":1,"backend":{"name":"x","version":"1"},"backend_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","policy_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","launch_nonce":"abababababababababababababababab","launch_spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced"}"#;
    assert!(matches!(
        SandboxReport::parse(json),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn a_wrong_schema_version_is_rejected() {
    let mut report = full_report();
    report.schema_version = 999;
    let json = serde_json::to_vec(&report).unwrap();
    assert!(matches!(
        SandboxReport::parse(&json),
        Err(ReportError::UnsupportedVersion { found: 999 })
    ));
}

#[test]
fn a_malformed_binding_value_is_rejected() {
    // A non-hex policy digest must not deserialize.
    let json = br#"{"schema_version":1,"backend":{"name":"x","version":"1"},"backend_digest":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","policy_digest":"not-a-digest","launch_nonce":"abababababababababababababababab","launch_spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"enforced"}"#;
    assert!(matches!(
        SandboxReport::parse(json),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn an_anonymous_backend_is_rejected() {
    let json = br#"{"schema_version":1,"backend":{"name":"","version":"1"},"policy_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","launch_nonce":"abababababababababababababababab","launch_spec_digest":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"enforced"}"#;
    assert!(matches!(
        SandboxReport::parse(json),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn full_and_none_enforcement_predicates() {
    assert!(full_report().is_fully_enforced());
    assert!(!full_report().is_none_enforced());

    let mut partial = full_report();
    partial.network = Enforcement::Partial;
    assert!(!partial.is_fully_enforced(), "one downgrade breaks full");

    let mut nothing = full_report();
    nothing.filesystem = Enforcement::NotEnforced;
    nothing.network = Enforcement::NotEnforced;
    nothing.env = Enforcement::NotEnforced;
    nothing.process_tree = Enforcement::NotEnforced;
    nothing.timeout = Enforcement::NotEnforced;
    assert!(nothing.is_none_enforced());
}

// --- newtype validation ---

#[test]
fn digest_and_nonce_forms_are_validated() {
    assert!(Digest256::parse(&"a".repeat(64)).is_ok());
    assert!(Digest256::parse(&"A".repeat(64)).is_err(), "uppercase");
    assert!(Digest256::parse(&"a".repeat(63)).is_err(), "too short");
    assert!(LaunchNonce::parse(&"a".repeat(32)).is_ok());
    assert!(LaunchNonce::parse(&"a".repeat(31)).is_err());
    assert!(BackendIdentity::new("n", "v").is_ok());
    assert!(BackendIdentity::new(" ", "v").is_err());
}

// --- policy validation + canonical digest ---

#[test]
fn a_sub_millisecond_timeout_is_rejected() {
    let mut p = policy();
    p.timeout = Duration::from_nanos(1);
    assert!(matches!(
        p.validate(),
        Err(SandboxPolicyError::TimeoutTooSmall(_))
    ));
}

#[test]
fn an_oversized_timeout_is_rejected() {
    let mut p = policy();
    p.timeout = Duration::from_secs(48 * 60 * 60);
    assert!(matches!(
        p.validate(),
        Err(SandboxPolicyError::TimeoutTooLarge(_))
    ));
}

#[test]
fn the_policy_digest_is_order_independent() {
    let a = policy();
    let mut b = policy();
    b.allow_exec.reverse();
    b.env_allowlist.reverse();
    assert_eq!(
        a.digest(),
        b.digest(),
        "allowances/env are sets — declaration order must not change the digest"
    );
}

#[test]
fn the_policy_digest_changes_when_a_meaningful_field_changes() {
    let a = policy();
    let mut b = policy();
    b.timeout = Duration::from_secs(601);
    assert_ne!(a.digest(), b.digest());
    let mut c = policy();
    c.allow_exec.push(PathBuf::from("/opt"));
    assert_ne!(a.digest(), c.digest());
}

// --- launch request ---

fn a_request() -> o7_sandbox_protocol::LaunchRequest {
    o7_sandbox_protocol::LaunchRequest {
        schema_version: SCHEMA_VERSION,
        target_digest: a_digest("target"),
        argv: vec![b"/bin/sh".to_vec(), b"-c".to_vec(), b"exit 0".to_vec()],
        cwd: b"/work".to_vec(),
        env: vec![
            o7_sandbox_protocol::EnvEntry {
                name: b"PATH".to_vec(),
                value: b"/usr/bin".to_vec(),
            },
            o7_sandbox_protocol::EnvEntry {
                name: b"HOME".to_vec(),
                value: b"/root".to_vec(),
            },
        ],
        stdin: o7_sandbox_protocol::StdinKind::Null,
        launch_nonce: a_nonce(),
    }
}

#[test]
fn a_launch_request_frame_round_trips() {
    let req = a_request();
    let bytes = o7_sandbox_protocol::request::encode(&req).expect("encodes");
    let back = o7_sandbox_protocol::request::decode(&bytes).expect("decodes");
    assert_eq!(req, back);
}

#[test]
fn a_truncated_or_oversized_request_is_rejected() {
    let bytes = o7_sandbox_protocol::request::encode(&a_request()).unwrap();
    assert!(o7_sandbox_protocol::request::decode(&bytes[..bytes.len() - 3]).is_err());
    let mut over = ((o7_sandbox_protocol::MAX_REQUEST_BYTES + 1) as u32)
        .to_be_bytes()
        .to_vec();
    over.extend_from_slice(b"{}");
    assert!(matches!(
        o7_sandbox_protocol::request::decode(&over),
        Err(o7_sandbox_protocol::RequestFrameError::TooLarge { .. })
    ));
}

#[test]
fn the_launch_spec_digest_is_env_order_independent_but_argv_order_sensitive() {
    let a = a_request();
    let mut b = a_request();
    b.env.reverse(); // env is a SET
    assert_eq!(
        a.spec_digest(),
        b.spec_digest(),
        "env order must not matter"
    );

    let mut c = a_request();
    c.argv.reverse(); // argv is ORDERED
    assert_ne!(a.spec_digest(), c.spec_digest(), "argv order must matter");

    let mut d = a_request();
    d.cwd = b"/elsewhere".to_vec();
    assert_ne!(a.spec_digest(), d.spec_digest(), "cwd must matter");
}

#[test]
fn the_policy_digest_has_a_frozen_known_answer() {
    // A deliberate, pinned canonical formula. If this vector changes, the policy-binding
    // digest changed and the canonical-bytes/version must move.
    assert_eq!(
        policy().digest().as_str(),
        "f4d350597692561817092fa51c2eca0648a09785a52b9edecdf0dcb620f7580f"
    );
}
