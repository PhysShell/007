//! `CanonicalJsonV1` — the ONE writable representation of canonical A1
//! bytes (contract §4.5, review P1-19): object keys sorted by byte
//! order; no insignificant whitespace; a fixed minimal escaping table;
//! integers only (floats are unrepresentable in the value model); byte
//! values via `ByteStringV1` at the schema layer. The writer carries
//! [`WRITER_VERSION`] and a known-answer corpus; any byte change for any
//! corpus entry is a new writer version, and recovery serializes under
//! the version recorded in the reservation — never the current default.

use std::collections::BTreeMap;

/// The frozen writer version this module implements.
pub const WRITER_VERSION: u32 = 1;

/// The restricted canonical value model: no floats, by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    /// JSON null.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// Unsigned integer (covers `accepted_at` nanoseconds).
    UInt(u64),
    /// Signed integer.
    Int(i64),
    /// UTF-8 string (byte values arrive here already `ByteStringV1`-encoded).
    Str(String),
    /// Array.
    Arr(Vec<CanonicalValue>),
    /// Object — a `BTreeMap`, so key order IS byte order and a duplicate
    /// key is unrepresentable at this layer.
    Obj(BTreeMap<String, CanonicalValue>),
}

fn push_escaped(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    for c in s.chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{0008}' => out.extend_from_slice(b"\\b"),
            '\u{000C}' => out.extend_from_slice(b"\\f"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            c if (c as u32) < 0x20 => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let n = c as u32;
                let hi = HEX.get(((n >> 4) & 0xf) as usize).copied().unwrap_or(b'0');
                let lo = HEX.get((n & 0xf) as usize).copied().unwrap_or(b'0');
                out.extend_from_slice(b"\\u00");
                out.push(hi);
                out.push(lo);
            }
            c => {
                let mut b = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// Serialize a canonical value to its single frozen byte form.
#[must_use]
pub fn write_canonical(v: &CanonicalValue) -> Vec<u8> {
    let mut out = Vec::new();
    write_into(&mut out, v);
    out
}

fn write_into(out: &mut Vec<u8>, v: &CanonicalValue) {
    match v {
        CanonicalValue::Null => out.extend_from_slice(b"null"),
        CanonicalValue::Bool(true) => out.extend_from_slice(b"true"),
        CanonicalValue::Bool(false) => out.extend_from_slice(b"false"),
        CanonicalValue::UInt(n) => out.extend_from_slice(n.to_string().as_bytes()),
        CanonicalValue::Int(n) => out.extend_from_slice(n.to_string().as_bytes()),
        CanonicalValue::Str(s) => push_escaped(out, s),
        CanonicalValue::Arr(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_into(out, item);
            }
            out.push(b']');
        }
        CanonicalValue::Obj(map) => {
            out.push(b'{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                push_escaped(out, k);
                out.push(b':');
                write_into(out, val);
            }
            out.push(b'}');
        }
    }
}

/// `recorded` — controller-assigned acceptance metadata with a frozen
/// wire shape (contract §4.5): `accepted_at` is an unsigned integer
/// count of nanoseconds since the Unix epoch, UTC; `writer_version`
/// names the exact canonical writer that produced the stored bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedMetadataV1 {
    /// Nanoseconds since the Unix epoch, UTC.
    pub accepted_at_unix_ns: u64,
    /// The canonical writer version ([`WRITER_VERSION`] at write time).
    pub writer_version: u32,
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn obj(pairs: &[(&str, CanonicalValue)]) -> CanonicalValue {
        CanonicalValue::Obj(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
    }

    /// The known-answer corpus (contract §4.5): input model → exact
    /// bytes. Changing ANY expected byte requires a new WRITER_VERSION.
    #[test]
    fn known_answer_corpus_v1() {
        let cases: Vec<(CanonicalValue, &[u8])> = vec![
            (CanonicalValue::Null, b"null"),
            (CanonicalValue::UInt(0), b"0"),
            (CanonicalValue::Int(-7), b"-7"),
            (
                CanonicalValue::Str("a\"b\\c\nd\u{1}".into()),
                b"\"a\\\"b\\\\c\\nd\\u0001\"",
            ),
            (
                obj(&[
                    ("b", CanonicalValue::UInt(2)),
                    ("a", CanonicalValue::UInt(1)),
                ]),
                b"{\"a\":1,\"b\":2}",
            ),
            (
                CanonicalValue::Arr(vec![CanonicalValue::Bool(true), CanonicalValue::Null]),
                b"[true,null]",
            ),
            // Non-ASCII stays raw UTF-8, never \u-escaped above 0x1f.
            (CanonicalValue::Str("π".into()), "\"π\"".as_bytes()),
        ];
        for (v, want) in cases {
            assert_eq!(write_canonical(&v), want, "value {v:?}");
        }
        assert_eq!(WRITER_VERSION, 1);
    }

    #[test]
    fn keys_sort_by_byte_order_and_bytes_are_whitespace_free() {
        let v = obj(&[
            ("z", CanonicalValue::Null),
            ("A", CanonicalValue::Null),
            ("a", CanonicalValue::Null),
        ]);
        let bytes = write_canonical(&v);
        assert_eq!(bytes, b"{\"A\":null,\"a\":null,\"z\":null}");
        assert!(!bytes.iter().any(|b| *b == b' ' || *b == b'\n'));
    }

    #[test]
    fn same_model_same_bytes_always() {
        let v = obj(&[("k", CanonicalValue::Arr(vec![CanonicalValue::UInt(1)]))]);
        assert_eq!(write_canonical(&v), write_canonical(&v.clone()));
    }
}
