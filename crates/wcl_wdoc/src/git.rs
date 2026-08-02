//! Reading a document tree **at a git revision**.
//!
//! Anything that compares two versions of a document — `wcl diff`'s
//! `<rev>:<path>` side, `wcl wad spec`'s reviewed baseline, an audit of a
//! model at two revisions — needs the same thing: the whole tree at that
//! revision on disk, so imports, the wdoc registry and relative paths resolve
//! exactly like a real checkout with no special loader. [`materialize_rev`] extracts it into a temp dir
//! (`git archive | tar`); the caller then opens the file from there normally.
//!
//! We shell out to the `git` binary rather than add a git crate (the project
//! keeps its dependency list minimal). Errors are plain strings: a caller
//! renders them beside its own diagnostics, and there is nothing here worth
//! matching on.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

/// Run `git` with the given args in `dir`, returning trimmed stdout or a
/// human-readable error (distinguishing "git not found" from a non-zero
/// exit, whose stderr is surfaced).
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Resolve the repo root and the repo-relative form of `path`, without
/// touching the working tree (the file may exist only in the target
/// revision). For a relative path we add `git rev-parse --show-prefix` (the
/// cwd's offset within the repo) to it; for an absolute path we strip the
/// repo root.
pub fn repo_rel(path: &str) -> Result<(PathBuf, String), String> {
    let p = Path::new(path);
    let run_dir = if p.is_absolute() {
        p.parent().unwrap_or(Path::new("/")).to_path_buf()
    } else {
        PathBuf::from(".")
    };
    let root = PathBuf::from(git(&run_dir, &["rev-parse", "--show-toplevel"])?);
    if p.is_absolute() {
        let rel = p.strip_prefix(&root).map_err(|_| {
            format!(
                "path '{path}' is outside the git repo at {}",
                root.display()
            )
        })?;
        Ok((root, rel.to_string_lossy().replace('\\', "/")))
    } else {
        let prefix = git(&run_dir, &["rev-parse", "--show-prefix"])?;
        Ok((root, format!("{prefix}{path}")))
    }
}

/// Resolve `rev` to its full commit sha in the repo at `root` — the
/// immutable baseline a generated change-spec (or an audit) records.
pub fn resolve_rev(rev: &str, root: &Path) -> Result<String, String> {
    git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )
}

/// The commit where `a` and `b` diverged — what a `a...b` range means, and
/// the baseline for reviewing a branch: the state the branch started from,
/// not whatever the other branch has done since.
pub fn merge_base(a: &str, b: &str, root: &Path) -> Result<String, String> {
    git(root, &["merge-base", a, b])
}

/// Extract the whole tree at `rev` into a fresh temp dir via
/// `git archive <rev> | tar -x`. The returned `TempDir` cleans itself up on
/// drop, so the caller must hold it for as long as anything read from it is
/// still in use.
pub fn materialize_rev(rev: &str, root: &Path) -> Result<TempDir, String> {
    let tmp = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;

    let mut archive = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["archive", "--format=tar", rev])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run git archive: {e}"))?;

    let archive_out = archive
        .stdout
        .take()
        .ok_or_else(|| "git archive produced no stdout handle".to_string())?;
    let tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(tmp.path())
        .stdin(archive_out)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run tar (is it installed?): {e}"))?;

    let archive = archive
        .wait_with_output()
        .map_err(|e| format!("git archive failed: {e}"))?;
    if !archive.status.success() {
        return Err(format!(
            "git archive {rev}: {}",
            String::from_utf8_lossy(&archive.stderr).trim()
        ));
    }
    if !tar.status.success() {
        return Err(format!(
            "tar extract of revision '{rev}' failed: {}",
            String::from_utf8_lossy(&tar.stderr).trim()
        ));
    }
    Ok(tmp)
}
