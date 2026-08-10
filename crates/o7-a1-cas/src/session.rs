//! The accounting authority, and the single site that mints a resolved value.
//!
//! # Who owns what
//!
//! ```text
//! resolver session owns accounting state
//! resolved value   owns resolution evidence
//! resolved value   is bound to the session that paid for that evidence
//! caller           owns neither proof nor accounting
//! ```
//!
//! The caller never supplies "charged so far". A caller-provided running total
//! is the caller-selected byte ceiling of step 1 wearing a different hat: the
//! fixture sets the number that the check then compares against, and the bound
//! becomes a parameter rather than a bound.
//!
//! Equally, the *returned* value does not own the total either. If a
//! `ResolvedOpaque` carried `remaining_budget`, two resolved objects would
//! carry two answers to one question, and the closure would have reinvented
//! distributed state inside a single traversal.
//!
//! # Effective bounds
//!
//! FD-1.5: "The **effective** bound for a resolution is `min(hard maximum,
//! campaign policy)`." Both halves are held here, computed once at
//! [`ResolutionSession::enter`], so no per-call site gets to pick.

use std::cell::RefCell;
use std::collections::HashSet;
use std::marker::PhantomData;

use o7_a1_contracts::{
    typed_media_type, ArtifactKindV1, ArtifactRef, BudgetPolicy, BudgetPolicyError, EnvelopeV1,
    MessageKindV1, ParseError, WireDigest, MESSAGE_KIND_VERSION_V1,
};

use crate::envelope::{admit_envelope, ResolvedEnvelope};
use crate::resolved::ResolvedOpaque;
use crate::store::BackingStore;

/// A reference slot that expects **opaque** bytes: rank 0 in FD-2.1, terminal
/// in the reference graph, never parsed and never promoted to an authority
/// object because it happens to look like one (FD-2.5).
///
/// The slot carries the expectation; the reference carries a claim. The
/// resolver checks the stored object against *this*, not against what the
/// sender declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueSlot {
    kind: ArtifactKindV1,
    media_type: String,
}

impl OpaqueSlot {
    /// # Panics
    /// Never. A slot naming a typed kind is refused at resolution rather than
    /// at construction, because the slot is declared by a schema and the
    /// refusal belongs where the contract states it (FD-2.5).
    #[must_use]
    pub fn new(kind: ArtifactKindV1, media_type: impl Into<String>) -> Self {
        Self {
            kind,
            media_type: media_type.into(),
        }
    }

    #[must_use]
    pub fn kind(&self) -> ArtifactKindV1 {
        self.kind
    }

    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
}

/// A reference slot that expects an **envelope-bearing** artifact: one of the
/// eleven message kinds, stored as two byte strings (FD-1.8).
///
/// # The expectation is derived, not supplied
///
/// A slot is the side of the FD-2.5 comparison that is *not* untrusted, so
/// anything a caller can choose about it is authority a caller has been handed.
/// The media type is therefore not a parameter: FD-1.7 fixes it as
/// `application/vnd.o7.a1.<kind>+json; v=<version>`, so it is computed from the
/// kind and the frozen message-kind version. Passing it in would have let a
/// caller name a kind and then declare whatever media type made a reference
/// match.
///
/// The kind itself is still chosen at the call site, and that is a **known
/// remaining gap**, recorded rather than glossed: FD-2.5 says "every reference
/// slot has one expected `kind` and expected media type, fixed by the schemas
/// in §3", so the kind should arrive from the parent payload's schema, not from
/// whoever is calling. No payload schema exists yet to derive it from, and
/// inventing a slot registry before the schemas that populate it would be
/// decoration. What must not happen is this surviving into a traversal: once a
/// parent schema can hand out its own slots, a caller-authored slot becomes the
/// weaker of two routes to one authority-relevant answer, and the weaker route
/// is the one that decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeSlot {
    kind: MessageKindV1,
    media_type: String,
}

impl EnvelopeSlot {
    /// The slot a consuming schema declares for `kind`.
    ///
    /// Takes [`MessageKindV1`] rather than [`ArtifactKindV1`]: only the eleven
    /// envelope-bearing kinds can occupy an envelope slot, and a type that
    /// cannot name the other twenty-eight is a better statement of that than a
    /// runtime refusal.
    #[must_use]
    pub fn new(kind: MessageKindV1) -> Self {
        Self {
            kind,
            // FD-1.7, computed once here so no call site can disagree with it.
            media_type: typed_media_type(kind.artifact_kind(), MESSAGE_KIND_VERSION_V1),
        }
    }

