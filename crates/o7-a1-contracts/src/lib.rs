//! `o7-a1-contracts` — the A1 wire contract seed.
//!
//! This crate implements the **frozen** A1 authority contract. Its normative
//! source is not this code:
//!
//! ```text
//! contract   docs/q-deck/a1-authority-contracts.md
//! blob       3b26849cc39a3391aaed46cca56be3b6715afabb          (post-S1)
//!            sha256:1a0739752a5a2f7b34bcbc8f2d600615f823c76ad8c3a91d603c4921c848175d
//! ```
//!
//! The binding is to those **bytes**, not to a branch or a head — a distinction
//! twice demonstrated rather than merely asserted. First, `8ee8666` moved the
//! A1-F branch head without changing a byte of the contract, so "the latest
//! head" names an object that moves independently of what is implemented
//! (`docs/architecture/prior-art-the-grid.md` §1.3). Second, **S1 changed the
//! bytes without moving any protocol version** — §7.2 fires on payload shape,
//! envelope, rank or reducer semantics, and S1 touched none of them, so the blob
//! is the only thing that distinguishes the pre-S1 contract from this one.
//!
//! # What this crate is
//!
//! Step 1 of `docs/tasks/a1-v0.md`: the wire/domain seed. Frozen scalar types,
//! the closed kind enums, `ArtifactRef`, the common envelope and its framed
//! identity, the parse policy, and the bounds.
//!
//! # What this crate deliberately is not, yet
//!
//! No closure traversal, no campaign reducer, no provider calls, no human lane.
//! Those are later steps of the same task, and the reason to keep them out is
//! not tidiness: convenience is exactly where the first layer starts quietly
//! knowing about the fifth.
//!
//! Live-execution reconciliation and lifecycle management are outside A1-V0
//! altogether — prior art recorded in `docs/architecture/prior-art-the-grid.md`,
//! first eligible consumer `controller-lifecycle`.
//!
//! # The invariant that shapes every module here
//!
//! ```text
//! a model emitted something
//!   != the system holds it as a fact
//!   != the system is permitted to proceed
//! ```
//!
//! This crate serves the first arrow only. It decides whether bytes are
//! *admissible* — well-formed, bounded, versioned, identified. It never decides
//! that anything is true.

pub mod bounds;
pub mod envelope;
pub mod framing;
pub mod json;
pub mod kind;
pub mod refs;
pub mod scalars;

pub use bounds::{BudgetPolicy, BudgetPolicyError};
pub use envelope::{
    ArtifactRefs, EnvelopeError, EnvelopeV1, EnvelopeVersion, MessageKindVersion,
    CAMPAIGN_PROTOCOL_VERSION_V1, ENVELOPE_VERSION_V1, MESSAGE_KIND_VERSION_V1,
};
pub use framing::Preimage;
pub use json::{parse_artifact, ParseError, WireArtifact};
pub use kind::{ArtifactKindV1, MessageKindV1, ProducerRole};
pub use refs::{typed_media_type, ArtifactRef, RefError};
pub use scalars::{
    AdapterVersion, BoundedText, BoundedVec, CommitId, Digest256, FrozenVersion, Id, ModelIdentity,
    Optional, ScalarError, Text, Timestamp, WireDigest,
};
