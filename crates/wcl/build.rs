//! Build-time glue for the `wcl` binary.
//!
//! 1. Stamp the version reported by `wcl --version` into the binary.
//!    The workspace `Cargo.toml` carries a `0.0.0` sentinel: the release CI
//!    rewrites it to the real version (e.g. `0.20.0-alpha`) before building
//!    the published artifacts, so `CARGO_PKG_VERSION` is already correct
//!    there. For a *source* build (`cargo install --git …`, `cargo build`,
//!    the installer's from-source fallback on platforms without a prebuilt
//!    binary) the sentinel survives, and `wcl --version` would otherwise
//!    print `0.0.0`. In that case we derive a version from `git describe`,
//!    falling back to a `-dev` marker when git is unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Re-stamp when HEAD moves (best-effort; nothing is watched if it can't
    // be located). Resolve it rather than assuming `../../.git/HEAD`: in a git
    // WORKTREE `.git` is a *file* holding `gitdir: <path>`, so that literal
    // path names something that never exists — and cargo treats a missing
    // watched file as permanently dirty, re-running this script and relinking
    // the binary on EVERY cargo invocation (~3s each, well over a minute
    // across one merge-bar run, and a rebuild on every `cargo run` while
    // developing). The main checkout has a real `.git/` directory, so the cost
    // lands only on worktrees.
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    if let Some(head) = git_head_path(&manifest) {
        println!("cargo:rerun-if-changed={}", head.display());
    }

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

/// The repo's `HEAD` file, following the worktree/submodule indirection.
///
/// `<repo>/.git` is a directory in an ordinary checkout and a file reading
/// `gitdir: <path>` in a worktree; the latter's path may be relative to the
/// repo root. `None` when this isn't a checkout at all (an unpacked source
/// tarball, a vendored copy), in which case nothing is watched — better than
/// watching a path that will never appear.
fn git_head_path(manifest: &Path) -> Option<PathBuf> {
    let repo_root = manifest.join("../..");
    let dot_git = repo_root.join(".git");
    let git_dir = if std::fs::metadata(&dot_git).ok()?.is_dir() {
        dot_git
    } else {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = PathBuf::from(pointer.split_once("gitdir:")?.1.trim());
        if target.is_absolute() {
            target
        } else {
            repo_root.join(target)
        }
    };
    let head = git_dir.join("HEAD");
    head.is_file().then_some(head)
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
