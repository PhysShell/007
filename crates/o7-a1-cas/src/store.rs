//! The owned CAS read boundary, and the one write path that admits bytes to it.
//!
//! # Why the read trait returns an untrusted type
//!
//! A backing store is not trusted. It can be a disk that lost a byte, a peer
//! that wrote the wrong object under a digest, or — in a test — a deliberately
//! hostile implementation built to feed the resolver garbage. So
//! [`BackingStore::get`] returns [`RawObject`]: bytes, and *only* bytes, with no
//! claim attached about whether they match anything.
//!
//! This is what lets the negative tests exist without a parallel production API.
//! A test that needs a digest mismatch implements this trait and returns
//! whatever it likes; what it cannot do is return a
//! [`crate::ResolvedArtifact`], because nothing in this module can produce one.
//! Forge the input, never the verdict.

use std::collections::HashMap;

use o7_a1_contracts::{EnvelopeError, EnvelopeV1, WireDigest};

/// Bytes as a backing store handed them over: **untrusted**.
///
/// Deliberately constructible by anyone from anything. It carries no claim, so
/// there is nothing to forge — a `RawObject` asserts only "some store returned
/// these bytes", which is exactly what an unverified read is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawObject(Vec<u8>);

impl RawObject {
    /// Wrap bytes a store returned. Available to hostile fixtures on purpose.
    #[must_use]
    pub fn from_backing_store(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The bytes, still unverified.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// The number of bytes actually stored, as opposed to the number an
    /// `ArtifactRef` *declared*. FD-1.5 accounts the declared size before any
    /// read; FD-1.8 then requires the two to agree, and a disagreement is an
    /// integrity failure rather than a rounding error.
    #[must_use]
    pub fn stored_size(&self) -> u64 {
        self.0.len() as u64
    }
}

/// Read access to 007-owned content-addressed storage.
///
/// One method, returning an untrusted type. Everything that turns bytes into
/// something an authority decision may rest on lives behind
/// [`crate::ResolutionSession`].
pub trait BackingStore {
    /// The object stored under `digest`, or `None` if this store has no such
    /// object. A store that returns bytes not hashing to `digest` is not
    /// breaking this contract — it is exercising the case the resolver exists
    /// to catch.
    fn get(&self, digest: &WireDigest) -> Option<RawObject>;
}

/// An in-memory owned CAS.
///
/// The **production write path** is [`Self::put`], and it computes the digest
/// from the bytes rather than accepting one. There is deliberately no
/// `insert(digest, bytes)`: a store that lets a caller choose the key is a store
/// where "content-addressed" is a naming convention. A test that needs that
/// behaviour implements [`BackingStore`] itself and stays on the untrusted side
/// of the boundary.
#[derive(Debug, Default)]
pub struct MemoryStore {
    objects: HashMap<String, Vec<u8>>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit bytes to the store, returning the digest they hash to.
    ///
    /// FD-1.1 names `Digest256::of_bytes` as *the* form for A1 digests, and
    /// this is the only way an object enters this store, so no object in it
    /// ever sits under a key nobody computed.
    pub fn put(&mut self, bytes: Vec<u8>) -> WireDigest {
        let digest = WireDigest::of_bytes(&bytes);
        self.objects.insert(digest.as_str().to_owned(), bytes);
        digest
    }

    /// Admit an envelope-bearing artifact: both halves, each under the key a
    /// reference will name.
    ///
    /// The asymmetry is FD-1.8's, not a convenience. A payload is keyed by its
    /// content digest; an **envelope is keyed by its framed digest** (FD-1.2),
    /// because that is what `ArtifactRef.digest` means for this class. A
    /// content-addressed `put` is therefore the wrong write path for an
    /// envelope, and offering only that would have quietly made every
    /// envelope-bearing reference unresolvable.
    ///
    /// The bytes come from [`o7_a1_contracts::EnvelopeV1::to_wire_bytes`] —
    /// step 1's single checked producer encoder. This crate does not get a
    /// second opinion about how an envelope is spelled.
    ///
    /// Returns the reference this artifact must be named by: its framed digest
    /// and the size of both halves together.
    ///
    /// # Errors
    /// [`o7_a1_contracts::EnvelopeError`] if the envelope cannot be encoded
    /// under its own FD-1.4 ceiling.
    pub fn put_envelope(
        &mut self,
        envelope: &EnvelopeV1,
        payload: Vec<u8>,
    ) -> Result<(WireDigest, u64), EnvelopeError> {
        let envelope_bytes = envelope.to_wire_bytes()?;
        let framed = envelope.framed_digest();
        let size = EnvelopeV1::ref_size(envelope_bytes.len() as u64, payload.len() as u64);
        self.objects
            .insert(envelope.payload_digest().as_str().to_owned(), payload);
        self.objects
            .insert(framed.as_str().to_owned(), envelope_bytes);
        Ok((framed, size))
    }

    /// How many objects this store holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

impl BackingStore for MemoryStore {
    fn get(&self, digest: &WireDigest) -> Option<RawObject> {
        self.objects
            .get(digest.as_str())
            .map(|bytes| RawObject::from_backing_store(bytes.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_write_path_computes_the_key_it_stores_under() {
        let mut store = MemoryStore::new();
        let digest = store.put(b"evidence".to_vec());
        assert_eq!(digest, WireDigest::of_bytes(b"evidence"));
        assert_eq!(
            store.get(&digest).map(|o| o.bytes().to_vec()),
            Some(b"evidence".to_vec())
        );
    }

    #[test]
    fn an_absent_object_is_absent_rather_than_empty() {
        // A missing object and a zero-byte object are different facts, and
        // FD-1.8 admits refs to the latter — so they must not collapse.
        let mut store = MemoryStore::new();
        let empty = store.put(Vec::new());
        assert_eq!(store.get(&empty).map(|o| o.stored_size()), Some(0));
        assert_eq!(store.get(&WireDigest::of_bytes(b"never stored")), None);
    }
}
