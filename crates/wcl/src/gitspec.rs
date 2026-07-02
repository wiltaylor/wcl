//! Git-revision inputs for `wcl diff`.
//!
//! A diff argument is either a plain working-tree path or a `<rev>:<path>`
//! specifier (e.g. `HEAD~1:config.wcl`, `main:a.wcl`). For a git spec we
//! **materialize the whole tree at that revision into a temp dir** (via
//! `git archive | tar`) and then open the file from there with the normal
//! disk loader — so imports, the wdoc registry, and relative paths resolve
//! exactly like a real checkout, with no special loader. We shell out to the
//! `git` binary rather than add a git crate (the project keeps its
//! dependency list minimal).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

/// A parsed diff input: either a working-tree path or a git revision + path.
#[derive(Debug, PartialEq)]
pub(crate) enum Spec {
    Working(PathBuf),
    Git { rev: String, path: String },
}

/// Classify a diff argument. A real file on disk always wins (so a literal
/// file is never mistaken for a revision); otherwise `<rev>:<path>` with
/// non-empty halves is a git spec, guarding against a bare Windows drive
/// letter (`C:\…`). Disambiguate a colon-named file with `./name`.
pub(crate) fn parse_spec(arg: &str) -> Spec {
    if Path::new(arg).exists() {
        return Spec::Working(PathBuf::from(arg));
    }
    if let Some((rev, path)) = arg.split_once(':')
        && !rev.is_empty()
        && !path.is_empty()
        && !is_windows_drive(rev)
    {
        return Spec::Git {
            rev: rev.to_string(),
            path: path.to_string(),
        };
    }
    Spec::Working(PathBuf::from(arg))
}

/// A single ASCII letter, i.e. a Windows drive prefix like `C` in `C:\…`.
fn is_windows_drive(rev: &str) -> bool {
    let mut chars = rev.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic()) && chars.next().is_none()
}

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

/// Resolve the repo root and the repo-relative path of a diff argument's
/// path, without touching the working tree (the file may exist only in the
/// target revision). For a relative path we add `git rev-parse
/// --show-prefix` (the cwd's offset within the repo) to it; for an absolute
/// path we strip the repo root.
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
/// drop, so the caller must hold it for as long as the opened document is
/// used.
pub(crate) fn materialize_rev(rev: &str, root: &Path) -> Result<TempDir, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_relative_path_is_working() {
        // A path that doesn't exist and has no colon stays a working spec.
        assert_eq!(
            parse_spec("nope.wcl"),
            Spec::Working(PathBuf::from("nope.wcl"))
        );
    }

    #[test]
    fn rev_path_is_a_git_spec() {
        assert_eq!(
            parse_spec("HEAD~1:config.wcl"),
            Spec::Git {
                rev: "HEAD~1".to_string(),
                path: "config.wcl".to_string()
            }
        );
        assert_eq!(
            parse_spec("main:docs/a.wcl"),
            Spec::Git {
                rev: "main".to_string(),
                path: "docs/a.wcl".to_string()
            }
        );
    }

    #[test]
    fn windows_drive_is_not_a_git_spec() {
        assert_eq!(
            parse_spec("C:\\tmp\\a.wcl"),
            Spec::Working(PathBuf::from("C:\\tmp\\a.wcl"))
        );
    }
}