    #[must_use]
    pub fn message_kind(&self) -> MessageKindV1 {
        self.kind
    }

    #[must_use]
    pub fn kind(&self) -> ArtifactKindV1 {
        self.kind.artifact_kind()
    }

    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
}

/// A resolution that failed, closed.
///
/// FD-1.5: "Resolution is all-or-nothing: a closure that exceeds a bound is
/// never partially accepted." No variant carries stored bytes or a
/// caller-supplied media type — a rejected object is untrusted input, and the
/// step-1 discipline about error content applies unchanged (AGENTS.md P0).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("the slot expects kind {expected}, the reference declares another")]
    WrongKindForSlot { expected: &'static str },
    #[error("the slot expects a different media type for {kind}")]
    WrongMediaTypeForSlot { kind: &'static str },
    #[error("{kind} is a typed A1 object; an opaque slot must not resolve one")]
    NotOpaque { kind: &'static str },
    #[error("no object is stored under the referenced digest")]
    ObjectMissing,
    #[error("the stored bytes do not hash to the referenced digest")]
    DigestMismatch,
    #[error("the reference declares {declared} bytes, the store holds {stored}")]
    SizeMismatch { declared: u64, stored: u64 },
    #[error("the closure would reach {actual} bytes, exceeding the effective bound {max}")]
    ClosureBytesExceeded { actual: u64, max: u64 },
    #[error("the closure would reach {actual} objects, exceeding the effective bound {max}")]
    ClosureObjectsExceeded { actual: u32, max: u32 },
    #[error("the stored envelope is not admissible: {reason}")]
    EnvelopeInadmissible { reason: ParseError },
    #[error("the stored envelope does not frame to the referenced digest")]
    FramedDigestMismatch,
    #[error("the stored envelope declares message_kind {actual}, the slot expects {expected}")]
    EnvelopeKindMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("no payload is stored under the envelope's payload_digest")]
    PayloadMissing,
    #[error("the stored payload does not hash to the envelope's payload_digest")]
    PayloadDigestMismatch,
    #[error("the reference declares {declared} bytes; the two stored halves are {stored}")]
    HalvesSizeMismatch { declared: u64, stored: u64 },
}

/// FD-1.5 — `min(hard maximum, campaign policy)`, computed once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveLimits {
    reachable_closure_bytes: u64,
    reachable_closure_objects: u32,
}

impl EffectiveLimits {
    #[must_use]
    pub fn reachable_closure_bytes(&self) -> u64 {
        self.reachable_closure_bytes
    }

    #[must_use]
    pub fn reachable_closure_objects(&self) -> u32 {
        self.reachable_closure_objects
    }
}

#[derive(Debug, Default)]
struct Accounting {
    /// FD-1.5 — typed object identity, `(kind, digest)`, never digest alone.
    seen: HashSet<(ArtifactKindV1, String)>,
    bytes: u64,
    objects: u32,
}

/// One resolution, and the accounting authority for it.
///
/// `'brand` is invariant and is introduced by [`Self::enter`] under a
/// higher-ranked bound, so values minted here cannot leave this session and are
/// not interchangeable with another session's.
#[derive(Debug)]
pub struct ResolutionSession<'brand> {
    limits: EffectiveLimits,
    state: RefCell<Accounting>,
    brand: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl ResolutionSession<'_> {
    /// Run one resolution under a campaign's budget policy.
    ///
    /// The closure receives a session whose brand is universally quantified, so
    /// `R` cannot mention it: a resolved value has no way out. That is the
    /// compile-time form of "evidence does not outlive the accounting authority
    /// that paid for it".
    ///
    /// # Errors
    /// [`BudgetPolicyError`] if the campaign policy itself exceeds a protocol
    /// hard maximum — checked here rather than at first use, because FD-1.5
    /// checks it "at campaign creation".
    pub fn enter<R>(
        policy: &BudgetPolicy,
        f: impl for<'brand> FnOnce(&ResolutionSession<'brand>) -> R,
    ) -> Result<R, BudgetPolicyError> {
        policy.validate()?;
        // `min(hard maximum, campaign policy)` is computed by `BudgetPolicy`
        // itself and is not recomputed here. A second copy of that arithmetic
        // would be a second route to one authority-relevant answer, which is
        // the defect class this whole step exists to avoid — and it would drift
        // the first time FD-1.5's hard maxima are superseded.
        let limits = EffectiveLimits {
            reachable_closure_bytes: policy.effective_closure_bytes(),
            reachable_closure_objects: policy.effective_closure_objects(),
        };
        let session = ResolutionSession {
            limits,
            state: RefCell::new(Accounting::default()),
            brand: PhantomData,
        };
        Ok(f(&session))
    }

    /// The effective bounds this session is enforcing.
    #[must_use]
    pub fn limits(&self) -> EffectiveLimits {
        self.limits
    }

    /// Bytes charged so far. Readable, never settable.
    #[must_use]
    pub fn charged_bytes(&self) -> u64 {
        self.state.borrow().bytes
    }

    /// Distinct objects charged so far, deduplicated by `(kind, digest)`.
    #[must_use]
    pub fn charged_objects(&self) -> u32 {
        self.state.borrow().objects
    }
}

