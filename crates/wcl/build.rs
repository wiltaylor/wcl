//! Stamp the version reported by `wcl --version` into the binary.
//!
//! The workspace `Cargo.toml` carries a `0.0.0` sentinel: the release CI
//! rewrites it to the real version (e.g. `0.20.0-alpha`) before building
//! the published artifacts, so `CARGO_PKG_VERSION` is already correct
//! there. For a *source* build (`cargo install --git …`, `cargo build`,
//! the installer's from-source fallback on platforms without a prebuilt
//! binary) the sentinel survives, and `wcl --version` would otherwise
//! print `0.0.0`. In that case we derive a version from `git describe`,
//! falling back to a `-dev` marker when git is unavailable.

use std::process::Command;

fn main() {
    // Re-stamp when HEAD moves (best-effort; a missing path is ignored).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=build.rs");

    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version = if pkg != "0.0.0" {
        // CI stamped a real version into Cargo.toml — trust it (and avoid
        // shelling out to git, so release builds stay deterministic).
        pkg
    } else {
        git_describe().unwrap_or_else(|| format!("{pkg}-dev"))
    };

    println!("cargo:rustc-env=WCL_VERSION={version}");
}

/// `git describe --tags`, normalised to drop a leading `v`. `None` when
/// git is absent, this isn't a checkout, or no tags are reachable.
fn git_describe() -> Option<String> {
    let out = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty=-dirty"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let desc = String::from_utf8(out.stdout).ok()?;
    let desc = desc.trim();
    if desc.is_empty() {
        return None;
    }
    Some(desc.strip_prefix('v').unwrap_or(desc).to_string())
}
