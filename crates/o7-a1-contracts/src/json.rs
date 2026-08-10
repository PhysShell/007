//! FD-1.3 encoding and null policy, the document-level half of FD-1.4, and the
//! crate's single admission path.
//!
//! These rules are stated once, globally, because the contract states them once,
//! globally: "One uniform null policy, everywhere in A1", and the FD-1.4 bounds
//! on depth, array length and string length apply to *any* array and *any*
//! string field.
//!
//! The rejection is always total. FD-1.4: "Exceeding any bound is a parse-time
//! rejection, never a truncation. A truncated artifact that still parses is the
//! failure mode these bounds exist to forbid."
//!
//! # Two layers, and why neither is sufficient alone
//!
//! ```text
//! validate_document   byte- and value-level rules that only the raw bytes
//!                     can establish: UTF-8, no BOM, payload byte bound,
//!                     nesting depth, array length, string length
//! the typed schema    everything a field's own type can refuse: closed
//!                     enums, unknown fields, frozen versions, scalar
//!                     bounds, and explicit null via `Optional`
//! validate_wire       cross-field rules no single field can see, e.g.
//!                     provider evidence required iff the role is a
//!                     provider role
//! ```
//!
//! [`parse_artifact`] runs all three. The typed layer is deliberately *also*
//! strict on its own, so a caller reaching a wire type through a different door
//! — `serde_json::from_str` — still cannot construct one that violates a
//! per-field rule. Only the byte-level rules and the cross-field rules need the
//! admission path, and both are properties no individual field can check.
//!
//! # Errors never carry payload content
//!
//! AGENTS.md makes credential leakage a P0, and a rejected artifact is untrusted
//! input that may contain anything. So a `ParseError` reports position and
//! shape, never the offending key or value: object members appear in paths as
//! `*` rather than by name, and a schema mismatch reports the serde *category*
//! and location instead of a message that would quote the unknown field or the
//! unrecognised enum variant.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::bounds::{MAX_ARRAY_LEN, MAX_JSON_DEPTH, MAX_STRING_BYTES};

/// A payload that is refused at ingest.
///
/// No variant carries payload content — see the module note.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("payload bytes are not valid UTF-8")]
    NotUtf8,
    #[error("payload begins with a UTF-8 BOM")]
    LeadingBom,
    #[error("payload is not well-formed JSON ({category}) at line {line}, column {column}")]
    NotJson {
        category: &'static str,
        line: usize,
        column: usize,
    },
    #[error("payload top-level value is not an object")]
    NotAnObject,
    #[error("explicit JSON null at {path}: an optional field is absent or carries a value")]
    ExplicitNull { path: String },
    #[error("JSON nesting depth exceeds {MAX_JSON_DEPTH} at {path}")]
    DepthExceeded { path: String },
    #[error("array at {path} has {actual} entries, exceeding {MAX_ARRAY_LEN}")]
    ArrayTooLong { path: String, actual: usize },
    #[error("string at {path} is {actual} bytes, exceeding {MAX_STRING_BYTES}")]
    StringTooLong { path: String, actual: usize },
    #[error("payload exceeds its {max}-byte bound at {actual} bytes")]
    PayloadTooLarge { actual: usize, max: u64 },
    #[error("payload does not match the schema of its declared kind ({category})")]
    SchemaMismatch { category: &'static str },
    #[error("artifact is structurally invalid: {reason}")]
    Invalid { reason: String },
}

/// The cross-field rules of a wire artifact — the ones no single field can
/// check, because they relate two fields to each other.
///
/// Implemented by every envelope-bearing type so [`parse_artifact`] can enforce
/// them without each caller remembering to.
pub trait WireArtifact: Sized {
    /// # Errors
    /// A human-readable reason. Implementations must not quote payload content.
    fn validate_wire(&self) -> Result<(), String>;
}

