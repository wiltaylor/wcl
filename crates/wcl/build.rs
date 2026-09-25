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

    // Re-stamp when the checked-out commit changes (best-effort; nothing is
    // watched if the repo can't be located). A commit on the current branch
    // leaves `HEAD` alone — it still reads `ref: refs/heads/main` — and moves
    // the ref it names, so the ref (loose file, or `packed-refs` after a
    // `git gc`) is watched too.
    //
    // Only paths that exist are ever named: cargo treats a missing watched
    // file as permanently dirty, re-running this script and relinking the
    // binary on EVERY cargo invocation (~3s each, well over a minute across
    // one merge-bar run). That is also why the git dir is resolved rather
    // than assumed to be `../../.git`: in a git WORKTREE `.git` is a *file*
    // holding `gitdir: <path>`.
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    for path in git_watch_paths(&manifest) {
        println!("cargo:rerun-if-changed={}", path.display());
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

/// The files whose change means `HEAD` points at a different commit.
///
/// A worktree keeps its own `HEAD` but shares refs with the main checkout,
/// through the directory its `commondir` file names.
///
/// Empty when this isn't a checkout at all (an unpacked source tarball, a
/// vendored copy) — better than watching a path that will never appear.
fn git_watch_paths(manifest: &Path) -> Vec<PathBuf> {
    let Some(git_dir) = git_dir(manifest) else {
        return Vec::new();
    };
    let head = git_dir.join("HEAD");
    if !head.is_file() {
        return Vec::new();
    }
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(rel) => git_dir.join(rel.trim()),
        Err(_) => git_dir.clone(),
    };
    let mut paths = Vec::new();
    // A detached HEAD holds a commit id and names no ref.
    if let Ok(contents) = std::fs::read_to_string(&head)
        && let Some(reference) = contents.trim().strip_prefix("ref:")
    {
        let loose = common_dir.join(reference.trim());
        if loose.is_file() {
            paths.push(loose);
        } else if let Some(parent) = loose.parent().filter(|p| p.is_dir()) {
            // The ref is packed (or not born yet). Watching its directory
            // catches the loose file git writes on the next commit.
            paths.push(parent.to_path_buf());
        }
    }
    let packed = common_dir.join("packed-refs");
    if packed.is_file() {
        paths.push(packed);
    }
    paths.push(head);
    paths
}

/// The repo's git directory, following the worktree/submodule indirection.
///
/// `<repo>/.git` is a directory in an ordinary checkout and a file reading
/// `gitdir: <path>` in a worktree; the latter's path may be relative to the
/// repo root.
fn git_dir(manifest: &Path) -> Option<PathBuf> {
    let repo_root = manifest.join("../..");
    let dot_git = repo_root.join(".git");
    if std::fs::metadata(&dot_git).ok()?.is_dir() {
        return Some(dot_git);
    }
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let target = PathBuf::from(pointer.split_once("gitdir:")?.1.trim());
    Some(if target.is_absolute() {
        target
    } else {
        repo_root.join(target)
    })
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
