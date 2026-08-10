//! `o7-a1-cas` — A1-V0 step 2: the owned CAS read boundary and the resolution
//! proof boundary.
//!
//! Normative source, unchanged from step 1 and not derived from this code:
//!
//! ```text
//! contract   docs/q-deck/a1-authority-contracts.md
//! blob  B1   3b26849cc39a3391aaed46cca56be3b6715afabb          (post-S1)
//! step 1     a6625bc6473e3029a3309ddd7f2795ce57516a60          (PR #124)
//! ```
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
//!   ResolvedArtifact<'brand>                  evidence, bound to the session
//! ```
//!
//! [`store::RawObject`] sits deliberately outside that arrow: it is what a
//! backing store returned, trusted by nobody. A hostile fixture is another
//! implementation of [`BackingStore`], and it can return anything at all — which
//! is what makes the negative tests possible without a second production API.
//! Forge the input, never the verdict.
//!
//! # What is not here yet
//!
//! Typed and envelope-bearing resolution, `immediate_refs`, the rank-edge rule,
//! and closure traversal. Rank-0 opaque resolution is complete for its class
//! (FD-2.1 rank 0 is terminal; FD-2.5 forbids parsing or promoting such bytes),
//! and the boundary is built before the traversal that will run through it on
//! purpose: a traversal written first would spend a while returning unverified
//! values, and the first test to cover it would record that as the norm.

mod envelope;
mod resolved;
mod session;
mod store;

pub use envelope::ResolvedEnvelope;
pub use resolved::{ResolvedArtifact, ResolvedOpaque};
pub use session::{EffectiveLimits, EnvelopeSlot, OpaqueSlot, ResolutionSession, ResolveError};
pub use store::{BackingStore, MemoryStore, PutError, RawObject};
