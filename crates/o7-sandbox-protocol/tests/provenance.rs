//! Provenance records DERIVATION; the policy digest records IDENTITY. These tests pin the
//! separation: two different derivations of the same effective confinement must remain
//! indistinguishable to the digest (that is what makes the digest a statement about MEANING)
//! while remaining fully distinguishable in the audit artifact.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::Duration;

use o7_sandbox_protocol::provenance::{
    CliOption, ConfigLocator, EnvName, PolicyField, PolicyKey, PolicyProvenance, PolicySource,
    PolicySources, PROVENANCE_SCHEMA_VERSION,
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
        network: PolicySource::Default,
        env_allowlist: opt("--allow-env"),
        timeout: opt("--timeout"),
    }
}

/// Derivation B: the same values, reached from a config file and the ambient environment.
fn config_sources() -> PolicySources {
    let at = |key: &str, line: u32| PolicySource::Config {
        file: ConfigLocator::parse(Path::new(".007/gate.toml")).unwrap(),
        key: PolicyKey::parse(key).unwrap(),
        line: NonZeroU32::new(line),
    };
    PolicySources {
        worktree: at("sandbox.worktree", 4),
        allow_exec: at("sandbox.allow_exec", 9),
        network: PolicySource::Default,
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
        Some(&PolicySource::Default)
    );
    assert_eq!(
        from_config.source(PolicyField::Network),
        Some(&PolicySource::Default)
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
        file: ConfigLocator::parse(Path::new("o7.toml")).unwrap(),
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
