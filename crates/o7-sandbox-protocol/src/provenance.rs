//! [`PolicyProvenance`] — WHERE a [`SandboxPolicy`]'s values came from, as audit metadata that
//! is structurally incapable of reproducing the sources it names.
//!
//! [`SandboxPolicy::digest`] answers *identity*: which confinement was installed. It cannot
//! answer *derivation*: why this exec allowance is here, which config line set the timeout,
//! whether the network posture was defaulted or asked for. Two derivations that produce the
//! same effective policy are — correctly — indistinguishable by digest, because the digest
//! binds the policy's MEANING. Provenance is the separate artifact that records the difference
//! the digest deliberately erases.
//!
//! # The dependency direction is one-way, and that is the point
//!
//! ```text
//! inputs / defaults / config
//!           │
//!           ├──────────────► PolicyProvenance     (audit metadata)
//!           │
//!           ▼
//!      SandboxPolicy ──► validate ──► canonical_bytes ──► digest ──► execution
//! ```
//!
//! Nothing in the admission or execution path reads provenance. It is NON-LOAD-BEARING by
//! construction: a missing, truncated, or malformed provenance artifact cannot change a
//! policy, a verdict, or a replay result, because no code on those paths consults it. This
//! module therefore adds no new fail-closed surface — it has no authority to fail closed
//! ABOUT. (Deliberately absent for the same reason: a `provenance_digest`. Anything that gets
//! a digest eventually gets checked, and a check is a trust dependency. If provenance ever
//! becomes part of a provable contract, that is a separate, deliberate change that moves this
//! artifact INTO the replay path.)
//!
//! # The invariant: provenance may IDENTIFY a source, never REPRODUCE it
//!
//! This repository is public and forbids committing environment dumps or credential-bearing
//! artifacts (`docs/public-governance.md`). A provenance record is exactly the artifact that
//! would be tempted to write `source: "FOO=hunter2"` or quote a config file's contents, and a
//! free-form `String` payload is an open invitation to do so — "the type allowed it" is how
//! that lands six months from now.
//!
//! So no variant of [`PolicySource`] carries free-form content. Every leaf is a validated,
//! length-bounded newtype whose grammar admits an *identifier* and rejects a *payload*:
//!
//! - [`EnvName`] accepts a POSIX-portable variable NAME and rejects `=`, so the `NAME=VALUE`
//!   shape cannot be smuggled through the name field;
//! - [`ConfigLocator`] accepts a `/`-separated locator over an ASCII allowlist, anchored by an
//!   explicit [`ConfigAnchor`], so it can carry neither `/home/alice/customer-secret/...` nor
//!   a segment that is really an assignment;
//! - [`PolicyKey`] names a key, [`CliOption`] names a long flag — neither takes the argument
//!   that was passed to it.
//!
//! Known fields are only half of it. Serde accepts UNKNOWN fields by default, so every
//! representation here is `deny_unknown_fields`: without it a hand-written
//! `{"source":"environment","name":"PATH","value":"sk-live-…"}` parses cleanly, drops the
//! secret on the floor, and gets reported as a well-formed record — an armoured door beside an
//! open window.
//!
//! The bound is structural, not a sanitizer. If this module ever needs to scrub its own output
//! before writing it, the design has already failed.
//!
//! [`SandboxPolicy::digest`]: crate::policy::SandboxPolicy::digest

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize};

use crate::ids::Digest256;
use crate::policy::SandboxPolicy;

/// The provenance artifact's own schema version, independent of the wire
/// [`SCHEMA_VERSION`](crate::SCHEMA_VERSION): provenance is a local record-dir artifact and
/// never crosses the backend boundary, so it versions on its own schedule.
pub const PROVENANCE_SCHEMA_VERSION: u32 = 1;

/// Length ceilings. An identifier is short; a ceiling is the crude-but-structural half of
/// "identify, not reproduce" — it stops a leaf from becoming a place to park a blob even if
/// the blob happens to satisfy the charset.
const MAX_CLI_OPTION: usize = 64;
const MAX_POLICY_KEY: usize = 128;
const MAX_ENV_NAME: usize = 128;
const MAX_CONFIG_LOCATOR: usize = 256;

