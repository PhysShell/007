//! §3 shared scalar types.
//!
//! `Digest256` is re-exported from `o7-run` rather than redefined: FD-1.1 names
//! `o7_run::event::Digest256::of_bytes` as *the* form for an A1 payload digest,
//! and a parallel type would be a second spelling of one truth — the failure
//! FD-2.4 rejects for imported kinds, applied to a scalar.
//!
//! # Constraints live in the types, not in a validation step someone must run
//!
//! Every type here rejects an inadmissible value **at deserialization**, so
//! there is no path — including `serde_json::from_str` on a wire type — that
//! produces a value violating its §3 constraint. A rule enforced only by a
//! separate `validate()` call is a rule that holds until the first caller
//! forgets, and a wire contract does not get to depend on that.
//!
//! # Errors never echo payload content
//!
//! A rejected artifact is untrusted input, and AGENTS.md forbids agent output
//! that may contain secrets from reaching logs or error strings. So these errors
//! report *shape* — lengths, positions, expectations — and never the offending
//! value. An error that quotes what it refused is a channel for moving the thing
//! it refused somewhere it is written down.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize, Serializer};

use crate::bounds::{MAX_ARRAY_LEN, MAX_ID_BYTES, MAX_STRING_BYTES};

pub use o7_run::event::{Digest256, DigestFormatError};

/// §3 — RFC 3339 timestamps are metadata only; this is a generous shape bound,
/// not a calendar check (FD-5.4).
pub const MAX_TIMESTAMP_BYTES: usize = 64;

/// A scalar that violates its §3 constraint.
///
/// No variant carries the rejected value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScalarError {
    #[error("an Id must be a non-empty UTF-8 string")]
    EmptyId,
    #[error("an Id must be <= {MAX_ID_BYTES} bytes, got {actual}")]
    IdTooLong { actual: usize },
    #[error("a text field must be <= {max} bytes, got {actual}")]
    TextTooLong { actual: usize, max: usize },
    #[error("a CommitId must be a non-empty full object id, never abbreviated")]
    EmptyCommitId,
    #[error("a CommitId must not be abbreviated: got {actual} chars, expected 40 or 64")]
    AbbreviatedCommitId { actual: usize },
    #[error("a CommitId must be lowercase hex; a non-hex byte appears at offset {offset}")]
    NonHexCommitId { offset: usize },
    #[error("a Digest256 must be exactly 64 lowercase hex chars, got a {actual}-byte string")]
    MalformedDigest { actual: usize },
    #[error("a bounded collection must have <= {max} entries, got {actual}")]
    TooManyEntries { actual: usize, max: usize },
}

