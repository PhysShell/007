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

    /// Compile-time guard: this type's ceiling must be one FD-1.4 actually
    /// names.
    ///
    /// Until 2A this guard said something weaker and more embarrassing — that
    /// the ceiling had to fit what the *materialising* document walk could
    /// afford, which capped every artifact at 1 MiB and made
    /// `InteractionManifestV1` unimplementable. That limitation is gone:
    /// [`crate::scan`] refuses during the parse and allocates O(depth). So the
    /// bound that remains is the contract's, not the implementation's.
    ///
    /// Note what did **not** happen: a constant was not raised to unblock a
    /// type. The parser changed, and the guard changed with it, which is the
    /// only order in which this is anything other than a number edit.
    ///
    /// Evaluated inside [`parse_artifact`], so it fires for any type actually
    /// admitted rather than for any type merely defined.
    const CEILING_IS_PARSEABLE: () = assert!(
        Self::MAX_BYTES <= crate::bounds::MAX_EVIDENCE_BLOB_BYTES,
        "this artifact's ceiling exceeds the largest per-object bound FD-1.4 \
         defines; a bound above every bound in the contract is not a bound"
    );

    /// # Errors
    /// A human-readable reason. Implementations must not quote payload content.
    fn validate_wire(&self) -> Result<(), String>;
}

/// The unchecked half of admission, behind a seal.
///
/// `from_bytes` deserializes and nothing else: no structural scan, no
/// cross-field rules. That is correct for a step *inside* [`parse_artifact`]
/// and wrong for anything else, so it must not be reachable from outside.
///
/// It lived on the public trait behind `#[doc(hidden)]` for exactly one review
/// round. `#[doc(hidden)]` suppresses documentation; it does not affect
/// visibility, so `<EnvelopeV1 as WireArtifact>::from_bytes(bytes)` compiled
/// fine for any downstream crate and returned an unvalidated envelope. That is
/// the third time on this PR that a door was left open with a note on it, and
/// the note was mistaken for a lock — the same shape as the `parse_payload`
/// helper in round 2 and the public `Deserialize` in round 7.
///
/// A private module with a public trait inside it is the standard sealed-trait
/// pattern: nameable here, unnameable and therefore uncallable anywhere else.
mod sealed {
    use super::ParseError;

    pub trait FromDocument: Sized {
        /// Build the artifact from bytes a structural scan has already
        /// admitted (`crate::scan`).
        ///
        /// Takes the bytes, not a `serde_json::Value`: materialising a value
        /// first was the thing FD-1.4's parse-time rejection could not survive
        /// at the 64 MiB ceiling. Implementations deserialize their **private**
        /// wire mirror straight from the slice and convert.
        ///
        /// # Errors
        /// [`ParseError::SchemaMismatch`], carrying the serde error *category*
        /// and never its text — `serde_json`'s own `Display` quotes the unknown
        /// field name and the unrecognised enum value verbatim, and a
        /// credential can arrive as either (AGENTS.md P0).
        fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError>;
    }
}

pub(crate) use sealed::FromDocument;

