//! `o7-sandbox-protocol` — the wire contract between the o7-worker parent and an EXTERNAL
//! `sandboy` backend.
//!
//! It is PURE: no I/O, no `unsafe`, no process launching. Both sides recompute truth from
//! bytes with only `serde` + a hash, so the backend never has to depend on the worker
//! runtime and the worker never has to trust an unverified report. The pieces:
//!
//! - [`SandboxPolicy`] — the confinement request, hashable to a canonical [`Digest256`].
//! - [`SandboxReport`] — what the backend actually enforced, per dimension (never a boolean
//!   `secure`), BOUND to the policy/launch/target via `policy_digest`, `launch_nonce`, and
//!   `target_digest`.
//! - [`frame`] — a single length-prefixed, size-bounded report frame with a defined end.
//!
//! o7-run stores the raw report frame as an opaque, content-addressed artifact; it does not
//! need these structured types.

pub mod frame;
pub mod ids;
pub mod policy;
pub mod report;

pub use frame::{FrameError, MAX_REPORT_BYTES};
pub use ids::{BackendIdentity, Digest256, IdError, LaunchNonce};
pub use policy::{
    Enforcement, NetworkPolicy, SandboxDimension, SandboxPolicy, SandboxPolicyError, MAX_TIMEOUT,
    MIN_TIMEOUT,
};
pub use report::{ReportError, SandboxReport};

/// The wire-format version. A report whose `schema_version` differs is rejected rather than
/// coerced — a version bump is a deliberate, gated change.
pub const SCHEMA_VERSION: u32 = 1;