impl<'brand> ResolutionSession<'brand> {
    /// The proof boundary for a rank-0 opaque object.
    ///
    /// Order is the contract's, not a convenience: slot expectation, then
    /// deduplicate, then charge the **declared** size, then check the bounds,
    /// and only then read. FD-1.5: "Accounting uses the *declared* `size`
    /// before reading, so an oversized blob is refused rather than streamed."
    ///
    /// A reference already seen in this closure is not charged twice — FD-1.5
    /// deduplicates before accounting — but it is still verified, because
    /// resolution returns evidence and evidence is not a cache entry.
    ///
    /// # Scope
    /// Rank 0 only, and that is a completeness statement rather than a partial
    /// implementation: FD-2.1 rank 0 is terminal, FD-2.5 says such bytes are
    /// "never parsed and never promoted", so there is nothing further to do for
    /// this class. Typed and envelope-bearing resolution — where FD-1.8
    /// requires *both* halves to be checked and the payload schema decides what
    /// to enqueue — is the next slice, and this method refuses those kinds
    /// rather than half-resolving them.
    ///
    /// # Errors
    /// [`ResolveError`], always closed. A resolution that fails yields no value
    /// at all, never a partially accepted one.
    pub fn resolve_opaque(
        &self,
        slot: &OpaqueSlot,
        reference: &ArtifactRef,
        store: &dyn BackingStore,
    ) -> Result<ResolvedOpaque<'brand>, ResolveError> {
        // FD-2.5: "The resolver checks the stored object against the *slot's*
        // expectation, not against the sender's declaration."
        if reference.kind() != slot.kind() {
            return Err(ResolveError::WrongKindForSlot {
                expected: slot.kind().name(),
            });
        }
        if reference.media_type() != slot.media_type() {
            return Err(ResolveError::WrongMediaTypeForSlot {
                kind: slot.kind().name(),
            });
        }
        // FD-2.1 rank 0 / FD-2.5: only a slot expecting a typed object causes a
        // parse, so an opaque slot must never be pointed at one.
        if slot.kind().is_typed_artifact() {
            return Err(ResolveError::NotOpaque {
                kind: slot.kind().name(),
            });
        }

        self.charge(reference)?;

        let stored = store
            .get(reference.digest())
            .ok_or(ResolveError::ObjectMissing)?;
        // FD-1.8: the stored object must reproduce the reference. Both halves
        // of that: the content digest, and the size the reference declared.
        if WireDigest::of_bytes(stored.bytes()) != *reference.digest() {
            return Err(ResolveError::DigestMismatch);
        }
        if stored.stored_size() != reference.size() {
            return Err(ResolveError::SizeMismatch {
                declared: reference.size(),
                stored: stored.stored_size(),
            });
        }

