//! Domain-separated protocol digests (contract §4.1–§4.2): computed over
//! explicitly framed, typed fields — never a hash of an arbitrary JSON
//! serialization, and NEVER conflated with `BlobDigest` content identity
//! (review T2: the two digest families are distinct at the Rust type
//! level; a shared 32-byte primitive is private, the public semantic
//! surface is per-purpose newtypes). Each purpose owns exactly one
//! context constant — pairing enforced by construction, so one purpose
//! can never be reused for two types. Context known answers are pinned
//! as LITERALS (a typo in a context constant fails the test instead of
//! moving both sides of it).

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ids::IdError;

fn hex64(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn validate_hex64(raw: &str) -> Result<(), IdError> {
    if raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(IdError::BadDigest)
    }
}

/// Length-prefixed field framer (the `spec_digest` discipline): every
/// field is length-prefixed; sets are sorted by the caller before
/// framing. Only reachable through a purpose type's `compute`, so a
/// frame always opens with that purpose's own context.
#[derive(Debug)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    fn new(ctx: &'static str) -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(ctx.as_bytes());
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

    /// Append a collection count before repeated fields.
    pub fn push_count(&mut self, n: u64) -> &mut Self {
        self.buf.extend_from_slice(&n.to_be_bytes());
        self
    }
}

macro_rules! protocol_digest {
    ($(#[$doc:meta])* $name:ident, $ctx:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// This purpose's frozen domain-separation context.
            pub const CONTEXT: &'static str = $ctx;

            /// Frame typed fields under this purpose's own context and
            /// close the digest. The ONLY way to mint a value of this
            /// type from data.
            #[must_use]
            pub fn compute(fill: impl FnOnce(&mut Framer)) -> Self {
                let mut f = Framer::new(Self::CONTEXT);
                fill(&mut f);
                Self(hex64(&f.buf))
            }

            /// Re-admit a stored hex form (validated shape only — the
            /// semantic guarantee is that it was minted by `compute`).
            ///
            /// # Errors
            /// [`IdError::BadDigest`] unless 64 lowercase hex chars.
            pub fn parse(raw: impl Into<String>) -> Result<Self, IdError> {
                let raw = raw.into();
                validate_hex64(&raw)?;
                Ok(Self(raw))
            }

            /// The hex form.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(raw: String) -> Result<Self, Self::Error> {
                Self::parse(raw)
            }
        }

        impl From<$name> for String {
            fn from(d: $name) -> Self {
                d.0
            }
        }
    };
}

protocol_digest!(
    /// `message_binding_digest` (contract §4.3).
    MessageBindingDigest,
    "o7-a1\0message-binding\0v1\0"
);
protocol_digest!(
    /// Contract identity digests.
    ContractDigest,
    "o7-a1\0contract\0v1\0"
);
protocol_digest!(
    /// Gate-registry snapshot digests (contract §10).
    RegistryDigest,
    "o7-a1\0gate-registry\0v1\0"
);
protocol_digest!(
    /// Verifier/controller policy digests (contract §10, §3).
    PolicyDigest,
    "o7-a1\0verifier-policy\0v1\0"
);
protocol_digest!(
    /// Logical model route digests (contract §9).
    ModelRouteDigest,
    "o7-a1\0model-route\0v1\0"
);

/// Every frozen context, for the registry tests. Extension is a
/// supersede-path contract change.
pub const ALL_CONTEXTS: &[&str] = &[
    MessageBindingDigest::CONTEXT,
    ContractDigest::CONTEXT,
    RegistryDigest::CONTEXT,
    PolicyDigest::CONTEXT,
    ModelRouteDigest::CONTEXT,
];

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registry_contexts_are_unique_and_well_formed() {
        let set: BTreeSet<&str> = ALL_CONTEXTS.iter().copied().collect();
        assert_eq!(set.len(), ALL_CONTEXTS.len(), "duplicate digest context");
        for c in ALL_CONTEXTS {
            assert!(c.starts_with("o7-a1\0"), "bad prefix: {c:?}");
            assert!(c.ends_with("\0v1\0"), "bad version suffix: {c:?}");
        }
    }

    /// Contract §4.2 known answers, pinned as LITERALS (review T2): the
    /// empty-frame digest of each context. Independently recomputed
    /// outside this codebase before pinning. A typo in a context
    /// constant now fails here instead of moving both sides of the
    /// comparison.
    #[test]
    fn known_answers_per_context_are_pinned_literals() {
        assert_eq!(
            MessageBindingDigest::compute(|_| {}).as_str(),
            "a1b002012dea24011e655e8990a3832cc69a17b2bc312476ee46380eff31a068"
        );
        assert_eq!(
            ContractDigest::compute(|_| {}).as_str(),
            "e955c44932833ab2e6cdba97e9c9ad4b2f6ed69d013dc8aa100c86c643c34a3f"
        );
        assert_eq!(
            RegistryDigest::compute(|_| {}).as_str(),
            "824e727e3f0dcdba16d613bd538e772bec3d800d0a5d831ac37a8da7335a03bc"
        );
        assert_eq!(
            PolicyDigest::compute(|_| {}).as_str(),
            "c154f3a79e1460b892df3c291bbb6bbb4a723726b3ab24c981648378a313f468"
        );
        assert_eq!(
            ModelRouteDigest::compute(|_| {}).as_str(),
            "bc38d9785c9fb3a4a754e74c561f4a4522b827156edebcae49b6df9ecad16886"
        );
    }

    #[test]
    fn framing_is_length_prefixed_not_concatenation() {
        let d1 = ContractDigest::compute(|f| {
            f.push_str("ab").push_str("c");
        });
        let d2 = ContractDigest::compute(|f| {
            f.push_str("a").push_str("bc");
        });
        assert_ne!(d1, d2);
    }

    #[test]
    fn same_fields_different_purpose_different_digest() {
        let a = ContractDigest::compute(|f| {
            f.push_str("x");
        });
        let b = PolicyDigest::compute(|f| {
            f.push_str("x");
        });
        assert_ne!(a.as_str(), b.as_str(), "domain separation is load-bearing");
    }
}
