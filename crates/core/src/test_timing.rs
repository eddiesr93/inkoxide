//! How a test's time grows with its input: a ratio between two sizes, not
//! a wall-clock limit, so a slow or busy machine does not fail it. Shared
//! by the unit tests and, through `tests/common`, the integration tests.

use std::time::Duration;

/// Noise in small timings: the larger run may take this much longer.
const NOISE: Duration = Duration::from_millis(500);

/// Four times `n` takes under eight times as long as `n`, where a
/// quadratic `run` would take sixteen. `run` returns how long the part
/// that grows with `n` took.
pub(crate) fn assert_linear(what: &str, n: usize, run: impl FnMut(usize) -> Duration) {
    assert_grows(what, n, 8, run);
}

/// Four times `n` takes under twice as long as `n`: `run` stops at a
/// limit, not at the end of its input.
pub(crate) fn assert_bounded(what: &str, n: usize, run: impl FnMut(usize) -> Duration) {
    assert_grows(what, n, 2, run);
}

fn assert_grows(what: &str, n: usize, most: u32, mut run: impl FnMut(usize) -> Duration) {
    let (small, big) = (run(n), run(4 * n));
    assert!(
        big < small * most + NOISE,
        "{what}: {n} took {small:?}, {} took {big:?}",
        4 * n
    );
}

/// `run` takes under eight times as long as `reference`: an input that
/// would grow past a limit stops at it, at the cost of an ordinary one.
pub(crate) fn assert_as_fast(
    what: &str,
    mut reference: impl FnMut() -> Duration,
    mut run: impl FnMut() -> Duration,
) {
    let (ordinary, took) = (reference(), run());
    assert!(
        took < ordinary * 8 + NOISE,
        "{what}: took {took:?}, an ordinary input {ordinary:?}"
    );
}
