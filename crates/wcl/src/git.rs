//! Reading a document tree **at a git revision**, for `wcl diff`'s
//! `<rev>:<path>` side.
//!
//! Comparing against a revision needs the whole tree at that revision on
//! disk, so imports, the wdoc registry and relative paths resolve exactly like
//! a real checkout with no special loader. [`materialize_rev`] extracts it
//! into a temp dir (`git archive | tar`); the caller then opens the file from
//! there normally.
//!
//! We shell out to the `git` binary rather than add a git crate (the project
//! keeps its dependency list minimal). Errors are plain strings: the caller
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
pub(crate) fn repo_rel(path: &str) -> Result<(PathBuf, String), String> {
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

/// Extract the whole tree at `rev` into a fresh temp dir via
/// `git archive <rev> | tar -x`. The returned `TempDir` cleans itself up on
/// drop, so the caller must hold it for as long as anything read from it is
/// still in use.
///
/// `rev` comes straight from the command line, so it follows
/// `--end-of-options`: a revision spelled like an option (`--output=x`) is
/// looked up as a revision and fails, rather than being obeyed as a flag.
pub(crate) fn materialize_rev(rev: &str, root: &Path) -> Result<TempDir, String> {
    let tmp = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;

    let mut archive = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["archive", "--format=tar", "--end-of-options", rev])
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway repository with one file committed twice: `a.wcl` holds
    /// `x = 1` at `HEAD~1` and `x = 2` at `HEAD`. `None` when `git` is not
    /// installed, so the tests skip rather than fail on such a machine.
    fn repo() -> Option<TempDir> {
        if Command::new("git").arg("--version").output().is_err() {
            eprintln!("git is not installed; skipping");
            return None;
        }
        let dir = TempDir::new().expect("mkdir repo");
        // Identity and signing are pinned per command, so neither a missing
        // global identity nor a global `commit.gpgsign` changes the outcome.
        let run = |args: &[&str]| {
            let mut full = vec![
                "-c",
                "user.name=wcl test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ];
            full.extend_from_slice(args);
            git(dir.path(), &full).expect("git setup command");
        };
        run(&["init", "-q"]);
        // Windows runners set `core.autocrlf=true` globally, which makes
        // `git archive` write `\r\n`. Pin it off so the bytes asserted below
        // are the bytes committed.
        run(&["config", "core.autocrlf", "false"]);
        std::fs::write(dir.path().join("a.wcl"), "x = 1\n").expect("write a.wcl");
        run(&["add", "a.wcl"]);
        run(&["commit", "-q", "-m", "one"]);
        std::fs::write(dir.path().join("a.wcl"), "x = 2\n").expect("write a.wcl");
        run(&["commit", "-q", "-am", "two"]);
        Some(dir)
    }

    #[test]
    fn materialises_the_file_as_it_was_at_the_revision() {
        let Some(repo) = repo() else { return };
        let old = materialize_rev("HEAD~1", repo.path()).expect("materialise HEAD~1");
        assert_eq!(
            std::fs::read_to_string(old.path().join("a.wcl")).expect("read old a.wcl"),
            "x = 1\n"
        );
        let new = materialize_rev("HEAD", repo.path()).expect("materialise HEAD");
        assert_eq!(
            std::fs::read_to_string(new.path().join("a.wcl")).expect("read new a.wcl"),
            "x = 2\n"
        );
    }

    #[test]
    fn a_bad_revision_is_an_error_naming_it() {
        let Some(repo) = repo() else { return };
        let err = materialize_rev("no-such-branch", repo.path()).expect_err("bad revision");
        assert!(err.contains("no-such-branch"), "{err}");
    }

    #[test]
    fn an_option_shaped_revision_is_not_read_as_an_option() {
        let Some(repo) = repo() else { return };
        // Without `--end-of-options`, `git archive` takes this as its own
        // `--output` flag and creates `pwned.tar` in the repo.
        let err = materialize_rev("--output=pwned.tar", repo.path())
            .expect_err("an option-shaped revision must not resolve");
        assert!(err.contains("--output=pwned.tar"), "{err}");
        assert!(
            !repo.path().join("pwned.tar").exists(),
            "git obeyed the revision as a flag"
        );
    }

    // Unix only: on Windows the temp dir can be an 8.3 short path
    // (`RUNNER~1`) that git reports in long form, so the prefix comparison
    // would be testing the runner's profile path, not this function.
    #[cfg(unix)]
    #[test]
    fn repo_rel_strips_the_root_from_an_absolute_path() {
        let Some(repo) = repo() else { return };
        // Canonical, because the temp dir may sit behind a symlink (macOS's
        // `/var` → `/private/var`) while git reports the resolved root.
        let root = repo.path().canonicalize().expect("canonical repo root");
        let file = root.join("a.wcl");
        let (found_root, rel) = repo_rel(file.to_str().expect("utf-8 path")).expect("repo_rel");
        assert_eq!(rel, "a.wcl");
        assert_eq!(
            found_root.canonicalize().expect("canonical found root"),
            root
        );
    }
}
