#![no_main]
//! Fuzz the parser of the model's raw output — the least-trusted input the judge
//! handles. Property: never panics on arbitrary bytes, and any `Some` result is
//! bracket-delimited (the slice must land on char boundaries).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Some(arr) = o7::judge::extract_json_array(s) {
            assert!(arr.starts_with('[') && arr.ends_with(']'));
        }
    }
});
