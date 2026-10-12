//! Arbitrary bytes as an image, mostly to reach the SVG parser and
//! converter. Seed with `fixtures/images`.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = inkoxide_fuzz::check_image(data);
});
