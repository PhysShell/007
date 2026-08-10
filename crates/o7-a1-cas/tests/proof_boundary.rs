//! The forbidden routes, asserted rather than intended.
//!
//! Step 1 established the pattern the hard way: a rule that is documented but
//! not mechanised is a rule some later route walks straight through, usually
//! with a test recording the walk as a pass. So this suite is mostly *negative
//! space* — what cannot be constructed, converted, escaped or forged — and the
//! few positive tests exist to prove the boundary is not simply refusing
//! everything.
//!
//! The compile-time half lives in `compile_fail` doctests on
//! [`o7_a1_cas::ResolvedArtifact`], because "this does not compile" is not
//! expressible as a runtime assertion.

// Fixtures are valid by construction; one that is not should fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::marker::PhantomData;

use o7_a1_cas::{
    BackingStore, MemoryStore, OpaqueSlot, RawObject, ResolutionSession, ResolveError,
    ResolvedArtifact,
};
use o7_a1_contracts::{
    typed_media_type, ArtifactKindV1, ArtifactRef, BudgetPolicy, WireDigest,
    MAX_REACHABLE_CLOSURE_OBJECTS,
};

fn policy() -> BudgetPolicy {
    BudgetPolicy {
        max_provider_turns: 8,
        max_wall_time_seconds: 3600,
        evidence_budget_bytes: 4096,
        closure_object_budget: 8,
    }
}

fn diff_slot() -> OpaqueSlot {
    OpaqueSlot::new(ArtifactKindV1::Diff, "text/x-diff")
}

/// A ref describing bytes truthfully. Every negative test below starts from
/// this and varies **one** thing about the input — never a bound, never a
/// check.
fn honest_ref(kind: ArtifactKindV1, media: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef::new(kind, media, WireDigest::of_bytes(bytes), bytes.len() as u64)
        .expect("the fixture reference must be admissible")
}

/// A store that answers every read with bytes of the fixture's choosing.
///
/// This is the whole hostile-fixture story, and it is deliberately *not* a
/// back door into the write path: it implements the read trait, so the worst it
/// can do is hand the resolver bad input. It has no way to return a
/// `ResolvedArtifact` — nothing outside the session can mint one.
struct HostileStore(Vec<u8>);

impl BackingStore for HostileStore {
    fn get(&self, _digest: &WireDigest) -> Option<RawObject> {
        Some(RawObject::from_backing_store(self.0.clone()))
    }
}

struct EmptyStore;

impl BackingStore for EmptyStore {
    fn get(&self, _digest: &WireDigest) -> Option<RawObject> {
        None
    }
}

// ---------------------------------------------------------------------------
// Negative space: what cannot be minted, converted, or serialized
// ---------------------------------------------------------------------------

/// Autoref specialisation: the inherent method applies only when the bound
/// holds, and an inherent method wins over a trait method when it applies. So
/// what these report is whether an impl *exists*, which is not otherwise
/// observable at runtime.
struct Probe<T>(PhantomData<T>);
trait NotSerialize {
    fn holds(&self) -> bool {
        false
    }
}
impl<T> NotSerialize for Probe<T> {}
impl<T: serde::Serialize> Probe<T> {
    fn holds(&self) -> bool {
        true
    }
}

struct DefaultProbe<T>(PhantomData<T>);
trait NotDefault {
    fn holds(&self) -> bool {
        false
    }
}
impl<T> NotDefault for DefaultProbe<T> {}
impl<T: Default> DefaultProbe<T> {
    fn holds(&self) -> bool {
        true
    }
}

struct Convert<A, B>(PhantomData<(A, B)>);
trait NoConversion {
    fn holds(&self) -> bool {
        false
    }
}
impl<A, B> NoConversion for Convert<A, B> {}
impl<A, B: From<A>> Convert<A, B> {
    fn holds(&self) -> bool {
        true
    }
}

