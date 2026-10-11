//! Arbitrary bytes registered as a font (TrueType, OpenType, WOFF and
//! WOFF2) and used for a line of text. Seed with `fixtures/fonts`.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    inkoxide_fuzz::check_font(data);
});
