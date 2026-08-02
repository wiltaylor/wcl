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
//!
//! 2. Build the `wcl editor` frontend (`editor-ui/` → `editor-ui/dist`,
//!    embedded by `src/editor/assets.rs`) with pnpm when its sources are
//!    newer than the dist. Set `WCL_EDITOR_UI_SKIP=1` to skip the frontend
//!    build entirely (a placeholder page is embedded instead) — the escape
//!    hatch for environments without node/pnpm.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

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

    build_editor_ui();
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

// ---------------------------------------------------------------------------
// editor-ui frontend
// ---------------------------------------------------------------------------

/// Ensure `editor-ui/dist` exists and is fresh. Behaviour matrix:
///
/// - `WCL_EDITOR_UI_SKIP=1`, or no `editor-ui/` sources at all → make sure a
///   placeholder dist exists (the rust-embed derive needs the folder at
///   compile time) and do nothing else.
/// - dist fresh → nothing.
/// - dist stale/missing + pnpm available → `pnpm install` (when
///   `node_modules` is missing) then `pnpm build`; a failure fails the
///   cargo build with pnpm's output.
/// - dist stale + pnpm missing → warn and embed the stale dist.
/// - dist missing + pnpm missing → fail the build, pointing at pnpm or the
///   `WCL_EDITOR_UI_SKIP=1` escape hatch.
fn build_editor_ui() {
    println!("cargo:rerun-if-env-changed=WCL_EDITOR_UI_SKIP");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    let ui_dir = manifest.join("../../editor-ui");
    let dist = ui_dir.join("dist");
    let dist_index = dist.join("index.html");

    // Watch the frontend sources (cargo watches directories recursively).
    // The dist is an *output* — watching it would loop the build.
    for rel in ["src", "index.html", "package.json", "vite.config.js"] {
        println!("cargo:rerun-if-changed={}", ui_dir.join(rel).display());
    }

    let skip = std::env::var("WCL_EDITOR_UI_SKIP").is_ok_and(|v| v == "1");
    let has_sources = ui_dir.join("package.json").is_file();
    if skip || !has_sources {
        if !dist_index.is_file() {
            write_placeholder(&dist);
        }
        if skip {
            println!("cargo:warning=WCL_EDITOR_UI_SKIP=1 — embedding the existing editor UI as-is");
        }
        return;
    }

    let dist_mtime = mtime(&dist_index);
    let src_mtime = ["src", "index.html", "package.json", "vite.config.js"]
        .iter()
        .filter_map(|rel| newest_mtime(&ui_dir.join(rel)))
        .max();
    let fresh = matches!((dist_mtime, src_mtime), (Some(d), Some(s)) if d >= s);
    if fresh {
        return;
    }

    if !pnpm_available() {
        if dist_index.is_file() {
            println!(
                "cargo:warning=editor-ui is stale but pnpm was not found; \
                 embedding the existing build"
            );
            return;
        }
        panic!(
            "the `wcl editor` frontend (editor-ui/) has no built dist and pnpm is not \
             installed.\nInstall pnpm (https://pnpm.io) and rebuild, or set \
             WCL_EDITOR_UI_SKIP=1 to embed a placeholder page instead."
        );
    }

    if !ui_dir.join("node_modules").is_dir() {
        run_pnpm(&ui_dir, &["install"]);
    }
    run_pnpm(&ui_dir, &["build"]);
    if !dist_index.is_file() {
        panic!("pnpm build completed but editor-ui/dist/index.html is missing");
    }
}

/// Run pnpm in `ui_dir`, failing the cargo build on error. Windows resolves
/// pnpm through `cmd` (it ships as a `.cmd` shim).
fn run_pnpm(ui_dir: &Path, args: &[&str]) {
    let mut cmd = pnpm_command();
    cmd.args(args).current_dir(ui_dir);
    println!("cargo:warning=editor-ui: running pnpm {}", args.join(" "));
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run pnpm {}: {e}", args.join(" ")));
    if !out.status.success() {
        panic!(
            "pnpm {} failed in {}:\n{}\n{}",
            args.join(" "),
            ui_dir.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

fn pnpm_command() -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "pnpm"]);
        c
    } else {
        Command::new("pnpm")
    }
}

fn pnpm_available() -> bool {
    pnpm_command()
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The newest modification time under `path` (a file, or a directory walked
/// recursively). `None` when it doesn't exist.
fn newest_mtime(path: &Path) -> Option<SystemTime> {
    if path.is_file() {
        return mtime(path);
    }
    let mut newest = None;
    for entry in std::fs::read_dir(path).ok()?.flatten() {
        let child = newest_mtime(&entry.path());
        newest = newest.max(child);
    }
    newest
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A minimal dist so the rust-embed derive compiles when the real frontend
/// was never built.
fn write_placeholder(dist: &Path) {
    std::fs::create_dir_all(dist).expect("create editor-ui/dist");
    std::fs::write(
        dist.join("index.html"),
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>wcl editor</title></head>\
         <body><h1>wcl editor</h1><p>The editor UI was not built into this binary. \
         Install pnpm and rebuild <code>wcl</code> without <code>WCL_EDITOR_UI_SKIP</code>.</p>\
         </body></html>",
    )
    .expect("write placeholder editor-ui/dist/index.html");
}
