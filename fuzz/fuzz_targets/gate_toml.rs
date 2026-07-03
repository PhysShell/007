#![no_main]
//! Fuzz the gate-manifest TOML parser (a per-target-repo config, only as trusted
//! as the target repo). Property: parsing arbitrary text never panics.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = o7::gate::GateManifest::parse(text);
    }
});