        Ok(ResolvedOpaque::mint(
            slot.kind(),
            slot.media_type().to_owned(),
            reference.digest().clone(),
            stored.bytes().to_vec(),
        ))
    }

    /// The proof boundary for an envelope-bearing artifact: FD-1.8's two
    /// stored byte strings, both verified against the one reference.
    ///
    /// The order is the contract's. Slot expectation first, then deduplicate
    /// and charge the declared size before any read, then the halves:
    ///
    /// ```text
    /// envelope bytes  -> parse_artifact::<EnvelopeV1>   full step-1 admission
    ///                 -> framed_digest == ref.digest    FD-1.2 / FD-1.8
    ///                 -> message_kind  == slot kind     FD-2.5, three-way
    /// payload bytes   -> hashes to envelope.payload_digest        FD-1.1
    /// both halves     -> sum to ref.size                          FD-1.8
    /// ```
    ///
    /// Every one of those is load-bearing. Verifying the payload and stopping
    /// leaves the envelope unchecked while the value looks resolved; verifying
    /// the envelope and stopping leaves the payload unbound; checking neither
    /// size leaves the reference free to under-declare what a closure will
    /// actually read.
    ///
    /// # Errors
    /// [`ResolveError`], always closed and never partial.
    pub fn resolve_envelope(
        &self,
        slot: &EnvelopeSlot,
        reference: &ArtifactRef,
        store: &dyn BackingStore,
    ) -> Result<ResolvedEnvelope<'brand>, ResolveError> {
        // FD-2.5: the slot's expectation, not the sender's declaration.
        if reference.kind() != slot.kind() {
            return Err(ResolveError::WrongKindForSlot {
                expected: slot.kind().name(),
            });
        }
        if reference.media_type() != slot.media_type() {
            return Err(ResolveError::WrongMediaTypeForSlot {
                kind: slot.kind().name(),
            });
        }
        self.charge(reference)?;

        // An envelope is keyed by its framed identity, because that is what a
        // reference names (FD-1.8) — not by the hash of its bytes.
        let stored_envelope = store
            .get(reference.digest())
            .ok_or(ResolveError::ObjectMissing)?;
        // No shortcut: the same public admission path a peer's bytes take.
        let envelope = admit_envelope(stored_envelope.bytes())
            .map_err(|reason| ResolveError::EnvelopeInadmissible { reason })?;

        // FD-1.2 / FD-1.8 — the stored envelope must reproduce ref.digest under
        // framing. Recomputed from the admitted value, never read from input.
        let framed = envelope.framed_digest();
        if framed != *reference.digest() {
            return Err(ResolveError::FramedDigestMismatch);
        }
        // The third side of the FD-2.5 comparison: what the bytes say they are.
        // Without this, a peer could have bytes admitted under one schema and
        // handed back as authority under a different kind.
        let actual = envelope.message_kind().artifact_kind();
        if actual != slot.kind() {
            return Err(ResolveError::EnvelopeKindMismatch {
                expected: slot.kind().name(),
                actual: actual.name(),
            });
        }

        let stored_payload = store
            .get(envelope.payload_digest())
            .ok_or(ResolveError::PayloadMissing)?;
        // FD-1.1 — over the exact stored bytes; the envelope owns this check.
        if !envelope.binds_payload(stored_payload.bytes()) {
            return Err(ResolveError::PayloadDigestMismatch);
        }

        // FD-1.8 — "the size check is `envelope_bytes + payload_bytes ==
        // ref.size`". Both halves, or the declared size bounded nothing.
        let stored =
            EnvelopeV1::ref_size(stored_envelope.stored_size(), stored_payload.stored_size());
        if stored != reference.size() {
            return Err(ResolveError::HalvesSizeMismatch {
                declared: reference.size(),
                stored,
            });
        }

        Ok(ResolvedEnvelope::mint(
            envelope,
            framed,
            stored_payload.bytes().to_vec(),
            stored_envelope.stored_size(),
        ))
    }

    /// Deduplicate, then account, then bound — in that order, and against the
    /// declared size.
    fn charge(&self, reference: &ArtifactRef) -> Result<(), ResolveError> {
        let identity = (reference.kind(), reference.digest().as_str().to_owned());
        let mut state = self.state.borrow_mut();
        if state.seen.contains(&identity) {
            return Ok(());
        }

        let objects = state.objects.saturating_add(1);
        if objects > self.limits.reachable_closure_objects {
            return Err(ResolveError::ClosureObjectsExceeded {
                actual: objects,
                max: self.limits.reachable_closure_objects,
            });
        }
        let bytes = state.bytes.saturating_add(reference.size());
        if bytes > self.limits.reachable_closure_bytes {
            return Err(ResolveError::ClosureBytesExceeded {
                actual: bytes,
                max: self.limits.reachable_closure_bytes,
            });
        }

        // Recorded only once the charge is admissible: a refused resolution
        // must leave no trace that a later one could mistake for payment.
        state.seen.insert(identity);
        state.objects = objects;
        state.bytes = bytes;
        Ok(())
    }
}
