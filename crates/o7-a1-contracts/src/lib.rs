//! `o7-a1-contracts` — the A1 wire contract seed.
//!
//! This crate implements the **frozen** A1 authority contract. Its normative
//! source is not this code:
//!
//! ```text
//! contract   docs/q-deck/a1-authority-contracts.md
//! blob       7db92f1b3dc9d7040da074956a0b3f2f200174c8
//!            sha256:9d26ee3ffbe5cb680075526833bdfef297372c6897b0f40afc6986cd0c7def45
//! ```
//!
//! The binding is to those **bytes**, not to a branch or a head. That is a
//! learned distinction rather than a stylistic one: while the contract was in
//! review, commit `8ee8666` moved the A1-F branch head without changing a byte
//! of the contract, so "the latest head" would have named an object that moves
//! independently of what is being implemented. See
//! `docs/architecture/prior-art-the-grid.md` §1.3.
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
    EnvelopeError, EnvelopeV1, EnvelopeVersion, MessageKindVersion, CAMPAIGN_PROTOCOL_VERSION_V1,
    ENVELOPE_VERSION_V1, MESSAGE_KIND_VERSION_V1,
};
pub use framing::Preimage;
pub use json::{parse_artifact, validate_document, ParseError, WireArtifact};
pub use kind::{ArtifactKindV1, MessageKindV1, ProducerRole};
pub use refs::{typed_media_type, ArtifactRef, RefError};
pub use scalars::{
    AdapterVersion, BoundedText, CommitId, Digest256, FrozenVersion, Id, ModelIdentity, Optional,
    ScalarError, Text, Timestamp, WireDigest,
};