/// Validate stored payload bytes as an A1 JSON document, without yet knowing
/// which schema they claim to satisfy.
///
/// Applies, in order: the byte bound, UTF-8, no BOM, well-formed JSON with no
/// trailing data, top-level object, then a single walk enforcing nesting depth,
/// array length and string length.
///
/// # Errors
/// [`ParseError`] for the first violation found.
pub fn validate_document(bytes: &[u8], max_bytes: u64) -> Result<Value, ParseError> {
    if bytes.len() as u64 > max_bytes {
        return Err(ParseError::PayloadTooLarge {
            actual: bytes.len(),
            max: max_bytes,
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotUtf8)?;
    // FD-1.3 rejects a leading BOM outright rather than stripping it: stripping
    // would mean the digest names bytes nobody parsed.
    if text.starts_with('\u{feff}') {
        return Err(ParseError::LeadingBom);
    }
    let value: Value = serde_json::from_str(text).map_err(|e| ParseError::NotJson {
        category: classify(&e),
        line: e.line(),
        column: e.column(),
    })?;
    if !value.is_object() {
        return Err(ParseError::NotAnObject);
    }
    walk(&value, "$", 1)?;
    Ok(value)
}

/// The full admission path: document rules, then the typed schema, then the
/// artifact's cross-field rules.
///
/// # Errors
/// [`ParseError`] from any of the three layers.
pub fn parse_artifact<T: DeserializeOwned + WireArtifact>(
    bytes: &[u8],
    max_bytes: u64,
) -> Result<T, ParseError> {
    let parsed: T = parse_payload(bytes, max_bytes)?;
    parsed
        .validate_wire()
        .map_err(|reason| ParseError::Invalid { reason })?;
    Ok(parsed)
}

/// Document rules, then the typed schema — for payloads that have no cross-field
/// rules of their own.
///
/// Prefer [`parse_artifact`] for anything implementing [`WireArtifact`]: this
/// function cannot know that a type has cross-field obligations, and silently
/// skipping them is exactly the gap an admission path exists to close.
///
/// # Errors
/// [`ParseError`] from either the document rules or the schema.
pub fn parse_payload<T: DeserializeOwned>(bytes: &[u8], max_bytes: u64) -> Result<T, ParseError> {
    let value = validate_document(bytes, max_bytes)?;
    serde_json::from_value(value).map_err(|e| ParseError::SchemaMismatch {
        category: classify(&e),
    })
}

/// The serde error *category*, which describes the failure without quoting the
/// input that caused it. `serde_json`'s `Display` embeds the unknown field name
/// and the unrecognised enum variant verbatim, so it is never propagated.
fn classify(e: &serde_json::Error) -> &'static str {
    match e.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    }
}

