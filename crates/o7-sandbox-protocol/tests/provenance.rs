//! Provenance records DERIVATION; the policy digest records IDENTITY. These tests pin the
//! separation: two different derivations of the same effective confinement must remain
//! indistinguishable to the digest (that is what makes the digest a statement about MEANING)
//! while remaining fully distinguishable in the audit artifact.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use o7_sandbox_protocol::provenance::{
    CliOption, ConfigAnchor, ConfigLocator, EnvName, PolicyField, PolicyKey, PolicyProvenance,
    PolicySource, PolicySources, PROVENANCE_SCHEMA_VERSION,
};
use o7_sandbox_protocol::{NetworkPolicy, SandboxPolicy};

/// One effective policy. Both derivations below produce exactly this.
fn effective_policy() -> SandboxPolicy {
    SandboxPolicy {
        worktree: PathBuf::from("/srv/o7/wt/run-1"),
        allow_exec: vec![PathBuf::from("/usr/bin/git")],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec!["PATH".into()],
        timeout: Duration::from_secs(30),
    }
}

/// Derivation A: everything named on the command line.
fn cli_sources() -> PolicySources {
    let opt = |s: &str| PolicySource::Cli {
        option: CliOption::parse(s).unwrap(),
    };
    PolicySources {
        worktree: opt("--worktree"),
        allow_exec: opt("--allow-exec"),
        network: PolicySource::Default {},
        env_allowlist: opt("--allow-env"),
        timeout: opt("--timeout"),
    }
}

/// Derivation B: the same values, reached from a config file and the ambient environment.
fn config_sources() -> PolicySources {
    let at = |key: &str, line: u32| PolicySource::Config {
        anchor: ConfigAnchor::TargetRepo,
        file: ConfigLocator::parse(".007/gate.toml").unwrap(),
        key: PolicyKey::parse(key).unwrap(),
        line: NonZeroU32::new(line),
    };
    PolicySources {
        worktree: at("sandbox.worktree", 4),
        allow_exec: at("sandbox.allow_exec", 9),
        network: PolicySource::Default {},
        env_allowlist: PolicySource::Environment {
            name: EnvName::parse("PATH").unwrap(),
        },
        timeout: at("sandbox.timeout_ms", 17),
    }
}

/// THE test the whole slice exists for: semantic identity is provenance-independent.
#[test]
fn the_same_effective_policy_digests_identically_under_different_derivations() {
    let policy = effective_policy();
    policy.validate().expect("the fixture policy is valid");

    let from_cli = PolicyProvenance::describe(&policy, &cli_sources());
    let from_config = PolicyProvenance::describe(&policy, &config_sources());

    // Identity: unchanged. Provenance does not enter `canonical_bytes`, so it cannot move the
    // digest — a policy means what it means regardless of how it was spelled into existence.
    assert_eq!(from_cli.policy_digest, policy.digest());
    assert_eq!(from_config.policy_digest, policy.digest());
    assert_eq!(from_cli.policy_digest, from_config.policy_digest);

    // Derivation: fully distinguishable. This is the question `policy_digest` structurally
    // cannot answer, which is why the artifact exists at all.
    assert_ne!(
        from_cli, from_config,
        "two different derivations must produce different provenance"
    );
    assert_ne!(
        from_cli.source(PolicyField::Timeout),
        from_config.source(PolicyField::Timeout)
    );

    // ...and the part both derivations agree on stays equal: nobody asked for deny-all
    // networking in either case.
    assert_eq!(
        from_cli.source(PolicyField::Network),
        Some(&PolicySource::Default {})
    );
    assert_eq!(
        from_config.source(PolicyField::Network),
        Some(&PolicySource::Default {})
    );
}

#[test]
fn describe_records_an_origin_for_every_policy_field() {
    let provenance = PolicyProvenance::describe(&effective_policy(), &cli_sources());
    assert_eq!(
        provenance.missing_fields(),
        Vec::new(),
        "a built record is total over PolicyField::ALL"
    );
    for field in PolicyField::ALL {
        assert!(
            provenance.source(field).is_some(),
            "{field:?} must have a recorded origin"
        );
    }
    assert_eq!(provenance.schema_version, PROVENANCE_SCHEMA_VERSION);
}

