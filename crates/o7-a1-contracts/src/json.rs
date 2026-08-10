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
//! [`parse_artifact`] runs all three, and it is the **only** public way to turn
//! bytes into a wire type. There is deliberately no public helper that stops
//! after the schema: a documented "prefer the other one" is a convention, and a
//! convention is what the first version of this module already tried and lost.
//!
//! **There is no second door.** No A1 wire artifact implements `Deserialize`, so
//! `serde_json::from_slice::<EnvelopeV1>` does not compile; the wire mirrors that
//! do implement it are private, and the trait carrying the unchecked
//! constructor is sealed, so it cannot be called or implemented downstream.
//! That matters because two of the three layers cannot be enforced by a value's
//! own type: `max_bytes` is a property of the byte string, not of the value, and
//! the cross-field rules relate fields no single field can see.
//!
//! Three revisions of this module got that wrong in the same shape, each time
//! leaving a route open with a note explaining why the other one was preferable:
//! a public `parse_payload`, then a public `Deserialize`, then a
//! `#[doc(hidden)]` trait method. A convention is not an admission boundary, and
//! `#[doc(hidden)]` is a convention with better formatting.
//!
//! # Errors never carry payload content
//!
//! AGENTS.md makes credential leakage a P0, and a rejected artifact is untrusted
//! input that may contain anything. So a `ParseError` reports position and
//! shape, never the offending key or value: object members appear in paths as
//! `*` rather than by name, and a schema mismatch reports the serde *category*
//! and location instead of a message that would quote the unknown field or the
//! unrecognised enum variant.

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
/// Every A1 wire type implements this, including the ones that have no such
/// rules: those return `Ok(())` explicitly, which states "this type has no
/// cross-field obligations" instead of leaving it to be inferred from a missing
/// impl. Requiring the trait is what lets [`parse_artifact`] be the single
/// public entry point.
///
/// The trait is **sealed**: [`sealed::FromDocument`] is unnameable outside this
/// crate, so no downstream crate can implement `WireArtifact`, and — the part
/// that matters — none can *call* the unchecked constructor either.
pub trait WireArtifact: sealed::FromDocument {
    /// The FD-1.4 per-object hard maximum for **this** artifact type.
    ///
    /// It is an associated const rather than a `parse_artifact` argument
    /// because FD-1.4 calls these "protocol hard maxima" and this crate's own
    /// `bounds` module calls them "a compile-time constant, never
    /// configurable". A caller-supplied ceiling makes a protocol constant into
    /// an operational parameter, and a resolver that passes the evidence-blob
    /// maximum to a control artifact then admits an object sixty-four times
    /// over its bound.
    ///
    /// That was not hypothetical: the regression test proving the byte bound
    /// fires also asserted that raising the bound admits the same 15 MB
    /// envelope. It was written to isolate *which* rule rejected the document
    /// and it documented a bypass instead — the fourth time on this PR that one
    /// of my own tests recorded a hole rather than closing it.
    const MAX_BYTES: u64;

    /// Compile-time guard: this type's ceiling must be one the document parser
    /// can safely materialize. See [`MATERIALISING_PARSER_SAFE_MAX`].
    ///
    /// Evaluated inside [`parse_artifact`], so it fires for any type actually
    /// admitted rather than for any type merely defined.
    const CEILING_IS_PARSEABLE: () = assert!(
        Self::MAX_BYTES <= MATERIALISING_PARSER_SAFE_MAX,
        "this artifact's FD-1.4 ceiling exceeds what validate_document can \
         materialize safely; the document layer needs a bounded parser before \
         an artifact type this large can be admitted"
    );

    /// # Errors
    /// A human-readable reason. Implementations must not quote payload content.
    fn validate_wire(&self) -> Result<(), String>;
}

