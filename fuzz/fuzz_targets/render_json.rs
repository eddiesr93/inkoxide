//! Arbitrary bytes as a JSON document. Seed with `fixtures/docs`.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| inkoxide_fuzz::check(data));
