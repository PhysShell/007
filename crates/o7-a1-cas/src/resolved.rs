//! The proof-bearing type, and the reason it carries a lifetime.
//!
//! # `ArtifactRef` is not `ResolvedOpaque`
//!
//! An `ArtifactRef` is a **claim** by whoever wrote it (FD-2.5): these bytes
//! exist, they hash to this digest, they are this many bytes, they are this
//! kind. A `ResolvedOpaque` is the same claim after a
//! [`crate::ResolutionSession`] read the store and paid for the check. The two
//! carry identical-looking data and completely different provenance, which is
//! why they are different types and why there is deliberately **no
//! `From<ArtifactRef> for ResolvedArtifact`**.
//!
//! That conversion is the most tempting API on this step: it looks natural, the
//! compiler is content, and it is semantically an admitted envelope with its
//! producer role reassigned after admission — the defect the previous step
//! spent eight review rounds removing, in a new costume.
//!
//! # Why the lifetime, rather than a session id
//!
//! Binding by brand makes the bad program fail to compile instead of failing an
//! `if proof.session != self.id` that somebody has to remember to write. The
//! hole it closes is the one where *nothing is forged*:
//!
//! ```text
//! session A:  resolve(ref) -> ResolvedArtifact ; closure charged
//! caller:     keeps the value
//! session B:  reuse it — no resolution, no charge
//! ```
//!
//! The digest is real, the bytes are real, the resolver genuinely checked them.
//! The lie is only in the context of use: the evidence came loose from the
//! accounting authority that paid for it. So:
//!
//! > Resolution evidence must not be transferable across accounting contexts
//! > unless the transfer itself re-establishes and re-charges every invariant
//! > that matters.
//!
//! `'brand` is invariant and universally quantified by
//! [`crate::ResolutionSession::enter`], so a resolved value cannot leave the
//! session that minted it and two sessions' values are not interchangeable.
//!
//! # What may leave, and what may not
//!
//! The lifetime on the value is not the whole rule, because a projection can
//! walk around it: a method returning some brand-free `VerifiedHandle` would
//! carry the verdict out while satisfying the borrow checker. So the rule is
//! about authority, not about data:
//!
//! > The brand may disappear from data. It may not disappear from authority.
//!
//! Copying a digest, a kind, a size or even the bytes out is fine — outside the
//! session those are raw facts and inputs again, exactly what they were before
//! anything was verified. What must not leave is a value whose *meaning* is
//! still "resolved". Every accessor below is audited against that: each returns
//! a plain fact, and none returns a type that asserts anything about how it was
//! obtained.
//!
//! # The executable form of this gate is on hold
//!
//! Until this slice, the brand was pinned by a `compile_fail` doctest paired
//! with a positive one: carrying a resolved value out of
//! `ResolutionSession::enter` did not compile, and the same code inside the
//! session did. A doctest compiles as an external crate, so both went away when
//! the proof types became crate-private — the stronger guarantee (downstream
//! cannot name these types at all) removed the weaker one's test.
//!
//! That is a real loss of an executable check, recorded rather than absorbed.
//! It returns with the public surface in the typed-slot slice, where there is
//! again something outside the crate for a value to escape *to*. What still
//! holds meanwhile is the compiler: `'brand` is invariant and universally
//! quantified, so the internal code compiles only because nothing escapes.

// This module has no non-test consumer yet, and that is the shape of the
// slice rather than an oversight: the resolution entry points are
// crate-private until a §3 payload schema can declare its own slots, and
// nothing inside this crate declares one. The alternative to this attribute
// is publishing the API to satisfy a lint, which is precisely the
// caller-authored authority route this slice exists to remove — a warning
// must not get to choose the public surface. It comes off in the typed-slot
// slice, when the resolver acquires a caller.
#![allow(dead_code)]

use std::marker::PhantomData;

use o7_a1_contracts::{ArtifactKindV1, WireDigest};

/// Rank-0 bytes this session read, checked against their reference, and
/// charged for.
///
/// The *proof* for this class is complete — FD-2.1 rank 0 is terminal and
/// FD-2.5 forbids parsing or promoting such bytes, so there is nothing further
/// to establish. It is nonetheless crate-private, because a complete proof
/// algorithm and a correctly wired authority to request it are different
/// things: the slot expectation still comes from a call site rather than from
/// a consuming schema (FD-2.5), and a public entry point would make that the
/// weaker of two routes the moment schema-derived slots exist.
///
/// Private fields, no public constructor, no `Deserialize`, no `Default`, no
/// conversion from a reference. The only site that mints one is
/// [`crate::ResolutionSession::resolve_opaque`], which is what makes "this value
/// exists" mean "the proof boundary ran".
///
/// It is a *different type* from [`crate::ResolvedEnvelope`] rather than a
/// variant of one struct with optional fields, because the two prove different
/// things. Unifying the result must not unify the proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedOpaque<'brand> {
    kind: ArtifactKindV1,
    media_type: String,
    digest: WireDigest,
    bytes: Vec<u8>,
    /// Invariant in `'brand`: two sessions' resolved values are not
    /// interchangeable, and neither is a subtype of the other.
    brand: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> ResolvedOpaque<'brand> {
    /// Crate-private, and reachable from exactly one call site.
    ///
    /// Taking the verified bytes by value rather than a "verified" flag is the
    /// difference between a value that *carries* its evidence and one that
    /// *claims* it.
    pub(crate) fn mint(
        kind: ArtifactKindV1,
        media_type: String,
        digest: WireDigest,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            kind,
            media_type,
            digest,
            bytes,
            brand: PhantomData,
        }
    }

    /// FD-1.8 — the kind this object was resolved *as*, which is the slot's
    /// expectation and not the sender's declaration (FD-2.5).
    #[must_use]
    pub(crate) fn kind(&self) -> ArtifactKindV1 {
        self.kind
    }

    /// FD-1.7 — the media type the slot expected and the reference declared.
    #[must_use]
    pub(crate) fn media_type(&self) -> &str {
        &self.media_type
    }

    /// FD-1.1 — the digest the stored bytes were verified against.
    ///
    /// Brand-free, and that is correct: a digest is a fact. Outside the session
    /// it means "these bytes hash to this", not "this session verified it".
    #[must_use]
    pub(crate) fn digest(&self) -> &WireDigest {
        &self.digest
    }

    /// The verified stored bytes.
    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The stored size, which resolution proved equal to the declared size.
    #[must_use]
    pub(crate) fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// FD-1.5 — typed object identity for closure deduplication:
    /// `(ref.kind, ref.digest)`, never the digest alone. The same bytes reached
    /// through two different typed slots are two nodes (FD-2.5).
    #[must_use]
    pub(crate) fn identity(&self) -> (ArtifactKindV1, &str) {
        (self.kind, self.digest.as_str())
    }
}
