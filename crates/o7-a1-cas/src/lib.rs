//! `o7-a1-cas` — A1-V0 step 2: the owned CAS read boundary and the resolution
//! proof boundary.
//!
//! Normative source, not derived from this code:
//!
//! ```text
//! contract   docs/q-deck/a1-authority-contracts.md
//! blob  B2   e22539ddf4f7c9ab260e16835eef8ef18abbe726          (post-S2, PR #135)
//! superseded 3b26849cc39a3391aaed46cca56be3b6715afabb          (B1, post-S1)
//! step 1     a6625bc6473e3029a3309ddd7f2795ce57516a60          (PR #124)
//! ```
//!
//! S2 added the FD-1.3 member-name uniqueness rule, which this crate inherits
//! through `o7-a1-contracts` rather than implementing itself: nothing here parses
//! A1 JSON. The binding is recorded anyway, because a crate bound to a superseded
//! blob is bound to superseded authority whatever its own code does.
//!
//! Task: `docs/tasks/a1-v0-step-2.md`. Relevant sections: FD-1.5 (closure and
//! its bounds, frozen as an algorithm), FD-1.8 (what a reference identifies),
//! FD-2.1 (rank), FD-2.5 (the resolver's duties).
//!
//! # The four properties this crate exists to hold
//!
//! Step 1 spent eight review rounds on one failure class: a rule that held on
//! one route and not on another. These are that lesson, moved to where it now
//! applies.
//!
//! ```text
//! 1. Admission equivalence
//!    all sanctioned construction / resolution routes accept the same semantic set.
//!
//! 2. Equivalent representation is not equivalent provenance
//!    ArtifactRef is not ResolvedArtifact.
//!
//! 3. Proof-bearing state is minted only by the proof boundary
//!    tests may forge hostile inputs and hostile storage, never resolved state.
//!
//! 4. Accounting authority belongs to the resolver session
//!    resolver owns accounting state
//!    resolved value owns resolution evidence
//!    resolved value is bound to the session that paid for that evidence
//!    caller owns neither proof nor accounting
//! ```
//!
//! # The shape
//!
//! ```text
//!   ArtifactRef            a claim: these bytes exist, hash to this, are this big
//!        │
//!        │  ResolutionSession::resolve_*      slot expectation (FD-2.5)
//!        │                                    deduplicate by (kind, digest)
//!        │                                    charge the DECLARED size
//!        │                                    bound the closure
//!        │                                    read, then verify against the ref
//!        ▼
//!   ResolvedOpaque<'brand>                    rank-0 bytes, complete (FD-2.1)
//!   ResolvedEnvelopeStorage<'brand>           FD-1.8 integrity, both halves
//!                                             — storage only, see below
//! ```
//!
//! [`store::RawObject`] sits deliberately outside that arrow: it is what a
//! backing store returned, trusted by nobody. A hostile fixture is another
//! implementation of [`BackingStore`], and it can return anything at all — which
//! is what makes the negative tests possible without a second production API.
//! Forge the input, never the verdict.
//!
//! # What this slice claims, and what it does not
//!
//! It claims the **lower layer of the resolver**, finished:
//!
//! ```text
//! bounded structural admission     o7-a1-contracts::scan  (refuses mid-parse)
//! owned CAS write/read boundary    put / put_envelope / BackingStore
//! accounting session               dedup, declared-size charging, bounds
//! FD-1.8 storage integrity         both halves of an envelope-bearing artifact
//! ```
//!
//! It does **not** claim `ArtifactRef -> fully resolved typed A1 artifact`.
//! FD-1.5 orders resolution as *verify the stored object against the ref*, then
//! "if the slot expects a typed object: parse it under that slot's schema". The
//! first half is done for both classes; the second cannot be, because no §3
//! payload schema exists yet — step 1 built the envelope and stopped, so
//! `WorkOrderV1` and its ten siblings are as absent as `InteractionManifestV1`.
//!
//! Rather than publish a resolver that satisfies FD-1.8 while quietly not
//! satisfying FD-2.5, the proof types, the slot types and the resolution entry
//! points are crate-private. A partially discharged proof is a fine
//! intermediate value and a bad public capability.
//!
//! # Why the slots are not public either
//!
//! FD-2.5 fixes each slot's `kind` and media type "by the schemas in §3", so a
//! slot is the side of the comparison that is *not* untrusted. A public
//! `Slot::new(kind, media_type)` hands that to the call site. It looks harmless
//! today because only tests call it — and that is the whole shape of the
//! problem, because when schema-derived slots arrive there would be two routes
//! to one authority-relevant answer:
//!
//! ```text
//! parsed parent schema -> schema-derived slot -> resolve      strong
//! caller               -> Slot::new(...)      -> resolve      weak
//! ```
//!
//! and the weak one makes the strong one optional. Step 1 spent eight review
//! rounds on that exact shape. An intermediate state needed to build something
//! does not have to become a public state of the system.
//!
//! Not here at all: `immediate_refs`, the rank-edge rule, and closure
//! traversal. The boundary is built before the traversal that will run through
//! it on purpose — a traversal written first spends a while returning
//! unverified values, and the first test to cover it records that as the norm.

mod envelope;
mod resolved;
mod session;
mod store;

#[cfg(test)]
mod tests;

// The public surface of this slice, and deliberately no more.
//
// The CAS substrate and the accounting session are finished and useful on their
// own terms. Everything that *mints or requests a proof* — the slot types, the
// resolution entry points, and the proof-bearing values themselves — is
// crate-private until a consuming §3 schema can declare its own slots.
//
// That is not modesty about an unfinished feature. FD-2.5 fixes a slot's kind
// and media type "by the schemas in §3", so a caller-authored slot is authority
// handed to the call site. Publishing one now would mean that when
// schema-derived slots arrive there are two routes to the same
// authority-relevant answer, and the weaker one decides. Step 1 spent eight
// rounds learning that an intermediate state needed to build something does not
// have to become a public state of the system.
pub use session::{EffectiveLimits, ResolutionSession, ResolveError};
pub use store::{BackingStore, MemoryStore, PutError, RawObject};