/// The largest document ceiling [`validate_document`] may be pointed at.
///
/// `validate_document` builds a complete `serde_json::Value` and *then* walks it
/// for the FD-1.4 depth, array-length and string-length bounds. A `Value` costs
/// several times its input in allocations, so "reject at parse time" is
/// currently true of the byte bound and only true after the fact of the
/// structural bounds — a document full of small array elements is materialized
/// before `ArrayTooLong` fires.
///
/// At 1 MiB that overshoot is bounded and unremarkable. At the 64 MiB manifest
/// ceiling it would not be, and this crate has already ruled on that shape of
/// defect once, in `BoundedVec`: a bound that allocates what it is about to
/// reject is not a bound.
///
/// The fix is a bounded visitor that refuses mid-parse, and it belongs with the
/// artifact type that actually carries a 64 MiB ceiling — none exists yet;
/// `EnvelopeV1` is the only [`WireArtifact`] and FD-1.4 caps it at 1 MiB. So
/// rather than leave that as a note somebody has to remember, the constraint is
/// a compile error: adding an artifact type above this ceiling fails to build
/// until the parser is replaced.
pub const MATERIALISING_PARSER_SAFE_MAX: u64 = crate::bounds::MAX_CONTROL_ARTIFACT_BYTES;

/// The unchecked half of admission, behind a seal.
///
/// `from_document` deserializes and nothing else: no byte bound, no
/// cross-field rules. That is correct for a step *inside* [`parse_artifact`]
/// and wrong for anything else, so it must not be reachable from outside.
///
/// It lived on the public trait behind `#[doc(hidden)]` for exactly one review
/// round. `#[doc(hidden)]` suppresses documentation; it does not affect
/// visibility, so `<EnvelopeV1 as WireArtifact>::from_document(value)` compiled
/// fine for any downstream crate and returned an unvalidated envelope. That is
/// the third time on this PR that a door was left open with a note on it, and
/// the note was mistaken for a lock — the same shape as the `parse_payload`
/// helper in round 2 and the public `Deserialize` in round 7.
///
/// A private module with a public trait inside it is the standard sealed-trait
/// pattern: nameable here, unnameable and therefore uncallable anywhere else.
mod sealed {
    use serde_json::Value;

    use super::ParseError;

    pub trait FromDocument: Sized {
        /// Build the artifact from an already document-validated JSON value.
        ///
        /// Implementations deserialize their **private** wire mirror and
        /// convert.
        ///
        /// # Errors
        /// [`ParseError::SchemaMismatch`], carrying the serde error *category*
        /// and never its text — `serde_json`'s own `Display` quotes the unknown
        /// field name and the unrecognised enum value verbatim, and a
        /// credential can arrive as either (AGENTS.md P0).
        fn from_document(value: Value) -> Result<Self, ParseError>;
    }
}

pub(crate) use sealed::FromDocument;

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
/// The byte ceiling comes from `T`, not from the caller — see
/// [`WireArtifact::MAX_BYTES`]. There is no parameter by which a resolver can
/// widen a protocol hard maximum.
///
/// # Errors
/// [`ParseError`] from any of the three layers.
pub fn parse_artifact<T: WireArtifact>(bytes: &[u8]) -> Result<T, ParseError> {
    // Forces evaluation of the const assertion; a `T` whose ceiling the
    // document parser cannot safely materialize fails to compile here.
    let () = T::CEILING_IS_PARSEABLE;
    let value = validate_document(bytes, T::MAX_BYTES)?;
    let parsed = T::from_document(value)?;
    parsed
        .validate_wire()
        .map_err(|reason| ParseError::Invalid { reason })?;
    Ok(parsed)
}

/// The serde error *category*, which describes the failure without quoting the
/// input that caused it. `serde_json`'s `Display` embeds the unknown field name
/// and the unrecognised enum variant verbatim, so it is never propagated.
pub(crate) fn classify(e: &serde_json::Error) -> &'static str {
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
        impl FromDocument for Strict {
            fn from_document(value: Value) -> Result<Self, ParseError> {
                serde_json::from_value(value).map_err(|e| ParseError::SchemaMismatch {
                    category: classify(&e),
                })
            }
        }
        impl WireArtifact for Strict {
            const MAX_BYTES: u64 = MAX_CONTROL_ARTIFACT_BYTES;

            fn validate_wire(&self) -> Result<(), String> {
                Ok(())
            }
        }
        let unknown_field = format!(r#"{{"known":1,"{SECRET_KEY}":"{SECRET_VALUE}"}}"#);
        let err = parse_artifact::<Strict>(unknown_field.as_bytes())
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