/// Admit stored payload bytes as an A1 JSON document, structurally.
///
/// Applies, in order: the byte bound, UTF-8, no BOM, then **one streaming
/// pass** enforcing top-level object, well-formedness with no trailing data,
/// nesting depth, array length, string length and the null policy.
///
/// It returns nothing. The previous version returned a `serde_json::Value` it
/// had materialised in order to walk it, which is what kept FD-1.4's
/// parse-time rejection from being true of the structural bounds — see
/// [`crate::scan`]. Producing no value also means this layer cannot become a
/// second route to a typed schema, whatever it is later pointed at.
///
/// **Crate-private, and that is load-bearing.** It takes a caller-supplied
/// ceiling, so exposing it would put back the protocol maximum that
/// [`WireArtifact::MAX_BYTES`] took away.
///
/// # Errors
/// [`ParseError`] for the first violation found.
pub(crate) fn admit_document(bytes: &[u8], max_bytes: u64) -> Result<(), ParseError> {
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
    crate::scan::Scanner::new(text).run()
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
    admit_document(bytes, T::MAX_BYTES)?;
    let parsed = T::from_bytes(bytes)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // The pre-2A materialising walk, kept **only** as a test oracle. 2A must
    // change when a bound fires, never which documents are admitted, so the
    // new streaming scan is cross-checked against the implementation it
    // replaced rather than against my memory of it. It is unreachable from
    // production and returns no value, so it is a comparison and not a route.
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

    use crate::bounds::MAX_CONTROL_ARTIFACT_BYTES;

    fn check(json: &str) -> Result<(), ParseError> {
        admit_document(json.as_bytes(), MAX_CONTROL_ARTIFACT_BYTES)
    }

    /// What production now does: structural scan, then serde over the same
    /// bytes. `parse_artifact` differs only in deserializing a typed schema
    /// rather than a `Value`.
    fn pipeline(json: &str) -> Result<(), ParseError> {
        admit_document(json.as_bytes(), MAX_CONTROL_ARTIFACT_BYTES)?;
        serde_json::from_str::<Value>(json)
            .map(|_| ())
            .map_err(|e| ParseError::NotJson {
                category: classify(&e),
                line: e.line(),
                column: e.column(),
            })
    }

    /// The pre-2A verdict for the same document, via the materialising oracle.
    fn oracle(json: &str) -> Result<(), ParseError> {
        if json.len() as u64 > MAX_CONTROL_ARTIFACT_BYTES {
            return Err(ParseError::PayloadTooLarge {
                actual: json.len(),
                max: MAX_CONTROL_ARTIFACT_BYTES,
            });
        }
        if json.starts_with('\u{feff}') {
            return Err(ParseError::LeadingBom);
        }
        let value: Value = serde_json::from_str(json).map_err(|e| ParseError::NotJson {
            category: classify(&e),
            line: e.line(),
            column: e.column(),
        })?;
        if !value.is_object() {
            return Err(ParseError::NotAnObject);
        }
        walk(&value, "$", 1)
    }

    #[test]
    fn a_well_formed_object_passes() {
        assert!(check(r#"{"a":1,"b":[true,"x"],"c":{"d":"e"}}"#).is_ok());
    }

    #[test]
    fn invalid_utf8_is_refused() {
        assert_eq!(
            admit_document(&[0xff, 0xfe, 0x00], MAX_CONTROL_ARTIFACT_BYTES),
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
            admit_document(&big, 8),
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
            fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
                serde_json::from_slice(bytes).map_err(|e| ParseError::SchemaMismatch {
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

    /// Compare the streaming scan against the walk it replaced.
    ///
    /// **Accept/reject must always agree.** That is the property 2A is not
    /// allowed to change: an optimisation that quietly admits or refuses a
    /// different set of documents is a contract change in a performance
    /// costume.
    ///
    /// The *reason* is allowed to differ in exactly one way, and it is a
    /// deliberate consequence rather than a concession. The old path ran
    /// whole-document syntax first and structure second, so for a document that
    /// is both malformed *and* structurally invalid it always reported the
    /// syntax error. The new one refuses at the first violation in document
    /// order, so `{"a":null, ...syntax error at column 100}` now reports the
    /// null. Both refuse; FD-1.4 requires the rejection and says nothing about
    /// which of two violations is named, and reporting the earlier one is the
    /// whole point of scanning.
    ///
    /// So the reason is compared exactly whenever neither side says `NotJson`,
    /// and when one does, the other must still be a structural refusal rather
    /// than something unrelated.
    ///
    /// **Two divergences are declared rather than compared**, each with its own
    /// test rather than an exemption buried here:
    ///
    /// - *error precedence* (this paragraph): the old path ran whole-document
    ///   syntax first, the new one reports the first violation in document
    ///   order.
    /// - *duplicate member names*: the old path deduped them inside
    ///   `serde_json::Value` before any rule ran, and admitted the artifact.
    ///   The new path refuses it. That is an accept/reject change, argued and
    ///   pinned in `duplicate_member_names_are_refused_which_the_walk_did_not_do`
    ///   — so documents containing duplicates are kept out of the corpus
    ///   passed to this function.
    ///
    /// The comparison is against the **pipeline**, not against the scan alone.
    /// The scan is a structural gate, not a second complete JSON parser: it
    /// deliberately does not re-implement serde's numeric policy, so
    /// `{"a":27e7942173}` passes the scan and is refused by serde a moment
    /// later, exactly as `parse_artifact` runs it. Comparing the scan alone
    /// would have pushed this crate toward owning two full parsers that must
    /// agree forever — the duplication that FD-1.2 avoids for digests and that
    /// step 1 spent a round removing from encoding.
    fn agree(document: &str) {
        let scanned = pipeline(document);
        let walked = oracle(document);
        assert_eq!(
            scanned.is_ok(),
            walked.is_ok(),
            "verdict changed for {document:?}: {scanned:?} vs {walked:?}"
        );
        let (Err(a), Err(b)) = (&scanned, &walked) else {
            return;
        };
        let syntax = |e: &ParseError| matches!(e, ParseError::NotJson { .. });
        if syntax(a) || syntax(b) {
            let structural = |e: &ParseError| {
                matches!(
                    e,
                    ParseError::NotJson { .. }
                        | ParseError::ExplicitNull { .. }
                        | ParseError::DepthExceeded { .. }
                        | ParseError::ArrayTooLong { .. }
                        | ParseError::StringTooLong { .. }
                        | ParseError::NotAnObject
                )
            };
            assert!(
                structural(a) && structural(b),
                "one side refused for an unrelated reason on {document:?}: {a:?} vs {b:?}"
            );
            return;
        }
        assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "refusal reason changed for {document:?}: {a:?} vs {b:?}"
        );
    }

    /// 2A must change **when** a bound fires, never **which** documents are
    /// admitted. So the streaming scan is compared against the materialising
    /// walk it replaced, on every shape the old suite covered plus the edges.
    ///
    /// An optimisation that quietly changes a verdict is a contract change
    /// wearing a performance costume.
    #[test]
    fn the_streaming_scan_agrees_with_the_walk_it_replaced() {
        let long = "x".repeat(MAX_STRING_BYTES + 1);
        let at_bound = "x".repeat(MAX_STRING_BYTES);
        let deep = format!(
            "{}{}{}",
            "{\"a\":".repeat(MAX_JSON_DEPTH + 1),
            "1",
            "}".repeat(MAX_JSON_DEPTH + 1)
        );
        let wide = vec!["1"; MAX_ARRAY_LEN + 1].join(",");
        let at_array_bound = vec!["1"; MAX_ARRAY_LEN].join(",");
        let corpus = [
            r#"{}"#.to_owned(),
            r#"{"a":1,"b":[true,"x"],"c":{"d":"e"}}"#.to_owned(),
            r#"{"a":null}"#.to_owned(),
            r#"{"a":{"b":[1,null]}}"#.to_owned(),
            r#"{"a":[]}"#.to_owned(),
            r#"{"a":-1.5e-3}"#.to_owned(),
            r#"{"a":"\u00e9\n\"quoted\""}"#.to_owned(),
            r#"{"a":"é"}"#.to_owned(),
            "[]".to_owned(),
            "\"s\"".to_owned(),
            "null".to_owned(),
            "1".to_owned(),
            "true".to_owned(),
            "not json at all".to_owned(),
            "{".to_owned(),
            "{} {}".to_owned(),
            r#"{"a":"unterminated"#.to_owned(),
            r#"{"a" 1}"#.to_owned(),
            r#"{a:1}"#.to_owned(),
            r#"{"a":[1,]}"#.to_owned(),
            r#"{"a":01}"#.to_owned(),
            "\u{feff}{}".to_owned(),
            format!(r#"{{"a":"{long}"}}"#),
            format!(r#"{{"a":"{at_bound}"}}"#),
            deep,
            format!(r#"{{"a":[{wide}]}}"#),
            format!(r#"{{"a":[{at_array_bound}]}}"#),
        ];

        for document in corpus {
            agree(&document);
        }
    }

    /// The corpus above is what I thought to write down; this is the part that
    /// finds what I did not.
    ///
    /// A deterministic generator builds documents, mutates them, and asserts
    /// the streaming scan and the walk it replaced reach the same verdict. It
    /// caught two real grammar bugs on its first run that the hand-written
    /// corpus missed — a top-level `n` mistaken for `null`, and a leading zero
    /// accepted where RFC 8259 forbids it — which is the entire argument for
    /// having it. Seeded, so a failure reproduces exactly.
    #[test]
    fn the_streaming_scan_agrees_with_the_walk_it_replaced_under_mutation() {
        struct Rng(u64);
        impl Rng {
            fn next(&mut self) -> u64 {
                // A plain LCG. Determinism is the requirement here, not
                // statistical quality: a differential test that cannot be
                // replayed reports a failure nobody can reproduce.
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
                self.0 >> 33
            }
            fn below(&mut self, n: u64) -> u64 {
                self.next() % n
            }
        }

        fn build(rng: &mut Rng, depth: usize, out: &mut String) {
            match rng.below(if depth >= 4 { 5 } else { 7 }) {
                0 => out.push_str("true"),
                1 => out.push_str("false"),
                2 => out.push_str("null"),
                3 => out.push_str(&format!("{}", rng.next() as i64 - 1_000_000)),
                4 => out.push_str(&format!("\"s{}\"", rng.below(1000))),
                5 => {
                    out.push('[');
                    let n = rng.below(4);
                    for i in 0..n {
                        if i > 0 {
                            out.push(',');
                        }
                        build(rng, depth + 1, out);
                    }
                    out.push(']');
                }
                _ => {
                    out.push('{');
                    let n = rng.below(4);
                    for i in 0..n {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("\"k{}\":", rng.below(1000)));
                        build(rng, depth + 1, out);
                    }
                    out.push('}');
                }
            }
        }

        const MUTATIONS: &[char] = &[
            '{', '}', '[', ']', '"', ':', ',', '0', '1', 'e', '.', '-', 'n', 't', ' ', '\\',
        ];

        let mut rng = Rng(0x5eed_a1c0_5732);
        for _ in 0..4000 {
            let mut document = String::new();
            build(&mut rng, 0, &mut document);

            // Half the documents are mutated, so the corpus covers both
            // well-formed shapes and the malformed neighbourhood around them.
            if rng.below(2) == 0 && !document.is_empty() {
                let at = (rng.below(document.len() as u64)) as usize;
                if document.is_char_boundary(at) {
                    let pick = rng.below(MUTATIONS.len() as u64) as usize;
                    let injected = MUTATIONS.get(pick).copied().unwrap_or('!');
                    match rng.below(3) {
                        0 => document.insert(at, injected),
                        1 => {
                            document.remove(at);
                        }
                        _ => document.insert(at, injected),
                    }
                }
            }

            agree(&document);
        }
    }

    /// FD-1.4 bounds "any single string field" at 65536 **bytes**, and the
    /// unit is the decoded UTF-8 value — which is what the walk this replaced
    /// measured. The first version of this scanner measured the *spelling*
    /// between the quotes instead: identical for unescaped ASCII, badly
    /// divergent otherwise, so a valid document was refused.
    ///
    /// The witnesses come in pairs at the edge, in both spellings, because a
    /// decoder that handles `\u0061` but not surrogate pairs is almost a
    /// Unicode decoder — it just omits the part of Unicode that is difficult.
    #[test]
    fn a_string_is_bounded_by_its_decoded_length_not_its_spelling() {
        // Each `\u0061` spells six bytes and decodes to one.
        let ascii_escape = |count: usize| format!(r#"{{"x":"{}"}}"#, r"\u0061".repeat(count));
        // Each pair spells twelve bytes and decodes to four: one emoji.
        let pair = |count: usize| format!(r#"{{"x":"{}"}}"#, r"\uD83D\uDE00".repeat(count));

        for (label, document, admissible) in [
            (
                "ascii escapes at the bound",
                ascii_escape(MAX_STRING_BYTES),
                true,
            ),
            (
                "ascii escapes one over",
                ascii_escape(MAX_STRING_BYTES + 1),
                false,
            ),
            (
                "surrogate pairs at the bound",
                pair(MAX_STRING_BYTES / 4),
                true,
            ),
            (
                "surrogate pairs one over",
                pair(MAX_STRING_BYTES / 4 + 1),
                false,
            ),
        ] {
            let scanned = check(&document);
            assert_eq!(
                scanned.is_ok(),
                admissible,
                "{label}: scanner said {scanned:?}"
            );
            if !admissible {
                assert!(
                    matches!(scanned, Err(ParseError::StringTooLong { .. })),
                    "{label}: must be refused by the string bound, got {scanned:?}"
                );
            }
            // And the boundary this slice must not have moved: whatever the
            // spelling, the walk it replaced reaches the same verdict.
            agree(&document);
        }

        // The reported witness, kept as itself: 11 000 decoded bytes spelled as
        // 66 000, comfortably inside the bound and previously refused.
        agree(&ascii_escape(11_000));
    }

    /// The lone-surrogate split, pinned across the two layers that share it.
    ///
    /// [`crate::scan`] deliberately does not adjudicate Unicode: it counts an
    /// unpaired surrogate as three bytes and moves on, because serde refuses
    /// the document immediately afterwards. That is a *cross-layer agreement* —
    /// "we skip this here because the next layer is guaranteed to catch it" —
    /// and #124 spent eight rounds establishing that such agreements belong in
    /// a test rather than in a comment. A later change to either layer that
    /// broke the pairing would otherwise turn a documented division of labour
    /// into a hole neither side is watching.
    ///
    /// So both halves are asserted: the scan traverses these without claiming
    /// authority, and the pipeline refuses them.
    #[test]
    fn a_lone_surrogate_is_refused_by_the_pipeline_though_not_by_the_scan() {
        for (label, document) in [
            ("lone high", r#"{"x":"\uD83D"}"#),
            ("lone low", r#"{"x":"\uDE00"}"#),
            ("high then a plain escape", r#"{"x":"\uD83D\n"}"#),
            (
                "high then a non-surrogate escape",
                r#"{"x":"\uD83D\u0061"}"#,
            ),
            ("reversed pair", r#"{"x":"\uDE00\uD83D"}"#),
        ] {
            assert!(
                check(document).is_ok(),
                "{label}: the scan traverses it without claiming Unicode authority"
            );
            assert!(
                pipeline(document).is_err(),
                "{label}: the pipeline must refuse it — the scan is relying on that"
            );
            // And the verdict is the one the walk this replaced reached.
            agree(document);
        }

        // A well-formed pair is admitted by both, so the refusals above are the
        // pairing rule and not a layer that refuses every surrogate.
        let paired = r#"{"x":"\uD83D\uDE00"}"#;
        assert!(check(paired).is_ok());
        assert!(pipeline(paired).is_ok());
        agree(paired);
    }

    /// The boundary family, enumerated rather than sampled.
    ///
    /// Random generation is the wrong instrument for this defect and would have
    /// gone on missing it: finding it by chance needs an escape-heavy spelling
    /// *and* a decoded length within a byte or two of 65536 at the same time.
    /// Four spellings times three positions at the edge is twelve documents and
    /// finds it every run.
    ///
    /// Each family carries a different ratio of spelled to decoded bytes — 1:1,
    /// 2:1, 6:1 and 3:1 — so any implementation measuring the wrong unit fails
    /// on at least one of them regardless of which wrong unit it picked.
    #[test]
    fn the_string_bound_is_exact_in_every_spelling() {
        /// A JSON string spelled in `family` whose decoded length is exactly
        /// `target` bytes, padded with literal ASCII when the unit does not
        /// divide.
        fn spelled(family: &str, target: usize) -> String {
            let (unit, decoded_per_unit) = match family {
                "literal" => ("a", 1),
                "simple escape" => (r"\n", 1),
                "ascii \\u escape" => (r"\u0061", 1),
                _ => (r"\uD83D\uDE00", 4),
            };
            let units = target / decoded_per_unit;
            let padding = target - units * decoded_per_unit;
            format!(r#"{{"x":"{}{}"}}"#, unit.repeat(units), "a".repeat(padding))
        }

        for family in [
            "literal",
            "simple escape",
            "ascii \\u escape",
            "surrogate pair",
        ] {
            for (offset, admissible) in [(-1_i64, true), (0, true), (1, false)] {
                let target = (MAX_STRING_BYTES as i64 + offset) as usize;
                let document = spelled(family, target);
                let scanned = check(&document);
                assert_eq!(
                    scanned.is_ok(),
                    admissible,
                    "{family} at {target} decoded bytes: {scanned:?}"
                );
                // The walk this replaced is the authority on where the edge is.
                agree(&document);
            }
        }
    }

    /// FD-1.4 bounds nesting depth, and the walk this replaced counted it per
    /// **value**: the root at depth 1, every child one deeper, scalars
    /// included.
    ///
    /// The first streaming version counted container *frames* instead, which
    /// admitted a scalar leaf one level past the bound. The differential missed
    /// it because the corpus tested `MAX + 1` containers — refused either way —
    /// and never the exact boundary. Both reviewers found it independently,
    /// which is the more useful fact about it: an off-by-one at a bound is
    /// invisible to a corpus that only samples well past the bound.
    #[test]
    fn the_depth_bound_counts_values_as_the_walk_did() {
        let nested = |containers: usize| {
            format!(
                "{}1{}",
                r#"{"a":"#.repeat(containers),
                "}".repeat(containers)
            )
        };
        // 31 containers put the scalar at depth 32: admissible.
        assert!(check(&nested(MAX_JSON_DEPTH - 1)).is_ok());
        // 32 put it at depth 33: refused, because the leaf counts.
        assert!(matches!(
            check(&nested(MAX_JSON_DEPTH)),
            Err(ParseError::DepthExceeded { .. })
        ));
        assert!(matches!(
            check(&nested(MAX_JSON_DEPTH + 1)),
            Err(ParseError::DepthExceeded { .. })
        ));
        // And the walk agrees at each of the three, which is the property that
        // was actually violated.
        for containers in [MAX_JSON_DEPTH - 1, MAX_JSON_DEPTH, MAX_JSON_DEPTH + 1] {
            agree(&nested(containers));
        }
    }

    /// Duplicate member names are refused, and that **is** a change to the
    /// admission set — declared here rather than arriving as a side effect.
    ///
    /// The walk this replaced admitted them. `validate_document` materialised a
    /// `serde_json::Value` first, whose object map silently keeps the last
    /// duplicate, so `{"known":null,"known":1}` reached the typed schema as
    /// `{"known":1}` and was admitted with the null never seen.
    ///
    /// Three reasons the stricter behaviour is the correct one, rather than a
    /// convenience of the new parser:
    ///
    /// - **FD-1.3** rejects an explicit `null` *everywhere*. The bytes contain
    ///   one. It went unseen only because a JSON library discarded it before
    ///   the rule ran; applying the rule to the document rather than to a
    ///   library's reduction of it is the rule as written.
    /// - **FD-1.2** computes envelope identity by framing *fields*, and says
    ///   that removes the whole class of "semantically identical,
    ///   digest-unequal" failures. RFC 8259 leaves duplicate-name handling
    ///   unspecified, so first-wins and last-wins are both conforming — and two
    ///   implementations would frame different fields from identical bytes.
    ///   Admitting duplicates reopens exactly the class FD-1.2 closes.
    /// - **FD-1.6** fails closed on anything ambiguous by design.
    ///
    /// The refusal comes from two layers depending on the spelling: the scan
    /// refuses a shadowed null because the null is genuinely present, and the
    /// typed schema refuses a plain duplicate as a duplicate field. Both are
    /// refusals of the artifact, which is what matters. Detecting duplicates in
    /// the scan itself would need per-object name tracking, whose cost scales
    /// with member count rather than depth — trading the O(depth) property for
    /// a tidier error is a bad exchange.
    #[test]
    fn duplicate_member_names_are_refused_which_the_walk_did_not_do() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Dup {
            #[allow(dead_code)]
            known: u32,
        }
        impl FromDocument for Dup {
            fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
                serde_json::from_slice(bytes).map_err(|e| ParseError::SchemaMismatch {
                    category: classify(&e),
                })
            }
        }
        impl WireArtifact for Dup {
            const MAX_BYTES: u64 = MAX_CONTROL_ARTIFACT_BYTES;

            fn validate_wire(&self) -> Result<(), String> {
                Ok(())
            }
        }

        // A shadowed null: refused by the scan, because FD-1.3 is about the
        // document and the document contains a null.
        assert!(matches!(
            check(r#"{"known":null,"known":1}"#),
            Err(ParseError::ExplicitNull { .. })
        ));
        // A plain duplicate: the scan has no opinion, the typed schema refuses.
        assert!(check(r#"{"known":2,"known":1}"#).is_ok());
        // Either way the artifact is refused, which is the level that matters.
        for document in [r#"{"known":null,"known":1}"#, r#"{"known":2,"known":1}"#] {
            assert!(
                parse_artifact::<Dup>(document.as_bytes()).is_err(),
                "a duplicate member must not yield an artifact: {document}"
            );
        }
        // The single-member document is admitted, so the refusals above are the
        // duplication and not the schema refusing everything.
        assert!(parse_artifact::<Dup>(br#"{"known":1}"#).is_ok());
    }

    /// FD-1.4 asks for a parse-time rejection.    /// FD-1.4 asks for a parse-time rejection. The witness that this one *is*
    /// one: everything after the offending point is garbage, so a scanner that
    /// reached it would report a syntax error instead.
    ///
    /// Under the walk this replaced, each of these was fully materialised
    /// before its bound was consulted — which is what made the 64 MiB ceiling
    /// unimplementable rather than merely slow.
    #[test]
    fn a_structural_bound_fires_before_the_rest_of_the_document_is_read() {
        let garbage = "!!! not json at all !!!";

        let wide = format!(
            r#"{{"a":[{},{garbage}"#,
            vec!["1"; MAX_ARRAY_LEN + 1].join(",")
        );
        assert!(
            matches!(check(&wide), Err(ParseError::ArrayTooLong { .. })),
            "the array cap must fire before the tail is scanned"
        );

        let deep = format!("{}{garbage}", "{\"a\":[".repeat(MAX_JSON_DEPTH + 2));
        assert!(
            matches!(check(&deep), Err(ParseError::DepthExceeded { .. })),
            "the depth cap must fire before the tail is scanned"
        );

        let long = format!(
            r#"{{"a":"{}","b":{garbage}"#,
            "x".repeat(MAX_STRING_BYTES + 1)
        );
        assert!(
            matches!(check(&long), Err(ParseError::StringTooLong { .. })),
            "the string cap must fire before the tail is scanned"
        );

        let nulled = format!(r#"{{"a":null,"b":{garbage}"#);
        assert!(
            matches!(check(&nulled), Err(ParseError::ExplicitNull { .. })),
            "the null policy must fire before the tail is scanned"
        );
    }

    /// The other half of 2A being real: a type at the FD-1.4 evidence ceiling
    /// now compiles, where `CEILING_IS_PARSEABLE` used to refuse it.
    ///
    /// This is the property the gate transformation bought, asserted rather
    /// than asserted-about. `InteractionManifestV1` is the first real type that
    /// needs it and belongs to the next slice; what is proved here is that the
    /// door it was waiting on is open.
    #[test]
    fn an_artifact_at_the_evidence_ceiling_can_now_be_admitted() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Huge {
            #[allow(dead_code)]
            known: u32,
        }
        impl FromDocument for Huge {
            fn from_bytes(bytes: &[u8]) -> Result<Self, ParseError> {
                serde_json::from_slice(bytes).map_err(|e| ParseError::SchemaMismatch {
                    category: classify(&e),
                })
            }
        }
        impl WireArtifact for Huge {
            const MAX_BYTES: u64 = crate::bounds::MAX_EVIDENCE_BLOB_BYTES;

            fn validate_wire(&self) -> Result<(), String> {
                Ok(())
            }
        }

        assert!(parse_artifact::<Huge>(br#"{"known":1}"#).is_ok());

        // And a hostile document at that ceiling is still refused structurally,
        // rather than the larger allowance becoming a larger hole: the array
        // cap does not scale with the byte ceiling.
        let hostile = format!(
            r#"{{"known":[{}]}}"#,
            vec!["1"; MAX_ARRAY_LEN + 1].join(",")
        );
        assert!(matches!(
            parse_artifact::<Huge>(hostile.as_bytes()),
            Err(ParseError::ArrayTooLong { .. })
        ));
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