/// Property 2, mechanised: `ArtifactRef` is not `ResolvedArtifact`, and there is
/// no conversion that quietly says otherwise.
///
/// `From<ArtifactRef> for ResolvedArtifact` is the most tempting API on this
/// step — it looks natural and the compiler is content — and it is an admitted
/// envelope with its producer role reassigned after admission, in a new
/// costume. So its absence is asserted, not trusted.
#[test]
fn a_reference_does_not_convert_into_a_resolution() {
    assert!(
        !Convert::<ArtifactRef, ResolvedArtifact<'static>>(PhantomData).holds(),
        "From<ArtifactRef> for ResolvedArtifact must not exist"
    );
    assert!(
        !Convert::<RawObject, ResolvedArtifact<'static>>(PhantomData).holds(),
        "untrusted store bytes must not convert into evidence either"
    );
    // The probe reports the bound honestly rather than always saying false.
    assert!(Convert::<&str, String>(PhantomData).holds());
}

/// Property 3: evidence has no transportable representation.
///
/// A `Serialize` alone is not yet a bypass — without a `Deserialize` or a
/// public constructor there is nothing to read the bytes back into, so the
/// brand still holds. What it *does* create is a portable rendering of a
/// proof-bearing object, which is a standing invitation for someone later to
/// add the obvious matching door and call it symmetry. The right shape for this
/// type is:
///
/// ```text
/// observable while borrowed
/// not transportable as proof
/// ```
///
/// so the absence is asserted now rather than defended later.
#[test]
fn resolution_evidence_has_no_transportable_form() {
    assert!(
        !Probe::<ResolvedArtifact<'static>>(PhantomData).holds(),
        "ResolvedArtifact must not be Serialize"
    );
    assert!(
        Probe::<String>(PhantomData).holds(),
        "the probe must report an implemented bound as true"
    );
}

/// The three routes that must not exist, named individually because "there is
/// no way to forge one" is the kind of claim that stays true only while
/// somebody keeps checking.
///
/// The third is the one to watch. `From<ArtifactRef>` and `From<RawObject>`
/// look like design mistakes; a `new_for_test` or `assume_resolved` does not —
/// it looks like pragmatism, and it arrives with the words "only for tests",
/// which is how the previous step acquired a diagnostic lever that a later
/// round had to remove.
#[test]
fn nothing_outside_the_resolver_can_mint_evidence() {
    assert!(
        !Convert::<ArtifactRef, ResolvedArtifact<'static>>(PhantomData).holds(),
        "a claim must not convert into a proof"
    );
    assert!(
        !Convert::<RawObject, ResolvedArtifact<'static>>(PhantomData).holds(),
        "untrusted store bytes must not convert into a proof"
    );
    assert!(
        !Convert::<Vec<u8>, ResolvedArtifact<'static>>(PhantomData).holds(),
        "nor may raw bytes"
    );
    // `Default` is the quietest constructor of all: it takes no arguments, so
    // it can never have checked anything.
    assert!(
        !DefaultProbe::<ResolvedArtifact<'static>>(PhantomData).holds(),
        "ResolvedArtifact must not be Default"
    );
    assert!(DefaultProbe::<Vec<u8>>(PhantomData).holds());
}

// ---------------------------------------------------------------------------
// The boundary does its job
// ---------------------------------------------------------------------------

#[test]
fn an_honest_reference_resolves_and_carries_its_evidence() {
    let mut store = MemoryStore::new();
    store.put(b"--- a/x\n+++ b/x\n".to_vec());
    let reference = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"--- a/x\n+++ b/x\n");

    ResolutionSession::enter(&policy(), |session| {
        let resolved = session
            .resolve_opaque(&diff_slot(), &reference, &store)
            .expect("an honest reference must resolve");
        assert_eq!(resolved.kind(), ArtifactKindV1::Diff);
        assert_eq!(resolved.bytes(), b"--- a/x\n+++ b/x\n");
        assert_eq!(resolved.size(), reference.size());
        assert_eq!(session.charged_bytes(), reference.size());
        assert_eq!(session.charged_objects(), 1);
    })
    .expect("the fixture policy is admissible");
}

/// FD-2.5: the resolver checks the stored object against the **slot's**
/// expectation, not against the sender's declaration.
#[test]
fn the_slot_expectation_wins_over_the_senders_declaration() {
    let mut store = MemoryStore::new();
    store.put(b"log".to_vec());

    ResolutionSession::enter(&policy(), |session| {
        // A reference claiming a different kind than the slot expects.
        let wrong_kind = honest_ref(ArtifactKindV1::GateLog, "text/x-diff", b"log");
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &wrong_kind, &store),
            Err(ResolveError::WrongKindForSlot { .. })
        ));

        // ...and one claiming a different media type for the right kind.
        let wrong_media = honest_ref(ArtifactKindV1::Diff, "application/octet-stream", b"log");
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &wrong_media, &store),
            Err(ResolveError::WrongMediaTypeForSlot { .. })
        ));

        // The same bytes through the matching slot are admissible, so the two
        // refusals are the expectation and not a broken fixture.
        let honest = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"log");
        assert!(session
            .resolve_opaque(&diff_slot(), &honest, &store)
            .is_ok());
    })
    .unwrap();
}

