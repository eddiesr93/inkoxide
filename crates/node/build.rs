use std::process::Command;

fn main() {
    napi_build::setup();

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
