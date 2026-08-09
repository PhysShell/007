//! # o7-a1-protocol — the A1 contract's type layer
//!
//! Normative source: `docs/q-deck/a1-contracts.md` (NORMATIVE DRAFT,
//! APPROVED FOR TYPES at `f291cfc`; not yet FROZEN). This crate is the
//! §14 "A1 owns now" protocol/library layer: schemas, registries, the
//! canonical-bytes writer, typed refs, and checked constructors. It
//! contains NO campaign runtime, NO production storage authority, and NO
//! provider adapters — those arrive with A2 and later slices.
//!
//! Layering follows the contract's §15 split exactly:
//! 1. wire-unrepresentable — closed enums, tagged producer variants,
//!    per-outcome payload shapes, `deny_unknown_fields` everywhere;
//! 2. construction-API-unrepresentable — closed fields plus checked
//!    constructors that demand cross-object PROOF inputs, so "two
//!    independently valid refs standing together" cannot be built;
//! 3. the RED matrix (contract §15.3) lands as its own slice after the
//!    types-surface review — deliberately not smuggled in here.
//!
//! ## Slice accounting (honest, not aspirational)
//!
//! This first slice lands the foundation the rest of the catalogue
//! depends on: identities and `BlobDigest`; `ByteStringV1`;
//! `CanonicalJsonV1` (writer version + known-answer corpus); the
//! domain-separated digest-context registry; the frozen v1 limits
//! profile; the closed CAS `ContentKind` registry, `CasObjectRefV1`,
//! and the store INTERFACE with a test-only in-memory implementation;
//! the closed edge-tag registry with the §11.3 allowed-edge matrix and
//! the machine-checked topological sort of its `intra` subgraph; and
//! the envelope core (`CausationV1`, `ProducerBindingV1`,
//! `RecordedMetadataV1`, `RefManifest`).
//!
//! Landed in the wrappers slice (APPROVED FOR NEXT SLICE @ ebc9b8f):
//! the class-3 external wrappers with their field-level checked
//! constructors; `InputStateBindingV1`; `CampaignRunBindingV1` +
//! `CampaignRunBindingRefV1` (the antecedent authority for execution
//! SafeRedrive); the §8.6 cause vocabulary with the T5-R1 cross-checks
//! (dispatch-level SafeRedrive wire-unrepresentable).
//!
//! NOT landed yet (next slices, in dependency order): the resolver /
//! proof layer (which alone mints `Accepted…` forms and replaces the
//! interim `ResolvedCausation`-style proof inputs); the provider
//! receipt/manifest payloads; gate registry types; the role artifact
//! payloads; the human/attention lane; the ABSENT→RESERVED→COMMITTED
//! idempotency machine and `CanonicalConstructionSeedV1`. Every one of
//! those is contract-frozen in `a1-contracts.md` and none is redesigned
//! here.
//!
//! OPEN mini-adjudication flagged for the next review (not silently
//! decided): the `ContentKind` a `CasObjectRefV1` carries when its
//! target is a COMMITTED ENVELOPE-BEARING ARTIFACT (causation
//! `blob_ref`, `CampaignRunBindingRefV1::blob_ref`,
//! `invocation_receipt_ref`, `HumanCommandRequestRefV1.blob_ref`). The
//! closed class-2 registry has no canonical-message kind; this slice
//! provisionally uses `message-payload-blob` for the binding ref and
//! records the question instead of minting a registry entry (the T5
//! lesson: implementations do not ratify contract changes silently).

pub mod binding;
pub mod bytes;
pub mod canonical;
pub mod cas;
pub mod digest;
pub mod edges;
pub mod envelope;
pub mod ids;
pub mod limits;
pub mod wrappers;
