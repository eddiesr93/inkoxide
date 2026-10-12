//! Files from the repository's `fixtures/` for the unit tests.

use crate::FontStore;

/// The bytes of `fixtures/<path>`.
pub(crate) fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// A font store with each `fixtures/fonts/<name>.ttf` registered as
/// `<name>`.
pub(crate) fn fonts(names: &[&str]) -> FontStore {
    let mut store = FontStore::new();
    for name in names {
        store
            .register(*name, fixture(&format!("fonts/{name}.ttf")))
            .unwrap();
    }
    store
}

/// A font store with Inter as `Inter-Regular`, the font most tests use.
pub(crate) fn inter() -> FontStore {
    fonts(&["Inter-Regular"])
}
