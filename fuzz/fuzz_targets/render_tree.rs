//! Structure-aware: the bytes drive a schema-shaped document generator,
//! so most inputs get past parsing into layout, pagination and painting.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| inkoxide_fuzz::check(&inkoxide_fuzz::document(data)));
