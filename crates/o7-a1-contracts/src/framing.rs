//! FD-1.2 field framing — the only way an A1 identity is computed.
//!
//! Digests are computed by explicit field framing (length-prefixed), never by
//! hashing a serialized JSON blob, so they are byte-stable regardless of map
//! ordering or serializer whitespace. This follows the precedent frozen in
//! `crates/o7-run/src/event.rs`, and `frame` is byte-identical to the helper
//! there: a `u64`-le length prefix followed by the bytes.
//!
//! # Why a preimage buffer rather than an incremental hasher
//!
//! The contract writes framings as a sequence of `h.update(...)` calls. Hashing
//! is over the concatenation of those updates, so accumulating the framed bytes
//! and hashing once is byte-identical — and it lets every digest in this crate
//! be produced by [`o7_run::event::Digest256::of_bytes`], which FD-1.1 names as
//! *the* form for A1 digests. No second constructor, and no path by which this
//! crate could mint a `Digest256` that did not come from real bytes.
//!
//! Framed preimages are small: an envelope's is a few hundred bytes plus its
//! refs, bounded by FD-1.4 at 256 refs.

use crate::scalars::Digest256;

/// A framed digest preimage under one domain separator.
#[derive(Debug, Clone)]
pub struct Preimage {
    buf: Vec<u8>,
}

impl Preimage {
    /// Start a preimage with its domain separator, e.g. `b"o7-a1-envelope\0v1\0"`.
    ///
    /// The separator is written raw, exactly as the contract spells it — it is
    /// not itself framed.
    #[must_use]
    pub fn new(domain: &[u8]) -> Self {
        Self {
            buf: domain.to_vec(),
        }
    }

    /// `frame(x)` — a `u64`-le length prefix followed by the bytes.
    pub fn frame(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf
            .extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.buf.extend_from_slice(bytes);
        self
    }

    /// Frame a string field.
    pub fn frame_str(&mut self, s: &str) -> &mut Self {
        self.frame(s.as_bytes())
    }

    /// Frame an optional field.
    ///
    /// FD-1.2: "Absent optional fields are framed as the empty string; they are
    /// never skipped, so 'absent' and 'empty' hash distinctly from 'different
    /// value', and the field order is fixed forever."
    ///
    /// Note that this makes absent and present-but-empty hash *identically*,
    /// which is exactly why FD-1.3 refuses explicit `null` and why no A1 field
    /// is allowed to mean something different when empty than when absent.
    pub fn frame_optional_str(&mut self, s: Option<&str>) -> &mut Self {
        self.frame(s.unwrap_or("").as_bytes())
    }

    /// Frame a `u32` in little-endian form, as every `…​.to_le_bytes()` in the
    /// contract's framings does.
    pub fn frame_u32(&mut self, v: u32) -> &mut Self {
        self.frame(&v.to_le_bytes())
    }

    /// Frame a `u64` in little-endian form.
    pub fn frame_u64(&mut self, v: u64) -> &mut Self {
        self.frame(&v.to_le_bytes())
    }

    /// The framed bytes, for tests and for callers that need to prove what was
    /// hashed rather than only the result.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Hash the accumulated preimage.
    #[must_use]
    pub fn digest(&self) -> Digest256 {
        Digest256::of_bytes(&self.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_is_length_prefixed_little_endian() {
        let mut p = Preimage::new(b"d\0");
        p.frame(b"ab");
        let mut expected = b"d\0".to_vec();
        expected.extend_from_slice(&2u64.to_le_bytes());
        expected.extend_from_slice(b"ab");
        assert_eq!(p.as_bytes(), expected.as_slice());
    }

    // The property length-prefixing exists for: two different splits of the
    // same concatenated bytes must not collide.
    #[test]
    fn framing_separates_fields_that_concatenation_would_merge() {
        let mut a = Preimage::new(b"d\0");
        a.frame_str("ab").frame_str("c");
        let mut b = Preimage::new(b"d\0");
        b.frame_str("a").frame_str("bc");
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn an_absent_optional_frames_as_the_empty_string() {
        let mut absent = Preimage::new(b"d\0");
        absent.frame_optional_str(None);
        let mut empty = Preimage::new(b"d\0");
        empty.frame_optional_str(Some(""));
        assert_eq!(absent.digest(), empty.digest());

        let mut present = Preimage::new(b"d\0");
        present.frame_optional_str(Some("x"));
        assert_ne!(absent.digest(), present.digest());
    }

    // An optional field must never be skipped: skipping would let a later field
    // slide into its position and two different envelopes hash alike.
    #[test]
    fn an_absent_optional_still_occupies_its_position() {
        let mut skipped = Preimage::new(b"d\0");
        skipped.frame_str("tail");
        let mut framed = Preimage::new(b"d\0");
        framed.frame_optional_str(None).frame_str("tail");
        assert_ne!(skipped.digest(), framed.digest());
    }

    #[test]
    fn the_domain_separator_is_part_of_the_preimage() {
        let mut one = Preimage::new(b"o7-a1-envelope\0v1\0");
        one.frame_str("x");
        let mut two = Preimage::new(b"o7-a1-budget-policy\0v1\0");
        two.frame_str("x");
        assert_ne!(one.digest(), two.digest());
    }
}
