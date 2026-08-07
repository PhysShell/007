//! Domain-separated protocol digests (contract §4.1–§4.2): computed over
//! explicitly framed, typed fields — never a hash of an arbitrary JSON
//! serialization. The context registry is compile-time constants only,
//! one purpose per type, uniqueness-tested, with a known-answer test per
//! context. `BlobDigest` (content addressing) is deliberately NOT here.

use crate::ids::BlobDigest;

/// A frozen digest context. Constants only — never assembled from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigestContext(&'static str);

/// `message_binding_digest` framing (contract §4.3).
pub const CTX_MESSAGE_BINDING: DigestContext = DigestContext("o7-a1\0message-binding\0v1\0");
/// Contract digests.
pub const CTX_CONTRACT: DigestContext = DigestContext("o7-a1\0contract\0v1\0");
/// Gate-registry snapshot digests (contract §10).
pub const CTX_GATE_REGISTRY: DigestContext = DigestContext("o7-a1\0gate-registry\0v1\0");
/// Verifier policy digests (contract §10).
pub const CTX_VERIFIER_POLICY: DigestContext = DigestContext("o7-a1\0verifier-policy\0v1\0");
/// Logical model route digests (contract §9).
pub const CTX_MODEL_ROUTE: DigestContext = DigestContext("o7-a1\0model-route\0v1\0");

/// Every frozen context, for the registry tests. Extending this list is
/// a contract change via the supersede path.
pub const ALL_CONTEXTS: &[DigestContext] = &[
    CTX_MESSAGE_BINDING,
    CTX_CONTRACT,
    CTX_GATE_REGISTRY,
    CTX_VERIFIER_POLICY,
    CTX_MODEL_ROUTE,
];

/// Length-prefixed framed digest builder — the same explicit framing
/// discipline as `LaunchRequest::spec_digest` (`o7-launch-spec\0v1\0`):
/// every field is length-prefixed, sets are sorted by the caller before
/// framing, and the context string opens the frame.
#[derive(Debug)]
pub struct FramedDigest {
    buf: Vec<u8>,
}

impl FramedDigest {
    /// Open a frame under `ctx`.
    #[must_use]
    pub fn new(ctx: DigestContext) -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(ctx.0.as_bytes());
        Self { buf }
    }

    /// Append one length-prefixed byte field.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf
            .extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        self.buf.extend_from_slice(bytes);
        self
    }

    /// Append one length-prefixed UTF-8 field.
    pub fn push_str(&mut self, s: &str) -> &mut Self {
        self.push_bytes(s.as_bytes())
    }

    /// Append a collection count (frame shape marker before repeated
    /// fields, exactly like `spec_digest`'s `push_len`).
    pub fn push_count(&mut self, n: u64) -> &mut Self {
        self.buf.extend_from_slice(&n.to_be_bytes());
        self
    }

    /// Close the frame into a digest.
    #[must_use]
    pub fn finish(self) -> BlobDigest {
        BlobDigest::of_bytes(&self.buf)
    }
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registry_contexts_are_unique_and_well_formed() {
        let set: BTreeSet<&str> = ALL_CONTEXTS.iter().map(|c| c.0).collect();
        assert_eq!(set.len(), ALL_CONTEXTS.len(), "duplicate digest context");
        for c in ALL_CONTEXTS {
            assert!(c.0.starts_with("o7-a1\0"), "bad prefix: {:?}", c.0);
            assert!(c.0.ends_with("\0v1\0"), "bad version suffix: {:?}", c.0);
        }
    }

    /// Known-answer per context: empty frame digests are pinned. Any
    /// change here is a wire change (supersede path).
    #[test]
    fn known_answers_per_context() {
        let pinned = [
            (
                CTX_MESSAGE_BINDING,
                BlobDigest::of_bytes(b"o7-a1\0message-binding\0v1\0"),
            ),
            (CTX_CONTRACT, BlobDigest::of_bytes(b"o7-a1\0contract\0v1\0")),
            (
                CTX_GATE_REGISTRY,
                BlobDigest::of_bytes(b"o7-a1\0gate-registry\0v1\0"),
            ),
            (
                CTX_VERIFIER_POLICY,
                BlobDigest::of_bytes(b"o7-a1\0verifier-policy\0v1\0"),
            ),
            (
                CTX_MODEL_ROUTE,
                BlobDigest::of_bytes(b"o7-a1\0model-route\0v1\0"),
            ),
        ];
        for (ctx, want) in pinned {
            assert_eq!(FramedDigest::new(ctx).finish(), want);
        }
    }

    #[test]
    fn framing_is_length_prefixed_not_concatenation() {
        // ("ab","c") must differ from ("a","bc") — the classic ambiguity.
        let mut d1 = FramedDigest::new(CTX_CONTRACT);
        d1.push_str("ab").push_str("c");
        let mut d2 = FramedDigest::new(CTX_CONTRACT);
        d2.push_str("a").push_str("bc");
        assert_ne!(d1.finish(), d2.finish());
    }
}