/// §3 — "opaque non-empty UTF-8 string, never parsed for meaning", ≤ 256 bytes.
///
/// The "never parsed for meaning" half is enforced by having no accessor that
/// invites it: an `Id` compares and frames, and that is all.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// # Errors
    /// [`ScalarError`] if empty or over the FD-1.4 opaque-id bound.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.is_empty() {
            return Err(ScalarError::EmptyId);
        }
        if s.len() > MAX_ID_BYTES {
            return Err(ScalarError::IdTooLong { actual: s.len() });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — UTF-8 string bounded by `MAX` bytes.
///
/// The bound is a type parameter rather than a runtime argument so a
/// schema-specific limit — `producer_adapter_version` at 128 bytes,
/// `model_identity` at 256 (§3.0) — is enforced by the field's own type on every
/// deserialization path, not by a validator a caller may skip.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct BoundedText<const MAX: usize>(String);

impl<const MAX: usize> BoundedText<MAX> {
    /// # Errors
    /// [`ScalarError::TextTooLong`] above `MAX` (or above the global FD-1.4
    /// string bound, whichever is smaller).
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        let max = if MAX < MAX_STRING_BYTES {
            MAX
        } else {
            MAX_STRING_BYTES
        };
        if s.len() > max {
            return Err(ScalarError::TextTooLong {
                actual: s.len(),
                max,
            });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for BoundedText<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — `Text`: UTF-8 string, ≤ 65536 bytes unless a schema states tighter.
pub type Text = BoundedText<MAX_STRING_BYTES>;

/// §3.0 — `producer_adapter_version`, ≤ 128 bytes.
pub type AdapterVersion = BoundedText<128>;

/// §3.0 — `model_identity`, ≤ 256 bytes. "Logical identity for routing/policy,
/// **not** runtime evidence."
pub type ModelIdentity = BoundedText<256>;

/// §3 — "full object id, the repository's object-format width", never
/// abbreviated (`docs/decision-and-admission-protocol.md` §4).
///
/// Both widths are accepted because the repository's object format is the
/// repository's to choose; what is refused is a *prefix*, which is the actual
/// hazard — a seven-character id resolves differently in two clones.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct CommitId(String);

impl CommitId {
    /// # Errors
    /// [`ScalarError`] if empty, abbreviated, or not lowercase hex.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.is_empty() {
            return Err(ScalarError::EmptyCommitId);
        }
        if s.len() != 40 && s.len() != 64 {
            return Err(ScalarError::AbbreviatedCommitId {
                actual: s.chars().count(),
            });
        }
        for (offset, b) in s.bytes().enumerate() {
            if !(b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(ScalarError::NonHexCommitId { offset });
            }
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CommitId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — RFC 3339 UTC, metadata only (FD-5.4), excluded from every framing.
///
/// Deliberately not parsed into a calendar type. It authorizes nothing, nothing
/// in A1 orders by it, and a rich type would invite exactly the reasoning FD-5.4
/// forbids.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// # Errors
    /// [`ScalarError::TextTooLong`] above [`MAX_TIMESTAMP_BYTES`]; the value is
    /// otherwise opaque.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.len() > MAX_TIMESTAMP_BYTES {
            return Err(ScalarError::TextTooLong {
                actual: s.len(),
                max: MAX_TIMESTAMP_BYTES,
            });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// A [`Digest256`] as it appears on the wire.
///
/// **Not a second digest type.** It holds the same `o7_run::event::Digest256`,
/// computes nothing of its own, and hands the identical value to every framing.
/// It exists for one reason: that type's `Deserialize` forwards
/// `DigestFormatError`, whose `Display` quotes the rejected string. A rejected
/// artifact is untrusted input, and AGENTS.md makes credential leakage a P0, so
/// `serde_json::from_slice::<EnvelopeV1>` on a payload whose `contract_digest`
/// is a stolen token must not produce an error containing it.
///
/// The wrapper is therefore a redacting deserializer, not a parallel model —
/// which is the distinction FD-2.4 draws when it imports a kind rather than
/// redefining it.
///
/// Note that `o7-run`'s own `DigestFormatError` still carries the rejected value
/// for its own callers; narrowing that is a change to another crate's public
/// error type and belongs to its own slice, not to this one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct WireDigest(Digest256);

impl WireDigest {
    /// # Errors
    /// [`ScalarError::MalformedDigest`], which names the shape expected and not
    /// the value received.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        Digest256::parse(s)
            .map(Self)
            .map_err(|_| ScalarError::MalformedDigest { actual: s.len() })
    }

    /// Hash bytes into a wire digest — the FD-1.1 form.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Digest256::of_bytes(bytes))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The underlying `o7-run` digest, for code that already speaks that type.
    #[must_use]
    pub fn inner(&self) -> &Digest256 {
        &self.0
    }
}

impl From<Digest256> for WireDigest {
    fn from(d: Digest256) -> Self {
        Self(d)
    }
}

impl<'de> Deserialize<'de> for WireDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// FD-1.6 — a version field frozen at exactly `V`.
///
/// "An unrecognized value of any of them is refused; the artifact is never
/// parsed 'as well as we can'." Making that a type rather than a check means a
/// deserialized artifact **cannot** carry an unsupported version: there is no
/// value of this type that represents one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FrozenVersion<const V: u32>;

impl<const V: u32> FrozenVersion<V> {
    /// The frozen value, for framing (FD-1.2 frames it as `to_le_bytes`).
    pub const VALUE: u32 = V;

    #[must_use]
    pub fn get(self) -> u32 {
        V
    }
}

impl<const V: u32> Serialize for FrozenVersion<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(V)
    }
}

impl<'de, const V: u32> Deserialize<'de> for FrozenVersion<V> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = u32::deserialize(deserializer)?;
        if raw == V {
            Ok(Self)
        } else {
            // The number is the peer's own version claim, not payload content:
            // safe to name, and useless to withhold.
            Err(serde::de::Error::custom(format!(
                "unsupported version {raw}, expected {V}: refused rather than parsed as well as we can (FD-1.6)"
            )))
        }
    }
}

/// A collection bounded by `MAX` entries, enforced at deserialization.
///
/// A container bound is neither a byte-level rule nor a cross-field rule, so it
/// belongs in neither the admission path nor `validate()`: it is a property of
/// the field itself, and a field's properties are enforced by its type. Before
/// this existed, `serde_json::from_slice::<EnvelopeV1>` could build an envelope
/// with more `artifact_refs` than FD-1.4 permits, because the count was checked
/// only by a validator that path never reaches.
///
/// The bound is also capped by the global FD-1.4 array maximum, so no schema can
/// widen it by choosing a larger `MAX`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedVec<T, const MAX: usize>(Vec<T>);

