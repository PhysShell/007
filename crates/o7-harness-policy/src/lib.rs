//! Repository policy for o7-worker's internal `test-harness` Cargo feature, plus a production-API
//! compile probe.
//!
//! Cargo features are NOT private. Any manifest can technically write
//! `o7-worker = { features = ["test-harness"] }`, forward it through a feature array, or pass
//! `--features test-harness` on the command line — resolver v2 only stops the self dev-dependency
//! from UNIFYING the feature into a normal build, it does not make the feature unrequestable. The
//! REPOSITORY POLICY is therefore what provides the guarantee: `test-harness` may be enabled from
//! exactly one dependency edge — o7-worker's own `[dev-dependencies]` self-edge — and nowhere else.
//! `tests/manifest_policy.rs` enforces that fail-closed by structurally parsing every workspace
//! manifest, and pins the production feature graph.
//!
//! This crate depends on o7-worker WITHOUT `test-harness`. In a plain `cargo build` (no test
//! targets, so no dev-dependency features are activated anywhere in the graph) that view is the
//! PRODUCTION o7-worker API. The [`probe_leak`] module — compiled only under the non-default
//! `probe-leak` feature — references the test-only surface, so `cargo build --features probe-leak`
//! MUST fail to compile; CI asserts that failure (and that it names the missing symbols). The
//! probe is deliberately BUILD-based, not a doctest: under `cargo test --workspace` resolver v2
//! unifies `test-harness` into o7-worker for every consumer, which would mask exactly the absence
//! the probe exists to prove.

/// The production `BackendConfig` is field-less: `Default` is its only constructor and there is no
/// `fake_mode` to populate. Compiles against the production API shape, so it is the positive
/// counterpart to [`probe_leak`] — the default build's API really is usable and harness-free.
#[must_use]
pub fn production_backend_config() -> o7_worker::BackendConfig {
    o7_worker::BackendConfig::default()
}

/// Compiled ONLY under the non-default `probe-leak` feature. It references the test-only surface
/// against the PRODUCTION o7-worker (a plain build activates no dev-dependency features), so it
/// MUST fail to compile. CI runs `cargo build -p o7-harness-policy --features probe-leak`, asserts
/// the build fails, and asserts the errors name `fake_mode` and `with_post_go_barrier` — so the
/// probe fails for the RIGHT reason (surface absent), never for an unrelated error. If this ever
/// compiles, the default production API grew a harness surface and the lock has been broken.
#[cfg(feature = "probe-leak")]
pub mod probe_leak {
    /// `BackendConfig::fake_mode` must not exist in the production API.
    #[must_use]
    pub fn references_fake_mode() -> o7_worker::BackendConfig {
        o7_worker::BackendConfig {
            fake_mode: Some(String::new()),
        }
    }

    /// `SandboyBoundary::with_post_go_barrier` must not exist in the production API.
    pub fn references_post_go_barrier() {
        let _leak = o7_worker::SandboyBoundary::with_post_go_barrier;
    }
}
