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
//! Used *inside* its session, the same code compiles and resolves — so the
//! failure below is the escape and not a broken example. The pair is the point:
//! a `compile_fail` test that fails for an unrelated reason passes just as
//! greenly as one that fails for the right one.
//!
//! ```
//! use o7_a1_cas::{MemoryStore, OpaqueSlot, ResolutionSession};
//! use o7_a1_contracts::{ArtifactKindV1, ArtifactRef, BudgetPolicy};
//!
//! let policy = BudgetPolicy {
//!     max_provider_turns: 1,
//!     max_wall_time_seconds: 1,
//!     evidence_budget_bytes: 1024,
//!     closure_object_budget: 8,
//! };
//! let mut store = MemoryStore::new();
//! let digest = store.put(b"diff".to_vec());
//! let r = ArtifactRef::new(ArtifactKindV1::Diff, "text/x-diff", digest, 4).unwrap();
//! let slot = OpaqueSlot::new(ArtifactKindV1::Diff, "text/x-diff");
//!
//! // The evidence is observed inside the session that paid for it, and only a
//! // brand-free summary leaves.
//! let size = ResolutionSession::enter(&policy, |s| {
//!     s.resolve_opaque(&slot, &r, &store).unwrap().size()
//! })
//! .unwrap();
//! assert_eq!(size, 4);
//! ```
//!
//! A resolved artifact cannot escape its session:
//!
//! ```compile_fail
//! use o7_a1_cas::{OpaqueSlot, ResolutionSession, MemoryStore, ResolvedArtifact};
//! use o7_a1_contracts::{ArtifactKindV1, ArtifactRef, BudgetPolicy};
//!
//! let policy = BudgetPolicy {
//!     max_provider_turns: 1,
//!     max_wall_time_seconds: 1,
//!     evidence_budget_bytes: 1024,
//!     closure_object_budget: 8,
//! };
//! let mut store = MemoryStore::new();
//! let digest = store.put(b"diff".to_vec());
//! let r = ArtifactRef::new(ArtifactKindV1::Diff, "text/x-diff", digest, 4).unwrap();
//! let slot = OpaqueSlot::new(ArtifactKindV1::Diff, "text/x-diff");
//!
//! // The brand is universally quantified, so the closure's return type cannot
//! // mention it: carrying the proof out of the session does not compile.
//! let escaped: ResolvedOpaque<'_> =
//!     ResolutionSession::enter(&policy, |s| s.resolve_opaque(&slot, &r, &store).unwrap()).unwrap();
//! ```

use std::marker::PhantomData;

use o7_a1_contracts::{ArtifactKindV1, WireDigest};

/// Rank-0 bytes this session read, checked against their reference, and
/// charged for.
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
pub struct ResolvedOpaque<'brand> {
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
    pub fn kind(&self) -> ArtifactKindV1 {
        self.kind
    }

    /// FD-1.7 — the media type the slot expected and the reference declared.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// FD-1.1 — the digest the stored bytes were verified against.
    ///
    /// Brand-free, and that is correct: a digest is a fact. Outside the session
    /// it means "these bytes hash to this", not "this session verified it".
    #[must_use]
    pub fn digest(&self) -> &WireDigest {
        &self.digest
    }

    /// The verified stored bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The stored size, which resolution proved equal to the declared size.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// FD-1.5 — typed object identity for closure deduplication:
    /// `(ref.kind, ref.digest)`, never the digest alone. The same bytes reached
    /// through two different typed slots are two nodes (FD-2.5).
    #[must_use]
    pub fn identity(&self) -> (ArtifactKindV1, &str) {
        (self.kind, self.digest.as_str())
    }
}

/// One surface over both proof classes, and no more than that.
///
/// The unification is deliberately shallow. `identity` and `stored_size` are
/// genuinely common — a closure deduplicates and accounts over both kinds of
/// node without caring which it holds — so they live here. Everything a class
/// *proved* stays behind its own type, reachable only by naming the class:
///
/// ```text
/// ResolvedOpaque      bytes hash to a digest, and the slot expected bytes
/// ResolvedEnvelope    an admitted envelope frames to a digest, binds a
///                     payload beneath it, and the two halves sum to the
///                     declared size
/// ```
///
/// Written as an enum rather than a struct with optional fields on purpose.
/// `envelope: Option<EnvelopeV1>` would answer "is there an envelope?" with
/// `None` for an opaque resolution, which reads as *no* rather than *wrong
/// question* — and a caller that treats a missing proof as an absent one is how
/// a boundary gets walked around by code that never meant to.
///
/// Unifying the result must not unify the proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedArtifact<'brand> {
    /// FD-2.1 rank 0: terminal bytes, never parsed, never promoted.
    Opaque(ResolvedOpaque<'brand>),
    /// One of the eleven message kinds, stored as two byte strings (FD-1.8).
    ///
    /// Boxed because the envelope proof is much the larger of the two, and an
    /// enum sized for its biggest variant would make every opaque node in a
    /// closure pay for the envelope case it is not.
    Envelope(Box<crate::ResolvedEnvelope<'brand>>),
}

impl<'brand> ResolvedArtifact<'brand> {
    /// FD-1.5 — `(kind, digest)`, the identity a closure deduplicates by.
    ///
    /// Common to both classes because deduplication is: "the same bytes
    /// referenced through two different typed slots are two distinct nodes"
    /// (FD-2.5) is a statement about the pair, whatever the pair describes.
    #[must_use]
    pub fn identity(&self) -> (o7_a1_contracts::ArtifactKindV1, &str) {
        match self {
            Self::Opaque(o) => o.identity(),
            Self::Envelope(e) => e.identity(),
        }
    }

    /// The stored size resolution proved — one object's bytes for an opaque
    /// node, both halves for an envelope-bearing one (FD-1.8).
    #[must_use]
    pub fn stored_size(&self) -> u64 {
        match self {
            Self::Opaque(o) => o.size(),
            Self::Envelope(e) => e.stored_size(),
        }
    }
}
