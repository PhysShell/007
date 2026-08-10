//! Envelope-bearing resolution: FD-1.8's **two** byte strings, both checked.
//!
//! # What a ref to an envelope-bearing artifact actually names
//!
//! FD-1.8 is explicit, and it is the rule most easily half-implemented:
//!
//! ```text
//! digest = the envelope digest (FD-1.2), which commits to payload_digest
//! size   = envelope_bytes + payload_bytes
//! ```
//!
//! So one reference names two stored objects, and "verify the object against
//! the ref" is two verifications plus an arithmetic one. Checking the payload
//! digest and stopping — the obvious half — leaves the envelope unverified
//! while the value looks resolved, and the size check silently passes because
//! nobody added the halves.
//!
//! Note the consequence for the store: an envelope is keyed by its **framed**
//! digest (FD-1.2), not by the hash of its bytes, because that is what a
//! reference names. A content-addressed `put` is therefore the write path for
//! the payload and *not* for the envelope; [`crate::MemoryStore::put_envelope`]
//! is, and it encodes through the step-1 checked producer encoder rather than
//! inventing a second one.
//!
//! # The kind is not the sender's to choose
//!
//! FD-2.5: "`ArtifactRef.kind` is a claim by whoever wrote the reference." For
//! a typed object that claim selects a *parser*, so accepting it unchecked
//! would let a peer have bytes admitted under one schema and returned as
//! authority under another kind. Three things must agree before anything is
//! returned, and they are checked as one operation:
//!
//! ```text
//! the slot's expectation        what the consuming schema declared (FD-2.5)
//! the reference's claim         kind and media type, from untrusted input
//! the bytes themselves          the message_kind inside the admitted envelope
//! ```
//!
//! # No typed shortcut
//!
//! The envelope bytes go through [`o7_a1_contracts::parse_artifact`] — the same
//! single admission path a peer's bytes take, structural scan and typed schema
//! and cross-field rules included. There is deliberately no
//! `RawObject -> serde -> looks typed` route here: this crate holds no second
//! opinion about what an envelope is.

use o7_a1_contracts::{parse_artifact, ArtifactKindV1, EnvelopeV1, WireDigest};

use std::marker::PhantomData;

/// An envelope-bearing artifact this session resolved: both halves read, both
/// verified against the reference, and the whole charged for.
///
/// Distinct from [`crate::ResolvedOpaque`] on purpose. The two are unified by
/// [`crate::ResolvedArtifact`] for the things that are genuinely common —
/// identity, size — and by nothing else, because an opaque resolution proves
/// bytes hash to a digest while this proves an admitted envelope frames to one
/// and binds the payload beneath it. A single struct with optional fields would
/// let a caller ask an opaque resolution for its envelope and get `None`, which
/// reads as "no envelope" rather than "wrong question".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvelope<'brand> {
    envelope: EnvelopeV1,
    framed_digest: WireDigest,
    payload: Vec<u8>,
    envelope_bytes: u64,
    brand: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> ResolvedEnvelope<'brand> {
    pub(crate) fn mint(
        envelope: EnvelopeV1,
        framed_digest: WireDigest,
        payload: Vec<u8>,
        envelope_bytes: u64,
    ) -> Self {
        Self {
            envelope,
            framed_digest,
            payload,
            envelope_bytes,
            brand: PhantomData,
        }
    }

    /// The admitted envelope. It went through `parse_artifact`, so it carries
    /// step 1's guarantees unchanged rather than a re-derived subset.
    #[must_use]
    pub fn envelope(&self) -> &EnvelopeV1 {
        &self.envelope
    }

    /// FD-1.2 — the framed identity, verified equal to the reference's digest.
    #[must_use]
    pub fn framed_digest(&self) -> &WireDigest {
        &self.framed_digest
    }

    /// The stored payload bytes, verified against the envelope's
    /// `payload_digest` (FD-1.1).
    ///
    /// Still bytes: FD-1.5 parses a payload only when a slot expects a typed
    /// object, and what that payload *means* is the consuming schema's
    /// question, not this boundary's.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// FD-1.8 — the two halves, which resolution proved sum to the declared
    /// `size`.
    #[must_use]
    pub fn stored_size(&self) -> u64 {
        self.envelope_bytes
            .saturating_add(self.payload.len() as u64)
    }

    /// FD-1.5 — `(kind, digest)`, the identity a closure deduplicates by.
    #[must_use]
    pub fn identity(&self) -> (ArtifactKindV1, &str) {
        (
            self.envelope.message_kind().artifact_kind(),
            self.framed_digest.as_str(),
        )
    }
}

/// Admit an envelope from stored bytes through the crate that owns the schema.
///
/// A free function rather than a method so that the only thing this module can
/// do with envelope bytes is the thing step 1 already defined.
pub(crate) fn admit_envelope(bytes: &[u8]) -> Result<EnvelopeV1, o7_a1_contracts::ParseError> {
    parse_artifact::<EnvelopeV1>(bytes)
}
