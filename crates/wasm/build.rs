use std::process::Command;

/// The stack of the module. It is in linear memory, with no guard below it,
/// so overflowing it would overwrite memory instead of trapping; the
/// deepest document the engine accepts needs 2 MiB natively
/// (`crates/core/tests/limits.rs`), and the default is 1 MiB.
const STACK_SIZE: u32 = 8 << 20;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("wasm") {
        println!("cargo:rustc-link-arg-cdylib=-zstack-size={STACK_SIZE}");
    }
    // `version().gitSha`: the commit the binding was built from, or
    // "unknown" outside a git checkout (e.g. a source tarball).
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());
    println!("cargo:rustc-env=INKOXIDE_GIT_SHA={sha}");
    for path in [
        "../../.git/HEAD",
        "../../.git/refs/heads",
        "../../.git/packed-refs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
}
