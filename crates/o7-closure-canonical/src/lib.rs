//! RFC 8785 (JCS) canonicalization and SHA-256 digests for closure evidence.
//!
//! `docs/architecture/closure-source-provenance-v1.md` §7 freezes the rule this
//! crate implements:
//!
//! ```text
//! canonical form  RFC 8785 JSON Canonicalization Scheme (JCS)
//! digest          SHA-256 over the canonical UTF-8 bytes
//! digest syntax   sha256:[lowercase-hex]
//! ```
//!
//! SCOPE — bytes only. This crate knows nothing about source kinds, projections,
//! retention or matchers. It turns a `serde_json::Value` into the exact bytes the
//! contracts hash, or refuses. Everything above that is somebody else's layer.
//!
//! WHAT IT REFUSES, AND WHY THAT IS THE WHOLE POINT. JCS specifies ECMAScript
//! `Number::toString` for non-integer numbers. Implementing that correctly is a
//! well-known trap, and no closure evidence in this repository contains one — every
//! numeric value in the frozen corpora is an integer, and §7 already requires ids to
//! be carried as strings so the canonical form cannot depend on a number
//! implementation limit. So V1 canonicalizes integers and **refuses** anything else
//! rather than approximating. A wrong digest is worse than a refused one: it is
//! evidence that authenticates the wrong bytes.

use std::fmt;

use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

/// A `sha256:[lowercase-hex]` digest, per §7's frozen syntax.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(String);

impl Digest {
    /// Parse a digest in the frozen syntax. Rejects anything else, including
    /// uppercase hex — the syntax says lowercase, and two spellings of one value
    /// would be two digests for one fact.
    pub fn parse(text: &str) -> Result<Self, CanonicalError> {
        let hex = text
            .strip_prefix("sha256:")
            .ok_or_else(|| CanonicalError::MalformedDigest {
                text: text.to_owned(),
                why: "missing the sha256: prefix",
            })?;
        if hex.len() != 64 {
            return Err(CanonicalError::MalformedDigest {
                text: text.to_owned(),
                why: "a sha256 digest is 64 hex characters",
            });
        }
        if !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(CanonicalError::MalformedDigest {
                text: text.to_owned(),
                why: "hex must be lowercase 0-9a-f",
            });
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// JCS defines non-integer numbers by ECMAScript `Number::toString`. V1 does
    /// not implement it, and refuses rather than guessing.
    NonIntegerNumber {
        rendered: String,
    },
    /// RFC 8785 numbers are IEEE-754 doubles, so an integer outside
    /// ±(2^53 − 1) has no exact canonical form. A conforming implementation
    /// refuses it; one that emits the decimal anyway produces a digest no
    /// conforming verifier will reproduce.
    IntegerOutsideSafeDomain {
        value: i128,
    },
    MalformedDigest {
        text: String,
        why: &'static str,
    },
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonIntegerNumber { rendered } => write!(
                f,
                "cannot canonicalize the non-integer number {rendered}: V1 implements \
                 JCS for integers only and refuses rather than approximate ECMAScript \
                 Number::toString"
            ),
            Self::IntegerOutsideSafeDomain { value } => write!(
                f,
                "cannot canonicalize the integer {value}: RFC 8785 numbers are \
                 IEEE-754 doubles, so only the safe domain +/-(2^53 - 1) has an \
                 exact canonical form"
            ),
            Self::MalformedDigest { text, why } => {
                write!(f, "malformed digest {text:?}: {why}")
            }
        }
    }
}

impl std::error::Error for CanonicalError {}

/// The exact bytes §7 hashes.
pub fn canonicalize(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out.into_bytes())
}

/// SHA-256 over the canonical bytes, in the frozen `sha256:` syntax.
pub fn digest(value: &Value) -> Result<Digest, CanonicalError> {
    let bytes = canonicalize(value)?;
    Ok(digest_of_canonical_bytes(&bytes))
}

/// SHA-256 over bytes already known to be canonical. Separate from [`digest`] so
/// a verifier holding retained bytes can hash exactly those, rather than
/// re-serializing a parse of them and hashing its own opinion.
pub fn digest_of_canonical_bytes(bytes: &[u8]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Digest(format!("sha256:{:x}", hasher.finalize()))
}

fn write_value(value: &Value, out: &mut String) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => write_number(n, out)?,
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => write_object(map, out)?,
    }
    Ok(())
}

/// The largest magnitude an IEEE-754 double represents exactly: 2^53 - 1.
const SAFE_INTEGER_LIMIT: i128 = 9_007_199_254_740_991;

fn write_number(n: &serde_json::Number, out: &mut String) -> Result<(), CanonicalError> {
    let value: i128 = if let Some(u) = n.as_u64() {
        i128::from(u)
    } else if let Some(i) = n.as_i64() {
        i128::from(i)
    } else {
        return Err(CanonicalError::NonIntegerNumber {
            rendered: n.to_string(),
        });
    };
    // Found by differential testing against rfc8785 before this crate shipped:
    // the reference implementation refuses 9007199254740993, and an earlier
    // revision here emitted it. An i64 is wider than the domain JCS defines.
    if value.abs() > SAFE_INTEGER_LIMIT {
        return Err(CanonicalError::IntegerOutsideSafeDomain { value });
    }
    out.push_str(&value.to_string());
    Ok(())
}

/// RFC 8785 §3.2.3: members are sorted by the **UTF-16 code units** of their
/// names, not by code point and not by UTF-8 bytes. The three orders agree on
/// ASCII and diverge above the BMP, which is why this is written out rather than
/// delegated to `String`'s `Ord`.
fn write_object(map: &Map<String, Value>, out: &mut String) -> Result<(), CanonicalError> {
    let mut members: Vec<(Vec<u16>, &String, &Value)> = map
        .iter()
        .map(|(k, v)| (k.encode_utf16().collect(), k, v))
        .collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));

    out.push('{');
    for (i, (_, key, value)) in members.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(key, out);
        out.push(':');
        write_value(value, out)?;
    }
    out.push('}');
    Ok(())
}

/// RFC 8785 §3.2.2.2: the two-character escapes where they exist, `\u00xx` with
/// lowercase hex for the remaining control characters, and every other code point
/// literal as UTF-8 — no `\/`, and no escaping of non-ASCII.
fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{0009}' => out.push_str("\\t"),
            '\u{000a}' => out.push_str("\\n"),
            '\u{000c}' => out.push_str("\\f"),
            '\u{000d}' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
