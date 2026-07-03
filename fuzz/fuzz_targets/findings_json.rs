#![no_main]
//! Fuzz the untrusted-input deserializer for own-check findings.json. Property:
//! parsing arbitrary bytes never panics (malformed input must be a clean Err).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = o7::judge::parse_findings_json(data);
});