impl<T, const MAX: usize> BoundedVec<T, MAX> {
    /// # Errors
    /// [`ScalarError::TooManyEntries`] above `MAX` (or above the global FD-1.4
    /// array bound, whichever is smaller).
    pub fn new(items: Vec<T>) -> Result<Self, ScalarError> {
        let max = if MAX < MAX_ARRAY_LEN {
            MAX
        } else {
            MAX_ARRAY_LEN
        };
        if items.len() > max {
            return Err(ScalarError::TooManyEntries {
                actual: items.len(),
                max,
            });
        }
        Ok(Self(items))
    }

    /// An empty collection — always admissible.
    #[must_use]
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }
}

impl<'a, T, const MAX: usize> IntoIterator for &'a BoundedVec<T, MAX> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T, const MAX: usize> TryFrom<Vec<T>> for BoundedVec<T, MAX> {
    type Error = ScalarError;

    fn try_from(items: Vec<T>) -> Result<Self, Self::Error> {
        Self::new(items)
    }
}

impl<T, const MAX: usize> BoundedVec<T, MAX> {
    /// The effective cap: the schema's own `MAX`, never above the global FD-1.4
    /// array maximum.
    const fn cap() -> usize {
        if MAX < MAX_ARRAY_LEN {
            MAX
        } else {
            MAX_ARRAY_LEN
        }
    }
}

impl<'de, T: Deserialize<'de>, const MAX: usize> Deserialize<'de> for BoundedVec<T, MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserializing the whole sequence and *then* checking the cap would let
        // an oversized array allocate every element — including its owned
        // strings — before being refused. FD-1.4 wants a parse-time rejection,
        // and a rejection that happens after the memory is spent is only a
        // report. So the visitor stops one element past the cap, and the
        // capacity hint is clamped so a declared length cannot preallocate
        // either.
        struct BoundedSeq<T, const MAX: usize>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>, const MAX: usize> serde::de::Visitor<'de> for BoundedSeq<T, MAX> {
            type Value = BoundedVec<T, MAX>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "a sequence of at most {} entries",
                    BoundedVec::<T, MAX>::cap()
                )
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let cap = BoundedVec::<T, MAX>::cap();
                let hint = seq.size_hint().unwrap_or(0);
                let mut items: Vec<T> = Vec::with_capacity(if hint < cap { hint } else { cap });

                while items.len() < cap {
                    match seq.next_element::<T>()? {
                        Some(item) => items.push(item),
                        None => return Ok(BoundedVec(items)),
                    }
                }

                // At the cap. `SeqAccess` cannot tell "another element" from
                // "end of sequence" without consuming input, so the overflow is
                // read as `IgnoredAny`: the deserializer walks past the value
                // without building it, so a 257th entry carrying a megabyte of
                // string costs nothing to refuse. Deserializing it as `T` first
                // would allocate exactly what the bound exists to prevent.
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "a bounded collection must have <= {cap} entries"
                    )));
                }
                Ok(BoundedVec(items))
            }
        }

        deserializer.deserialize_seq(BoundedSeq::<T, MAX>(PhantomData))
    }
}

/// FD-1.3 — an optional A1 field: **absent, or carrying a value**.
///
/// "Explicit JSON `null` is rejected at parse time. There is no field in this
/// contract where `null` and absent mean different things, because that
/// distinction has never once been worth what it costs."
///
/// `Option<T>` cannot express that: serde maps `null` to `None`, so a type using
/// it silently accepts the one encoding the contract forbids. This type has no
/// representation for `null`, which is why the rejection cannot be bypassed by
/// reaching the typed schema through a different door.
///
/// Use with `#[serde(default, skip_serializing_if = "Optional::is_absent")]`:
/// serde then supplies the default when the key is absent and calls this
/// deserializer only when the key is present — so a `null` token here always
/// means an explicit null was written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Optional<T>(Option<T>, PhantomData<T>);

impl<T> Default for Optional<T> {
    fn default() -> Self {
        Self(None, PhantomData)
    }
}

impl<T> Optional<T> {
    #[must_use]
    pub fn absent() -> Self {
        Self(None, PhantomData)
    }

    #[must_use]
    pub fn present(value: T) -> Self {
        Self(Some(value), PhantomData)
    }

    #[must_use]
    pub fn is_absent(&self) -> bool {
        self.0.is_none()
    }