/// FD-2.5: "Rank-0 bytes are never parsed and never promoted to an authority
/// object because they happen to look like one." An opaque slot pointed at a
/// typed A1 kind is refused rather than half-resolved.
#[test]
fn an_opaque_slot_refuses_a_typed_artifact() {
    let mut store = MemoryStore::new();
    store.put(b"{}".to_vec());
    let kind = ArtifactKindV1::ProviderExecutionReceipt;
    let media = typed_media_type(kind, 1);
    let slot = OpaqueSlot::new(kind, media.clone());
    let reference = honest_ref(kind, &media, b"{}");

    ResolutionSession::enter(&policy(), |session| {
        assert!(matches!(
            session.resolve_opaque(&slot, &reference, &store),
            Err(ResolveError::NotOpaque { .. })
        ));
    })
    .unwrap();
}

/// FD-1.8 — the stored object must reproduce the reference. Both halves.
#[test]
fn a_store_that_lies_is_caught_on_both_halves() {
    let reference = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"the real bytes");

    ResolutionSession::enter(&policy(), |session| {
        // Different bytes of the same length: only the digest can catch this.
        let swapped = HostileStore(b"the fake bytes".to_vec());
        assert_eq!(swapped.0.len(), b"the real bytes".len());
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &reference, &swapped),
            Err(ResolveError::DigestMismatch)
        ));
    })
    .unwrap();

    // A store whose object is a different size than the reference declared.
    // Built so the digest check cannot be what fires: the reference names the
    // digest of the *stored* bytes and lies only about the size.
    let stored = b"short".to_vec();
    let lying_size = ArtifactRef::new(
        ArtifactKindV1::Diff,
        "text/x-diff",
        WireDigest::of_bytes(&stored),
        999,
    )
    .unwrap();
    ResolutionSession::enter(&policy(), |session| {
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &lying_size, &HostileStore(stored.clone())),
            Err(ResolveError::SizeMismatch {
                declared: 999,
                stored: 5
            })
        ));
    })
    .unwrap();
}

#[test]
fn an_absent_object_is_refused_rather_than_treated_as_empty() {
    let reference = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"absent");
    ResolutionSession::enter(&policy(), |session| {
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &reference, &EmptyStore),
            Err(ResolveError::ObjectMissing)
        ));
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Accounting authority
// ---------------------------------------------------------------------------

/// FD-1.5: deduplicate **before** accounting, by `(kind, digest)` — never by
/// digest alone, because "the same bytes referenced through two different typed
/// slots are two distinct nodes".
#[test]
fn deduplication_is_by_kind_and_digest_together() {
    let bytes = b"shared evidence";
    let mut store = MemoryStore::new();
    store.put(bytes.to_vec());

    ResolutionSession::enter(&policy(), |session| {
        let as_diff = honest_ref(ArtifactKindV1::Diff, "text/x-diff", bytes);
        session
            .resolve_opaque(&diff_slot(), &as_diff, &store)
            .unwrap();
        let after_first = (session.charged_bytes(), session.charged_objects());

        // The same reference again: verified again, charged once.
        session
            .resolve_opaque(&diff_slot(), &as_diff, &store)
            .unwrap();
        assert_eq!(
            (session.charged_bytes(), session.charged_objects()),
            after_first,
            "a repeated reference must not be charged twice"
        );

        // The same digest under a different kind is a different node.
        let as_log = honest_ref(ArtifactKindV1::GateLog, "application/json", bytes);
        let log_slot = OpaqueSlot::new(ArtifactKindV1::GateLog, "application/json");
        session.resolve_opaque(&log_slot, &as_log, &store).unwrap();
        assert_eq!(session.charged_objects(), 2, "(kind, digest), not digest");
        assert_eq!(session.charged_bytes(), after_first.0 * 2);
    })
    .unwrap();
}

