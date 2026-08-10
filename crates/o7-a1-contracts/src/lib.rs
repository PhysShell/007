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

mod bounds;
mod envelope;
mod framing;
mod json;
mod kind;
mod refs;
mod scalars;

pub use bounds::{
    BudgetPolicy, BudgetPolicyError, MAX_ARRAY_LEN, MAX_ARTIFACT_REFS, MAX_CONTROL_ARTIFACT_BYTES,
    MAX_DIRECT_REFERENCED_BYTES, MAX_DISPATCHES_PER_RECEIPT, MAX_EVIDENCE_BLOB_BYTES, MAX_FINDINGS,
    MAX_ID_BYTES, MAX_INTERACTION_SEQUENCE, MAX_JSON_DEPTH, MAX_REACHABLE_CLOSURE_BYTES,
    MAX_REACHABLE_CLOSURE_OBJECTS, MAX_REFS_PER_EXECUTION, MAX_STRING_BYTES,
    V0_DEFAULT_CLOSURE_OBJECT_BUDGET, V0_DEFAULT_EVIDENCE_BUDGET_BYTES,
};
pub use envelope::{
    ArtifactRefs, EnvelopeError, EnvelopeFieldsV1, EnvelopeV1, EnvelopeVersion, MessageKindVersion,
    CAMPAIGN_PROTOCOL_VERSION_V1, ENVELOPE_VERSION_V1, MESSAGE_KIND_VERSION_V1,
};
pub use json::{parse_artifact, ParseError, WireArtifact};
pub use kind::{ArtifactKindV1, MessageKindV1, ProducerRole};
pub use refs::{typed_media_type, ArtifactRef, RefError};
pub use scalars::{
    AdapterVersion, BoundedText, BoundedVec, CommitId, FrozenVersion, Id, ModelIdentity, Optional,
    ScalarError, Text, Timestamp, WireDigest, MAX_TIMESTAMP_BYTES,
};
