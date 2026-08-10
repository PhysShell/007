//! FD-1.3 encoding and null policy, and the document-level half of FD-1.4.
//!
//! These rules are stated once, globally, because the contract states them once,
//! globally: "One uniform null policy, everywhere in A1", and the FD-1.4 bounds
//! on depth, array length and string length apply to *any* array and *any*
//! string field. Enforcing them as a single pass over the document — before any
//! typed deserialization — means a new payload schema inherits them by existing,
//! rather than by an author remembering a `#[serde(...)]` attribute on each
//! optional field.
//!
//! The rejection is always total. FD-1.4: "Exceeding any bound is a parse-time
//! rejection, never a truncation. A truncated artifact that still parses is the
//! failure mode these bounds exist to forbid."

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::bounds::{MAX_ARRAY_LEN, MAX_JSON_DEPTH, MAX_STRING_BYTES};

/// A payload that is refused at ingest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("payload bytes are not valid UTF-8")]
    NotUtf8,
    #[error("payload begins with a UTF-8 BOM")]
    LeadingBom,
    #[error("payload is not well-formed JSON: {message}")]
    NotJson { message: String },
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
    #[error("payload does not match the schema of its declared kind: {message}")]
    SchemaMismatch { message: String },
}

/// Validate stored payload bytes as an A1 JSON document, without yet knowing
/// which schema they claim to satisfy.
///
/// Applies, in order: UTF-8, no BOM, well-formed JSON with no trailing data,
/// top-level object, then a single walk enforcing the null policy and the
/// document-level bounds.
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
        message: e.to_string(),
    })?;
    if !value.is_object() {
        return Err(ParseError::NotAnObject);
    }
    walk(&value, "$", 1)?;
    Ok(value)
}

/// Validate the document and then deserialize it under a typed schema.
///
/// The typed layer carries `#[serde(deny_unknown_fields)]` and closed enums
/// (FD-1.6), so an unknown field or an unrecognised variant fails here.
///
/// # Errors
/// [`ParseError`] from either the document rules or the schema.
pub fn parse_payload<T: DeserializeOwned>(bytes: &[u8], max_bytes: u64) -> Result<T, ParseError> {
    let value = validate_document(bytes, max_bytes)?;
    serde_json::from_value(value).map_err(|e| ParseError::SchemaMismatch {
        message: e.to_string(),
    })
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
                walk(item, &format!("{path}[{i}]"), depth + 1)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (k, v) in map {
                walk(v, &format!("{path}.{k}"), depth + 1)?;
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
}
