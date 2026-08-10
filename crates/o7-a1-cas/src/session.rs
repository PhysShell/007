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
//! `ResolvedArtifact` carried `remaining_budget`, two resolved objects would
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

use o7_a1_contracts::{ArtifactKindV1, ArtifactRef, BudgetPolicy, BudgetPolicyError, WireDigest};

use crate::resolved::ResolvedArtifact;
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
    ) -> Result<ResolvedArtifact<'brand>, ResolveError> {
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

        Ok(ResolvedArtifact::mint(
            slot.kind(),
            slot.media_type().to_owned(),
            reference.digest().clone(),
            stored.bytes().to_vec(),
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