/// Why a provenance leaf could not be built. Every variant means the same thing: the input
/// was a payload where an identifier was required.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProvenanceError {
    #[error("{0:?} is not a long option of the form `--lower-kebab` (max {MAX_CLI_OPTION} chars)")]
    CliOption(String),
    #[error("{0:?} is not a dotted lower_snake config key (max {MAX_POLICY_KEY} chars)")]
    PolicyKey(String),
    /// Notably rejects anything containing `=`, so `NAME=VALUE` cannot pose as a name.
    #[error("{0:?} is not a POSIX-portable environment variable NAME (max {MAX_ENV_NAME} chars)")]
    EnvName(String),
    /// Rejects `=`, `\\`, `:`, whitespace, control characters, and non-ASCII by construction:
    /// the segment charset is an allowlist, not a denylist of known-bad shapes.
    #[error(
        "{0:?} is not a config locator: `/`-separated non-empty segments over [A-Za-z0-9._-], \
         none of them `.` or `..`, at most {MAX_CONFIG_LOCATOR} chars"
    )]
    ConfigLocator(String),
}

/// A long CLI option NAME (`--allow-exec`) — never the argument passed to it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CliOption(String);

impl CliOption {
    /// Parse a long option name: `--`, then lowercase ASCII / digits / single inner hyphens.
    ///
    /// # Errors
    /// [`ProvenanceError::CliOption`] if the string is not that shape.
    pub fn parse(value: &str) -> Result<Self, ProvenanceError> {
        let err = || ProvenanceError::CliOption(value.to_owned());
        if value.len() > MAX_CLI_OPTION {
            return Err(err());
        }
        let body = value.strip_prefix("--").ok_or_else(err)?;
        let first = body.chars().next().ok_or_else(err)?;
        if !first.is_ascii_lowercase()
            || body.ends_with('-')
            || body.contains("--")
            || !body
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(err());
        }
        Ok(CliOption(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A dotted config key (`sandbox.timeout_ms`) — never the value stored under it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct PolicyKey(String);

impl PolicyKey {
    /// Parse a dotted `lower_snake` key path.
    ///
    /// # Errors
    /// [`ProvenanceError::PolicyKey`] if any segment is empty or outside `[a-z_][a-z0-9_]*`.
    pub fn parse(value: &str) -> Result<Self, ProvenanceError> {
        let err = || ProvenanceError::PolicyKey(value.to_owned());
        if value.is_empty() || value.len() > MAX_POLICY_KEY {
            return Err(err());
        }
        for segment in value.split('.') {
            let first = segment.chars().next().ok_or_else(err)?;
            if !(first.is_ascii_lowercase() || first == '_')
                || !segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(err());
            }
        }
        Ok(PolicyKey(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An environment variable NAME. The value is not representable here, and the charset
/// excludes `=`, so a `NAME=VALUE` pair cannot be parked in this field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EnvName(String);

impl EnvName {
    /// Parse a POSIX-portable variable name: `[A-Za-z_][A-Za-z0-9_]*`.
    ///
    /// # Errors
    /// [`ProvenanceError::EnvName`] otherwise — including anything containing `=`.
    pub fn parse(value: &str) -> Result<Self, ProvenanceError> {
        let err = || ProvenanceError::EnvName(value.to_owned());
        if value.is_empty() || value.len() > MAX_ENV_NAME {
            return Err(err());
        }
        let first = value.chars().next().ok_or_else(err)?;
        if !(first.is_ascii_alphabetic() || first == '_')
            || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(err());
        }
        Ok(EnvName(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a [`ConfigLocator`] is relative TO.
///
/// A relative path with no declared anchor answers "where did this come from?" with
/// "somewhere around here, probably" — which is not an answer an audit artifact may give.
/// One variant, because 007 has exactly one config root today; the same shape as
/// [`NetworkPolicy::DenyAll`](crate::policy::NetworkPolicy), and a second one is added when a
/// second config root actually exists, not in anticipation of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigAnchor {
    /// The TARGET repository root — the repo `o7 run --repo` points at, whose `.007/gate.toml`
    /// supplies the run's manifest. NOT the run record directory: no config lives there.
    TargetRepo,
}

/// A locator for the config file a value came from — a coordinate, not the file. Meaningful
/// only together with a [`ConfigAnchor`], which is why [`PolicySource::Config`] carries both.
///
/// # Grammar
///
/// A platform `Path` is the wrong grammar for an audit artifact and was the wrong grammar
/// here: `Component::Normal` accepts nearly any byte string, so `ANTHROPIC_API_KEY=sk-live-…`
/// parsed as a perfectly good single-component "relative path", and `C:\Users\alice\secret\`
/// is not a Windows prefix on Linux — just a relative component with backslashes in the name.
/// Both sailed through an is-absolute plus reject-`..` check.
///
/// So this is a PORTABLE grammar, not a path: `/`-separated segments over `[A-Za-z0-9._-]`,
/// each non-empty and neither `.` nor `..`. That admits `.007/gate.toml` and rejects `=`,
/// `\`, `:`, spaces, control characters, and non-ASCII by construction — the charset is an
/// allowlist, so a shape nobody anticipated is refused rather than accepted by default.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ConfigLocator(String);

impl ConfigLocator {
    /// Parse a `/`-separated locator over the portable segment charset.
    ///
    /// # Errors
    /// [`ProvenanceError::ConfigLocator`] for an empty locator, a leading/trailing/doubled
    /// `/`, a `.` or `..` segment, any character outside `[A-Za-z0-9._-]`, or an over-long
    /// locator.
    pub fn parse(value: &str) -> Result<Self, ProvenanceError> {
        let err = || ProvenanceError::ConfigLocator(value.to_owned());
        if value.is_empty() || value.len() > MAX_CONFIG_LOCATOR {
            return Err(err());
        }
        for segment in value.split('/') {
            // An empty segment covers the leading `/` (absolute), a trailing `/`, and `//`.
            if segment.is_empty() || segment == "." || segment == ".." {
                return Err(err());
            }
            if !segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
            {
                return Err(err());
            }
        }
        Ok(ConfigLocator(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Re-validate on the untrusted deserialize path, exactly as [`Digest256`] does: a
/// hand-edited or third-party provenance file must not be able to introduce a leaf shape the
/// constructors forbid.
macro_rules! validating_deserialize {
    ($ty:ident, $parse:expr) => {
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                #[allow(clippy::redundant_closure_call)]
                ($parse)(&raw).map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

validating_deserialize!(CliOption, CliOption::parse);
validating_deserialize!(PolicyKey, PolicyKey::parse);
validating_deserialize!(EnvName, EnvName::parse);
validating_deserialize!(ConfigLocator, ConfigLocator::parse);

/// Where one policy value came from. Every variant names its source; no variant can carry the
/// source's CONTENTS.
///
/// `deny_unknown_fields` is part of that guarantee, not tidiness. Serde accepts unknown fields
/// by default, so without it `{"source":"environment","name":"PATH","value":"sk-live-…"}`
/// deserializes happily into `Environment { name: "PATH" }` with the secret silently dropped —
/// and a reader would then report the file as well-formed. The leaf newtypes make a payload
/// unrepresentable in a KNOWN field; this makes it unrepresentable in an unknown one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicySource {
    /// The value nobody asked for — the built-in safe default (e.g. deny-all networking).
    ///
    /// An empty STRUCT variant, not a unit variant, and the difference is load-bearing: under
    /// internal tagging serde deserializes a unit variant by ignoring the rest of the map, so
    /// `deny_unknown_fields` does not reach it and `{"source":"default","raw":"sk-live-…"}`
    /// would parse. The wire form is identical either way (`{"source":"default"}`); only the
    /// unknown-field behaviour differs.
    Default {},
    /// Set by a long CLI option. The option is named; its argument is not recorded (it is
    /// already visible in the effective policy itself, canonically and digest-bound).
    Cli { option: CliOption },
    /// Set by a config file, identified by an anchor plus a locator relative to it, the key
    /// path, and optionally the line — a coordinate into the file, never an excerpt of it.
    Config {
        anchor: ConfigAnchor,
        file: ConfigLocator,
        key: PolicyKey,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        line: Option<NonZeroU32>,
    },
    /// Taken from the ambient environment, by variable NAME only.
    Environment { name: EnvName },
}

/// Which [`SandboxPolicy`] field a [`PolicySource`] explains.
///
/// One variant per policy field. [`PolicyProvenance::describe`] destructures `SandboxPolicy`
/// exhaustively, so a new policy field cannot be added without also being given an origin
/// here — the same compile-time authority link `sandbox_dimensions!` establishes between the
/// dimension list and the report's trust predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyField {
    Worktree,
    AllowExec,
    Network,
    EnvAllowlist,
    Timeout,
}

impl PolicyField {
    /// Every field, for iteration and totality checks.
    pub const ALL: [PolicyField; 5] = [
        PolicyField::Worktree,
        PolicyField::AllowExec,
        PolicyField::Network,
        PolicyField::EnvAllowlist,
        PolicyField::Timeout,
    ];
}

/// One declared origin per [`SandboxPolicy`] field — the input to
/// [`PolicyProvenance::describe`]. Total by construction: there is no `Option`, so a caller
/// cannot build provenance that silently omits a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySources {
    pub worktree: PolicySource,
    pub allow_exec: PolicySource,
    pub network: PolicySource,
    pub env_allowlist: PolicySource,
    pub timeout: PolicySource,
}

/// The derivation record for one effective [`SandboxPolicy`].
///
/// `policy_digest` is a JOIN KEY for audit — it says which policy this explains — and is not
/// a check anything performs before executing. Nothing consults this struct to decide what to
/// enforce; see the module docs for the one-way dependency this preserves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyProvenance {
    /// The schema this record claims. NOT validated on deserialize — see
    /// [`Self::is_supported_version`] for why the check lives at the reader instead.
    pub schema_version: u32,
    /// The digest of the effective policy this explains.
    pub policy_digest: Digest256,
    /// Field → origin. A `BTreeMap` so the serialized artifact is byte-stable for a given
    /// derivation regardless of insertion order.
    pub fields: BTreeMap<PolicyField, PolicySource>,
}

impl PolicyProvenance {
    /// Record where `policy`'s values came from.
    ///
    /// Takes the ALREADY-BUILT policy: provenance describes a derivation that has already
    /// happened and cannot influence it.
    #[must_use]
    pub fn describe(policy: &SandboxPolicy, sources: &PolicySources) -> Self {
        // Exhaustive destructures with NO `..` in either pattern. A new `SandboxPolicy` field
        // fails to build here (`pattern requires ..`) until it is given a `PolicySources`
        // entry and a `PolicyField` variant — so a policy field cannot exist with no recorded
        // origin. DO NOT add `..` to loosen this; the exhaustiveness IS the link.
        let SandboxPolicy {
            worktree: _,
            allow_exec: _,
            network: _,
            env_allowlist: _,
            timeout: _,
        } = policy;
        let PolicySources {
            worktree,
            allow_exec,
            network,
            env_allowlist,
            timeout,
        } = sources;

        let mut fields = BTreeMap::new();
        fields.insert(PolicyField::Worktree, worktree.clone());
        fields.insert(PolicyField::AllowExec, allow_exec.clone());
        fields.insert(PolicyField::Network, network.clone());
        fields.insert(PolicyField::EnvAllowlist, env_allowlist.clone());
        fields.insert(PolicyField::Timeout, timeout.clone());

        PolicyProvenance {
            schema_version: PROVENANCE_SCHEMA_VERSION,
            policy_digest: policy.digest(),
            fields,
        }
    }

    /// Whether this record's declared schema is the one this build understands.
    ///
    /// Deliberately a QUERY rather than a deserialize-time rejection. Parsing a v999 document
    /// into a v1 struct succeeds whenever the fields happen to line up, and silently reporting
    /// it as understood is the actual defect: a v1 reader would be claiming to have read a
    /// document written to a schema it has never seen. Rejecting at parse would instead
    /// collapse "from the future" into "corrupt", losing the distinction an operator needs.
    /// So the type parses version-agnostically and the reader classifies — which keeps the
    /// whole thing diagnostic, exactly like every other outcome here.
    #[must_use]
    pub fn is_supported_version(&self) -> bool {
        self.schema_version == PROVENANCE_SCHEMA_VERSION
    }

    /// The recorded origin of one field, if this record has one.
    #[must_use]
    pub fn source(&self, field: PolicyField) -> Option<&PolicySource> {
        self.fields.get(&field)
    }

    /// Fields with no recorded origin. Always empty for a record built by
    /// [`Self::describe`]; a non-empty result means a hand-edited or truncated artifact.
    ///
    /// DIAGNOSTIC ONLY — a caller may report this, and must not gate execution on it.
    #[must_use]
    pub fn missing_fields(&self) -> Vec<PolicyField> {
        PolicyField::ALL
            .into_iter()
            .filter(|f| !self.fields.contains_key(f))
            .collect()
    }

    /// Whether this record claims to explain `policy`.
    ///
    /// AUDIT JOIN, NOT A TRUST CHECK: `false` means "this provenance is about some other
    /// policy", which is a reason to report a mismatched artifact — never a reason to refuse
    /// to run a policy that already validated on its own terms.
    #[must_use]
    pub fn explains(&self, policy: &SandboxPolicy) -> bool {
        self.policy_digest == policy.digest()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_cli_option_is_a_long_flag_name_only() {
        assert!(CliOption::parse("--allow-exec").is_ok());
        assert!(CliOption::parse("--timeout").is_ok());
        for bad in [
            "--allow-exec=/usr/bin/git", // the argument is not part of the NAME
            "-x",
            "allow-exec",
            "--",
            "--Allow-Exec",
            "--allow--exec",
            "--allow-exec-",
            "--allow exec",
        ] {
            assert!(
                CliOption::parse(bad).is_err(),
                "{bad:?} must not parse as a CLI option name"
            );
        }
    }

    #[test]
    fn an_env_name_cannot_carry_a_value() {
        assert!(EnvName::parse("PATH").is_ok());
        assert!(EnvName::parse("http_proxy").is_ok());
        assert!(EnvName::parse("_UNDERSCORE1").is_ok());
        // The whole point: the `NAME=VALUE` shape is unrepresentable, so a well-meaning
        // caller cannot record a secret by "just putting the assignment in the name".
        for bad in [
            "ANTHROPIC_API_KEY=sk-live-abcdef",
            "PATH=/usr/bin",
            "=VALUE",
            "1LEADING_DIGIT",
            "HAS SPACE",
            "HAS-HYPHEN",
            "",
        ] {
            assert!(
                EnvName::parse(bad).is_err(),
                "{bad:?} must not parse as an env NAME"
            );
        }
    }

    #[test]
    fn a_config_locator_cannot_carry_an_absolute_host_path() {
        assert_eq!(
            ConfigLocator::parse(".007/gate.toml").unwrap().as_str(),
            ".007/gate.toml"
        );
        for bad in [
            "/home/alice/customer-secret/o7.toml", // absolute host paths are disclosure
            "../../etc/o7.toml",
            "./o7.toml",
            "",
        ] {
            assert!(
                ConfigLocator::parse(bad).is_err(),
                "{bad:?} must not parse as a record-relative locator"
            );
        }
    }

    #[test]
    fn a_policy_key_names_a_key_not_a_value() {
        assert!(PolicyKey::parse("sandbox.timeout_ms").is_ok());
        assert!(PolicyKey::parse("network").is_ok());
        for bad in ["sandbox.", ".timeout", "Sandbox.Timeout", "a..b", ""] {
            assert!(PolicyKey::parse(bad).is_err(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn leaf_newtypes_revalidate_on_the_untrusted_deserialize_path() {
        // A hand-edited artifact must not be able to reintroduce a shape the constructors
        // reject — serde is a parser here, not a bypass.
        assert!(serde_json::from_str::<EnvName>(r#""FOO=secret""#).is_err());
        assert!(serde_json::from_str::<ConfigLocator>(r#""/etc/passwd""#).is_err());
        assert!(serde_json::from_str::<CliOption>(r#""--x=1""#).is_err());
        assert!(serde_json::from_str::<PolicyKey>(r#""A.B""#).is_err());
        assert_eq!(
            serde_json::from_str::<EnvName>(r#""PATH""#).unwrap(),
            EnvName::parse("PATH").unwrap()
        );
    }

    #[test]
    fn every_serialized_source_variant_names_a_source_and_carries_no_payload_field() {
        // Structural check on the wire shape: the object keys are exactly the identifying
        // ones. A future variant that adds a contents-bearing field fails here.
        let cases = [
            (PolicySource::Default {}, vec!["source"]),
            (
                PolicySource::Cli {
                    option: CliOption::parse("--allow-exec").unwrap(),
                },
                vec!["source", "option"],
            ),
            (
                PolicySource::Config {
                    anchor: ConfigAnchor::TargetRepo,
                    file: ConfigLocator::parse(".007/gate.toml").unwrap(),
                    key: PolicyKey::parse("sandbox.timeout_ms").unwrap(),
                    line: NonZeroU32::new(17),
                },
                vec!["source", "anchor", "file", "key", "line"],
            ),
            (
                PolicySource::Environment {
                    name: EnvName::parse("PATH").unwrap(),
                },
                vec!["source", "name"],
            ),
        ];
        for (source, expected) in cases {
            let value = serde_json::to_value(&source).unwrap();
            let object = value.as_object().expect("a source serializes as an object");
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            let mut expected = expected;
            expected.sort_unstable();
            assert_eq!(keys, expected, "unexpected key set for {source:?}");
            for forbidden in ["value", "contents", "raw", "argument", "data", "text"] {
                assert!(
                    !object.contains_key(forbidden),
                    "{source:?} must not carry a {forbidden:?} field"
                );
            }
        }
    }
}
