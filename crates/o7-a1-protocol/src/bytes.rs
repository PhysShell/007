//! `ByteStringV1` — the ONE frozen representation for OS-byte values
//! (contract §4.5): base64url WITHOUT padding. A0 deliberately preserves
//! non-UTF-8 paths and patch bytes; A1 must not undo that with a
//! convenient `String`. Implemented locally (~40 lines) rather than
//! adding an external dependency the supply-chain gate would have to
//! adjudicate.

use serde::{Deserialize, Serialize};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode raw bytes as base64url without padding.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let idx = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        let take = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for i in idx.iter().take(take) {
            // idx entries are masked to 6 bits, always in-alphabet.
            out.push(char::from(
                ALPHABET.get(*i as usize).copied().unwrap_or(b'A'),
            ));
        }
    }
    out
}

/// Why a `ByteStringV1` failed to decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ByteStringError {
    /// A character outside the base64url alphabet, or padding (`=`),
    /// which the frozen representation forbids.
    #[error("byte string contains a character outside unpadded base64url")]
    BadChar,
    /// A length that no unpadded base64url encoding produces.
    #[error("byte string has an impossible unpadded base64url length")]
    BadLength,
}

fn decode_char(c: u8) -> Result<u32, ByteStringError> {
    match c {
        b'A'..=b'Z' => Ok(u32::from(c - b'A')),
        b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err(ByteStringError::BadChar),
    }
}

/// Decode unpadded base64url into raw bytes.
///
/// # Errors
/// [`ByteStringError`] on a foreign character (including `=` padding) or
/// an impossible length.
pub fn decode(s: &str) -> Result<Vec<u8>, ByteStringError> {
    let b = s.as_bytes();
    if b.len() % 4 == 1 {
        return Err(ByteStringError::BadLength);
    }
    let mut out = Vec::with_capacity(b.len() / 4 * 3 + 2);
    for chunk in b.chunks(4) {
        let mut n: u32 = 0;
        for c in chunk {
            n = (n << 6) | decode_char(*c)?;
        }
        match chunk.len() {
            4 => {
                out.push(((n >> 16) & 0xff) as u8);
                out.push(((n >> 8) & 0xff) as u8);
                out.push((n & 0xff) as u8);
            }
            3 => {
                n <<= 6;
                out.push(((n >> 16) & 0xff) as u8);
                out.push(((n >> 8) & 0xff) as u8);
            }
            2 => {
                n <<= 12;
                out.push(((n >> 16) & 0xff) as u8);
            }
            _ => return Err(ByteStringError::BadLength),
        }
    }
    Ok(out)
}

/// A byte value carried in canonical JSON via the frozen unpadded
/// base64url form. Repository paths (`RepoPathBytes`) and every other
/// OS-byte value use this — never a lossy `String`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ByteStringV1(Vec<u8>);

impl ByteStringV1 {
    /// Wrap raw bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<String> for ByteStringV1 {
    type Error = ByteStringError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Ok(Self(decode(&s)?))
    }
}

impl From<ByteStringV1> for String {
    fn from(b: ByteStringV1) -> Self {
        encode(&b.0)
    }
}

/// A repository path as exact bytes (contract §4.5/§8.1).
pub type RepoPathBytes = ByteStringV1;

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn round_trips_including_non_utf8() {
        for case in [
            &b""[..],
            &b"a"[..],
            &b"ab"[..],
            &b"abc"[..],
            &[0xff, 0x00, 0x80][..],
        ] {
            let enc = encode(case);
            assert_eq!(decode(&enc).unwrap(), case, "case {case:?} via {enc}");
        }
    }

    #[test]
    fn known_answers_are_unpadded_url_safe() {
        // RFC 4648 vector "foobar" = "Zm9vYmFy"; url-safe, no padding.
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"fo"), "Zm8");
        assert_eq!(encode(&[0xfb, 0xef]), "--8");
    }

    #[test]
    fn padding_and_foreign_chars_fail_closed() {
        assert_eq!(decode("Zm8="), Err(ByteStringError::BadChar));
        assert_eq!(decode("Zm+8"), Err(ByteStringError::BadChar));
        assert_eq!(decode("A"), Err(ByteStringError::BadLength));
    }
}
