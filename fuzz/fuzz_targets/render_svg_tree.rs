//! Structure-aware SVG: the bytes drive an SVG generator that combines
//! what `usvg` expands (references, markers, patterns, style sheets,
//! entities, nested SVG images), so inputs probe the complexity limits.
//! A timeout or an out-of-memory is a finding: the limits let it through.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = inkoxide_fuzz::check_image(&inkoxide_fuzz::svg::svg(data));
});