#[test]
fn explains_joins_a_record_to_the_policy_it_describes() {
    let policy = effective_policy();
    let provenance = PolicyProvenance::describe(&policy, &cli_sources());
    assert!(provenance.explains(&policy));

    let mut other = policy.clone();
    other.timeout = Duration::from_secs(31);
    assert!(
        !provenance.explains(&other),
        "a record must not claim to explain a different policy"
    );
}

#[test]
fn the_serialized_artifact_is_byte_stable_and_round_trips() {
    let provenance = PolicyProvenance::describe(&effective_policy(), &config_sources());
    let once = serde_json::to_string_pretty(&provenance).unwrap();
    let twice = serde_json::to_string_pretty(&provenance).unwrap();
    assert_eq!(once, twice, "serialization must be deterministic");

    let parsed: PolicyProvenance = serde_json::from_str(&once).unwrap();
    assert_eq!(parsed, provenance);

    // The field map is a BTreeMap, so the artifact's key order is the enum's declared order,
    // not the caller's insertion order.
    let value: serde_json::Value = serde_json::from_str(&once).unwrap();
    let keys: Vec<&str> = value
        .get("fields")
        .and_then(serde_json::Value::as_object)
        .expect("fields is an object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "field keys are emitted in a canonical order");
}

/// A `line: None` config source must not emit a null placeholder — the artifact records what
/// is known, and says nothing where nothing is known.
#[test]
fn an_unknown_config_line_is_omitted_rather_than_nulled() {
    let source = PolicySource::Config {
        anchor: ConfigAnchor::TargetRepo,
        file: ConfigLocator::parse("o7.toml").unwrap(),
        key: PolicyKey::parse("sandbox.network").unwrap(),
        line: None,
    };
    let text = serde_json::to_string(&source).unwrap();
    assert!(!text.contains("line"), "got {text}");
    assert_eq!(
        serde_json::from_str::<PolicySource>(&text).unwrap(),
        source,
        "an omitted line round-trips back to None"
    );
}

/// Corrective round. The leaf newtypes only ever policed KNOWN fields; serde accepts unknown
/// ones by default, so every representation below could carry a payload beside a valid
/// identifier and have it silently dropped — leaving a reader to call the file well-formed.
/// `deny_unknown_fields` closes that, and `Default` had to become an empty STRUCT variant to
/// be covered at all: under internal tagging serde deserializes a unit variant by ignoring the
/// rest of the map, so the attribute never reached it.
#[test]
fn a_payload_beside_a_valid_identifier_is_rejected_not_silently_dropped() {
    for (label, json) in [
        (
            "environment + value",
            r#"{"source":"environment","name":"PATH","value":"sk-live-secret"}"#,
        ),
        (
            "cli + argument",
            r#"{"source":"cli","option":"--allow-exec","argument":"/etc/shadow"}"#,
        ),
        (
            "config + contents",
            r#"{"source":"config","anchor":"target_repo","file":"o7.toml","key":"a","contents":"[secrets]\ntoken=…"}"#,
        ),
        (
            "config + value",
            r#"{"source":"config","anchor":"target_repo","file":"o7.toml","key":"a","value":"sk-live"}"#,
        ),
        (
            "default + arbitrary payload",
            r#"{"source":"default","raw":"sk-live-secret"}"#,
        ),
    ] {
        assert!(
            serde_json::from_str::<PolicySource>(json).is_err(),
            "{label} must be rejected, not parsed with the payload dropped"
        );
    }

    // ...and the same at the top level.
    let digest = "0".repeat(64);
    assert!(
        serde_json::from_str::<PolicyProvenance>(&format!(
            r#"{{"schema_version":1,"policy_digest":"{digest}","fields":{{}},"leaked_env":"FOO=bar"}}"#
        ))
        .is_err(),
        "an unknown top-level field must be rejected"
    );

    // The legitimate shapes still parse — this closes a hole, it does not close the door.
    for json in [
        r#"{"source":"default"}"#,
        r#"{"source":"cli","option":"--allow-exec"}"#,
        r#"{"source":"environment","name":"PATH"}"#,
        r#"{"source":"config","anchor":"target_repo","file":".007/gate.toml","key":"sandbox.timeout_ms","line":17}"#,
    ] {
        assert!(
            serde_json::from_str::<PolicySource>(json).is_ok(),
            "{json} is a legitimate source and must still parse"
        );
    }
}

/// Corrective round. `Component::Normal` accepts nearly any byte string, so a platform `Path`
/// was never a grammar — `FOO=secret` was a perfectly good one-component "relative path", and
/// a Windows-shaped path is not a `Prefix` on Linux, just a name containing backslashes. The
/// replacement is an ASCII allowlist over `/`-separated segments.
#[test]
fn a_config_locator_is_an_identifier_grammar_not_a_platform_path() {
    for bad in [
        "ANTHROPIC_API_KEY=sk-live-secret", // an assignment posing as a filename
        r"C:\Users\alice\secret\o7.toml",   // not a Prefix on Linux — just backslashes
        r"\\server\share\secret\o7.toml",
        "foo\nsecret",
        "has space",
        "a:b",
        "/home/alice/customer-secret/o7.toml",
        "../../etc/o7.toml",
        "./o7.toml",
        "foo//bar",
        "foo/",
        "..",
        "",
    ] {
        assert!(
            ConfigLocator::parse(bad).is_err(),
            "{bad:?} must not parse as a config locator"
        );
    }

    for good in [".007/gate.toml", "o7.toml", "a/b/c-d_e.2.toml"] {
        assert!(
            ConfigLocator::parse(good).is_ok(),
            "{good:?} is a legitimate locator"
        );
    }

    // The untrusted deserialize path enforces the same grammar.
    assert!(serde_json::from_str::<ConfigLocator>(r#""FOO=secret""#).is_err());
    assert!(serde_json::from_str::<ConfigLocator>(r#""/etc/passwd""#).is_err());
    assert!(serde_json::from_str::<ConfigLocator>(r#"".007/gate.toml""#).is_ok());
}

/// Corrective round. A locator relative to nothing in particular answers "where did this come
/// from?" with "somewhere around here" — so the anchor is explicit and travels with it.
#[test]
fn a_config_source_declares_what_its_locator_is_relative_to() {
    let source = PolicySource::Config {
        anchor: ConfigAnchor::TargetRepo,
        file: ConfigLocator::parse(".007/gate.toml").unwrap(),
        key: PolicyKey::parse("sandbox.timeout_ms").unwrap(),
        line: NonZeroU32::new(17),
    };
    let value = serde_json::to_value(&source).unwrap();
    assert_eq!(
        value.get("anchor").and_then(serde_json::Value::as_str),
        Some("target_repo")
    );
    // An anchorless config source is not representable.
    assert!(serde_json::from_str::<PolicySource>(
        r#"{"source":"config","file":"o7.toml","key":"a"}"#
    )
    .is_err());
}

/// Corrective round. A v1 reader must not report a document written to a schema it has never
/// seen as one it understood.
#[test]
fn an_unsupported_schema_version_is_visible_rather_than_assumed_understood() {
    let provenance = PolicyProvenance::describe(&effective_policy(), &cli_sources());
    assert!(provenance.is_supported_version());

    let mut text = serde_json::to_value(&provenance).unwrap();
    if let Some(object) = text.as_object_mut() {
        object.insert("schema_version".into(), serde_json::json!(999));
    }
    let future: PolicyProvenance = serde_json::from_value(text).unwrap();
    assert!(
        !future.is_supported_version(),
        "a v999 document must not claim to be understood by a v1 reader"
    );
}