    #[must_use]
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }

    #[must_use]
    pub fn as_ref(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

impl<T> From<Option<T>> for Optional<T> {
    fn from(v: Option<T>) -> Self {
        Self(v, PhantomData)
    }
}

impl<T: Serialize> Serialize for Optional<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            // Only reachable when a caller serializes without
            // `skip_serializing_if`; emitting a null would produce bytes this
            // crate refuses to read back, so the write side fails instead.
            None => Err(serde::ser::Error::custom(
                "an absent optional is omitted, never written as null (FD-1.3)",
            )),
            Some(v) => v.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // `Option<T>` maps a null token to `None`. Because the field carries
        // `#[serde(default)]`, an absent key never reaches this function — so
        // `None` here can only mean an explicit null was on the wire.
        match Option::<T>::deserialize(deserializer)? {
            Some(v) => Ok(Self::present(v)),
            None => Err(serde::de::Error::custom(
                "explicit JSON null is rejected (FD-1.3): an optional field is absent or carries a value",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_bounded_and_non_empty() {
        assert_eq!(Id::parse(""), Err(ScalarError::EmptyId));
        assert!(Id::parse("m-1").is_ok());
        assert!(matches!(
            Id::parse(&"x".repeat(MAX_ID_BYTES + 1)),
            Err(ScalarError::IdTooLong { .. })
        ));
        assert!(Id::parse(&"x".repeat(MAX_ID_BYTES)).is_ok());
    }

    #[test]
    fn a_commit_id_may_not_be_abbreviated() {
        assert!(CommitId::parse(&"a".repeat(40)).is_ok());
        assert!(CommitId::parse(&"a".repeat(64)).is_ok());
        assert!(matches!(
            CommitId::parse("a4a9f97"),
            Err(ScalarError::AbbreviatedCommitId { actual: 7 })
        ));
    }

    #[test]
    fn a_commit_id_must_be_lowercase_hex() {
        assert!(matches!(
            CommitId::parse(&"A".repeat(40)),
            Err(ScalarError::NonHexCommitId { offset: 0 })
        ));
        assert!(matches!(
            CommitId::parse(&format!("{}z{}", "a".repeat(10), "a".repeat(29))),
            Err(ScalarError::NonHexCommitId { offset: 10 })
        ));
    }

    // AGENTS.md P0: nothing that may carry a secret reaches an error string. A
    // rejected value is untrusted input, so the error names shape only.
    #[test]
    fn a_scalar_error_never_quotes_the_rejected_value() {
        let secret = "ghp_totally_a_secret_value_that_must_not_be_logged";
        for message in [
            CommitId::parse(secret).err().map(|e| e.to_string()),
            Id::parse(&secret.repeat(20)).err().map(|e| e.to_string()),
            Text::parse(&secret.repeat(2000))
                .err()
                .map(|e| e.to_string()),
            Timestamp::parse(&secret.repeat(4))
                .err()
                .map(|e| e.to_string()),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !message.contains("ghp_"),
                "error leaked the rejected value: {message}"
            );
        }
    }

    #[test]
    fn a_bounded_text_enforces_its_schema_specific_limit_on_the_wire() {
        assert!(
            serde_json::from_str::<AdapterVersion>(&format!("\"{}\"", "x".repeat(128))).is_ok()
        );
        assert!(
            serde_json::from_str::<AdapterVersion>(&format!("\"{}\"", "x".repeat(129))).is_err()
        );
        assert!(serde_json::from_str::<ModelIdentity>(&format!("\"{}\"", "x".repeat(256))).is_ok());
        assert!(
            serde_json::from_str::<ModelIdentity>(&format!("\"{}\"", "x".repeat(257))).is_err()
        );
    }

    #[test]
    fn a_timestamp_is_bounded_on_the_wire_not_only_through_parse() {
        let long = format!("\"{}\"", "x".repeat(MAX_TIMESTAMP_BYTES + 1));
        assert!(serde_json::from_str::<Timestamp>(&long).is_err());
        assert!(serde_json::from_str::<Timestamp>("\"2026-08-09T20:00:00Z\"").is_ok());
    }

    #[test]
    fn a_frozen_version_refuses_any_other_value() {
        assert!(serde_json::from_str::<FrozenVersion<1>>("1").is_ok());
        assert!(serde_json::from_str::<FrozenVersion<1>>("2").is_err());
        assert!(serde_json::from_str::<FrozenVersion<1>>("0").is_err());
        assert_eq!(
            serde_json::to_string(&FrozenVersion::<1>).unwrap_or_default(),
            "1"
        );
    }

    #[test]
    fn an_optional_rejects_an_explicit_null() {
        #[derive(Debug, Deserialize)]
        struct Holder {
            #[serde(default)]
            field: Optional<String>,
        }
        let absent: Holder = serde_json::from_str("{}").unwrap_or_else(|_| Holder {
            field: Optional::present("wrong".to_owned()),
        });
        assert!(absent.field.is_absent());

        let present: Holder = serde_json::from_str(r#"{"field":"v"}"#).unwrap_or_else(|_| Holder {
            field: Optional::absent(),
        });
        assert!(present.field.is_present());

        assert!(serde_json::from_str::<Holder>(r#"{"field":null}"#).is_err());
    }

    #[test]
    fn a_digest_is_the_o7_run_form() {
        let d = Digest256::of_bytes(b"");
        assert_eq!(d.as_str().len(), 64);
        assert!(Digest256::parse(d.as_str()).is_ok());
        assert!(Digest256::parse("nope").is_err());
    }
}
