//! The `include` block + `included_sites` builtin: build other wdoc
//! documents found under a folder and ship each one's output into a
//! subdirectory of this build.
//!
//! Unlike `import` (which merges another file's blocks into this
//! document), an included document is built independently — exactly as if
//! `wcl wdoc build` had been run on it — and its self-contained output
//! tree is copied under a subdirectory of this build. The block names a
//! folder; the build walks the folder's **subdirectories** (files sitting
//! directly in the folder are skipped) and treats every file whose name
//! matches a glob as a sub-site entry point. Each entry builds into
//! `<folder-basename>/<entry's parent folder, relative to folder>/`.
//!
//! [`resolve_included`] is the single source of truth shared by the build
//! pipeline (which recurses [`crate::build`] per entry) and the
//! `included_sites` builtin (which exposes the same `{ name, href }` list
//! to WCL for navigation), so the built output paths and the nav hrefs
//! cannot drift.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::build::BuildError;

/// One discovered sub-site entry point.
pub(crate) struct IncludedSite {
    /// The entry's parent folder relative to the scanned folder, with
    /// `/` separators (e.g. `foo` or `foo/bar`). Used as the nav name.
    pub name: String,
    /// The sub-site's root-relative URL (`<prefix>/<name>/`).
    pub href: String,
    /// The entry `.wcl` file on disk to build.
    pub src_path: PathBuf,
    /// Output subdirectory relative to the build root (`<prefix>/<name>`).
    pub out_subdir: PathBuf,
}

/// Match `name` against a tiny filename glob: `*` matches any run of
/// characters (including empty), `?` matches exactly one character;
/// everything else is literal. Operates on bytes — sufficient for ASCII
/// filenames like `main.wcl` / `*.skill.wcl`. Richer path globs would
/// want the `globset` crate; this stays dependency-free.
pub(crate) fn glob_match(pattern: &str, name: &str) -> bool {
    let p = pattern.as_bytes();
    let n = name.as_bytes();
    // Classic two-pointer wildcard match with backtracking on `*`.
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Collect every regular file under `root` (recursively), in a
/// deterministic (sorted) order.
fn walk_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), BuildError> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .map_err(|e| BuildError::Io(e, format!("read_dir {}", root.display())))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Resolve the sub-sites declared by an `include "<folder>" { pattern }`.
///
/// `folder` resolves against `base_dir`; its subdirectories are walked and
/// every file whose name matches `pattern` becomes a sub-site whose name is
/// its parent folder relative to `folder`. The output prefix is the
/// folder's basename. Errors when the folder is missing or two matches
/// resolve to the same output subdirectory.
pub(crate) fn resolve_included(
    base_dir: Option<&Path>,
    folder: &str,
    pattern: &str,
) -> Result<Vec<IncludedSite>, BuildError> {
    let root = match base_dir {
        Some(b) => b.join(folder),
        None => PathBuf::from(folder),
    };
    if !root.is_dir() {
        return Err(BuildError::BadPage(format!(
            "include folder '{}' not found (or not a directory)",
            root.display()
        )));
    }
    let prefix = Path::new(folder)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("include")
        .to_string();

    let mut files = Vec::new();
    walk_files(&root, &mut files)?;

    let mut out: Vec<IncludedSite> = Vec::new();
    // Two distinct entry files resolving to the same output subdir (two
    // matches in one folder) is a build error — no silent overwrite.
    let mut by_subdir: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    for file in files {
        let fname = file.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !glob_match(pattern, fname) {
            continue;
        }
        let parent = file.parent().unwrap_or(&root);
        // Skip files sitting directly in the scanned folder (depth 0); a
        // sub-site lives in its own subdirectory.
        let Ok(rel) = parent.strip_prefix(&root) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let out_subdir = Path::new(&prefix).join(rel);
        if let Some(prev) = by_subdir.get(&out_subdir)
            && prev != &file
        {
            return Err(BuildError::BadPage(format!(
                "include: two entry files map to the same output '{}' \
                 ('{}' and '{}') — only one match per subdirectory",
                out_subdir.display(),
                prev.display(),
                file.display()
            )));
        }
        by_subdir.insert(out_subdir.clone(), file.clone());
        out.push(IncludedSite {
            href: format!("{prefix}/{name}/"),
            name,
            src_path: file,
            out_subdir,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_literal_and_wildcards() {
        assert!(glob_match("main.wcl", "main.wcl"));
        assert!(!glob_match("main.wcl", "other.wcl"));
        assert!(glob_match("*.wcl", "main.wcl"));
        assert!(glob_match("*.skill.wcl", "foo.skill.wcl"));
        assert!(!glob_match("*.skill.wcl", "foo.wcl"));
        assert!(glob_match("*", "anything.wcl"));
        assert!(glob_match("m?in.wcl", "main.wcl")); // '?' matches one char
        assert!(!glob_match("m?in.wcl", "min.wcl")); // '?' is not optional
        assert!(glob_match("a*b*c", "axxbxxc"));
        assert!(!glob_match("a*b*c", "axxbxx"));
    }

    #[test]
    fn missing_folder_errors() {
        let r = resolve_included(Some(Path::new("/nonexistent-wdoc-root")), "nope", "*.wcl");
        assert!(r.is_err());
    }
}