fn walk(value: &Value, path: &str, depth: usize) -> Result<(), ParseError> {
    if depth > MAX_JSON_DEPTH {
        return Err(ParseError::DepthExceeded {
            path: path.to_owned(),
        });
    }
    match value {
        Value::Null => Err(ParseError::ExplicitNull {
            path: path.to_owned(),
        }),
        Value::String(s) => {
            if s.len() > MAX_STRING_BYTES {
                Err(ParseError::StringTooLong {
                    path: path.to_owned(),
                    actual: s.len(),
                })
            } else {
                Ok(())
            }
        }
        Value::Array(items) => {
            if items.len() > MAX_ARRAY_LEN {
                return Err(ParseError::ArrayTooLong {
                    path: path.to_owned(),
                    actual: items.len(),
                });
            }
            for (i, item) in items.iter().enumerate() {
                // An array index is a position, not content: safe to name.
                walk(item, &format!("{path}[{i}]"), depth + 1)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for v in map.values() {
                // An object member name is untrusted payload content, so the
                // path records that a member was traversed, not which one.
                walk(v, &format!("{path}.*"), depth + 1)?;
            }
            Ok(())
        }
        Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bounds::MAX_CONTROL_ARTIFACT_BYTES;

    fn check(json: &str) -> Result<Value, ParseError> {
        validate_document(json.as_bytes(), MAX_CONTROL_ARTIFACT_BYTES)
    }

    #[test]
    fn a_well_formed_object_passes() {
        assert!(check(r#"{"a":1,"b":[true,"x"],"c":{"d":"e"}}"#).is_ok());
    }

    #[test]
    fn invalid_utf8_is_refused() {
        assert_eq!(
            validate_document(&[0xff, 0xfe, 0x00], MAX_CONTROL_ARTIFACT_BYTES),
            Err(ParseError::NotUtf8)
        );
    }

    #[test]
    fn a_leading_bom_is_refused_not_stripped() {
        assert_eq!(check("\u{feff}{}"), Err(ParseError::LeadingBom));
    }

    #[test]
    fn a_non_object_top_level_is_refused() {
        assert_eq!(check("[]"), Err(ParseError::NotAnObject));
        assert_eq!(check("\"s\""), Err(ParseError::NotAnObject));
        assert_eq!(check("null"), Err(ParseError::NotAnObject));
    }

    #[test]
    fn trailing_data_is_refused() {
        assert!(matches!(check("{} {}"), Err(ParseError::NotJson { .. })));
    }

    #[test]
    fn an_explicit_null_is_refused_anywhere() {
        assert!(matches!(
            check(r#"{"a":null}"#),
            Err(ParseError::ExplicitNull { .. })
        ));
        assert!(matches!(
            check(r#"{"a":{"b":[1,null]}}"#),
            Err(ParseError::ExplicitNull { .. })
        ));
    }

    #[test]
    fn an_absent_optional_is_fine_which_is_the_whole_point() {
        assert!(check(r#"{"present":1}"#).is_ok());
    }

    #[test]
    fn depth_is_bounded() {
        let deep = format!(
            "{}{}{}",
            "{\"a\":".repeat(MAX_JSON_DEPTH + 1),
            "1",
            "}".repeat(MAX_JSON_DEPTH + 1)
        );
        assert!(matches!(
            check(&deep),
            Err(ParseError::DepthExceeded { .. })
        ));
    }

    #[test]
    fn array_length_is_bounded() {
        let items = vec!["1"; MAX_ARRAY_LEN + 1].join(",");
        assert!(matches!(
            check(&format!(r#"{{"a":[{items}]}}"#)),
            Err(ParseError::ArrayTooLong { .. })
        ));
    }

    #[test]
    fn string_length_is_bounded() {
        let long = "x".repeat(MAX_STRING_BYTES + 1);
        assert!(matches!(
            check(&format!(r#"{{"a":"{long}"}}"#)),
            Err(ParseError::StringTooLong { .. })
        ));
    }

    #[test]
    fn a_payload_above_its_byte_bound_is_refused_before_parsing() {
        let big = vec![b'x'; 16];
        assert!(matches!(
            validate_document(&big, 8),
            Err(ParseError::PayloadTooLarge { max: 8, .. })
        ));
    }

    // AGENTS.md P0. A rejected payload is untrusted input; an error that quotes
    // it is a channel for moving a secret into a log.
    #[test]
    fn an_error_never_quotes_an_object_key_or_value() {
        const SECRET_KEY: &str = "ghp_secret_key_name";
        const SECRET_VALUE: &str = "ghp_secret_field_value";

        let null_under_secret_key = format!(r#"{{"{SECRET_KEY}":null}}"#);
        let long_string_under_secret_key = format!(
            r#"{{"{SECRET_KEY}":"{}"}}"#,
            "x".repeat(MAX_STRING_BYTES + 1)
        );
        let secret_value_in_array =
            format!(r#"{{"a":[{}]}}"#, vec!["1"; MAX_ARRAY_LEN + 1].join(","));

        for message in [
            check(&null_under_secret_key).err().map(|e| e.to_string()),
            check(&long_string_under_secret_key)
                .err()
                .map(|e| e.to_string()),
            check(&secret_value_in_array).err().map(|e| e.to_string()),
        ]
        .into_iter()
        .flatten()
        {
            assert!(!message.contains("ghp_"), "error leaked content: {message}");
        }

        // The schema layer is the other half: serde's own Display embeds the
        // unknown field name and the unrecognised enum variant verbatim.
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Strict {
            #[allow(dead_code)]
            known: u32,
        }
        let unknown_field = format!(r#"{{"known":1,"{SECRET_KEY}":"{SECRET_VALUE}"}}"#);
        let err = parse_payload::<Strict>(unknown_field.as_bytes(), MAX_CONTROL_ARTIFACT_BYTES)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(!err.contains("ghp_"), "schema error leaked content: {err}");
        assert!(err.contains("data"), "category should be reported: {err}");
    }

    #[test]
    fn a_syntax_error_reports_position_without_the_source_text() {
        let err = check(r#"{"a":"ghp_secret" "#)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(!err.contains("ghp_"), "{err}");
        assert!(err.contains("line"), "{err}");
    }
}