#[test]
fn the_closure_object_bound_refuses_rather_than_degrading() {
    let mut store = MemoryStore::new();
    store.put(b"one".to_vec());
    store.put(b"two".to_vec());
    let tight = BudgetPolicy {
        closure_object_budget: 1,
        ..policy()
    };

    ResolutionSession::enter(&tight, |session| {
        let first = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"one");
        let second = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"two");
        assert!(session.resolve_opaque(&diff_slot(), &first, &store).is_ok());
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &second, &store),
            Err(ResolveError::ClosureObjectsExceeded { max: 1, .. })
        ));
    })
    .unwrap();
}

/// FD-1.5: the **declared** size is accounted before any read, "so an oversized
/// blob is refused rather than streamed". The reference below declares more
/// than the budget; the store never has to be consulted.
#[test]
fn the_declared_size_is_charged_before_the_object_is_read() {
    let over_budget = ArtifactRef::new(
        ArtifactKindV1::Diff,
        "text/x-diff",
        WireDigest::of_bytes(b"irrelevant"),
        1_000_000,
    )
    .unwrap();

    ResolutionSession::enter(&policy(), |session| {
        // `EmptyStore` would answer `ObjectMissing` for anything. Getting
        // `ClosureBytesExceeded` instead is the proof that accounting ran first.
        assert!(matches!(
            session.resolve_opaque(&diff_slot(), &over_budget, &EmptyStore),
            Err(ResolveError::ClosureBytesExceeded { max: 4096, .. })
        ));
    })
    .unwrap();
}

/// A refused resolution must leave no trace a later one could mistake for
/// payment — neither a charge nor a dedup entry that would make the same object
/// look already paid for.
#[test]
fn a_refused_resolution_charges_nothing() {
    let mut store = MemoryStore::new();
    store.put(b"real".to_vec());
    let reference = honest_ref(ArtifactKindV1::Diff, "text/x-diff", b"real");

    ResolutionSession::enter(&policy(), |session| {
        // Refused by the slot rule, before any accounting.
        let wrong = honest_ref(ArtifactKindV1::GateLog, "text/x-diff", b"real");
        assert!(session
            .resolve_opaque(&diff_slot(), &wrong, &store)
            .is_err());
        assert_eq!(session.charged_objects(), 0);
        assert_eq!(session.charged_bytes(), 0);

        // Refused by integrity, after accounting: the charge stands, because
        // the closure genuinely reached that object. What must not happen is
        // the object being admitted.
        let corrupt = HostileStore(b"nope".to_vec());
        assert!(session
            .resolve_opaque(&diff_slot(), &reference, &corrupt)
            .is_err());

        // And the honest resolution still succeeds afterwards: the failed read
        // did not poison the session.
        assert!(session
            .resolve_opaque(&diff_slot(), &reference, &store)
            .is_ok());
    })
    .unwrap();
}

/// FD-1.5 — the effective bound is `min(hard maximum, campaign policy)`, and
/// the session does not recompute that arithmetic itself.
#[test]
fn effective_limits_are_the_minimum_of_policy_and_hard_maximum() {
    let generous = BudgetPolicy {
        evidence_budget_bytes: o7_a1_contracts::MAX_REACHABLE_CLOSURE_BYTES,
        closure_object_budget: MAX_REACHABLE_CLOSURE_OBJECTS,
        ..policy()
    };
    ResolutionSession::enter(&generous, |session| {
        assert_eq!(
            session.limits().reachable_closure_bytes(),
            generous.effective_closure_bytes()
        );
        assert_eq!(
            session.limits().reachable_closure_objects(),
            generous.effective_closure_objects()
        );
    })
    .unwrap();

    ResolutionSession::enter(&policy(), |session| {
        assert_eq!(session.limits().reachable_closure_bytes(), 4096);
        assert_eq!(session.limits().reachable_closure_objects(), 8);
    })
    .unwrap();
}

/// A policy above a protocol hard maximum is refused at session entry, which is
/// FD-1.5's "checked at campaign creation" — not at first use, where a
/// resolution would already be under way.
#[test]
fn a_policy_above_the_hard_maximum_never_opens_a_session() {
    let bad = BudgetPolicy {
        evidence_budget_bytes: o7_a1_contracts::MAX_REACHABLE_CLOSURE_BYTES + 1,
        ..policy()
    };
    assert!(ResolutionSession::enter(&bad, |_| ()).is_err());
}
