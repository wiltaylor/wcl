//! The `include` block + `included_sites` builtin: build other wdoc
//! documents found under a folder and ship each one's output into a
//! subdirectory of this build.
//!
//! Unlike `import` (which merges another file's blocks into this
//! document), an included document is built independently — exactly as if
//! `wcl wdoc build` / `wcl wdoc skill` had been run on it — and its
//! self-contained output tree is copied under a subdirectory of this build.
//!
//! Two discovery modes, exactly one per `include`:
//! - **`pattern`** — a filename glob matched recursively against every file
//!   in the folder's subdirectories. The sub-site name is the matching
//!   file's parent folder relative to the scanned folder. Good for a flat
//!   folder of single-file sites.
//! - **`entry`** — a fixed relative path checked inside each *immediate*
//!   subdirectory of the folder (no recursion). Each subdirectory that has
//!   the entry file is a sub-site named after that subdirectory. Rendered
//!   `out/` trees, `_wdoc/`, and bundled subtrees are never scanned.
//!
//! An optional `site` selector picks which named site of a (multi-site)
//! member to build, threaded as the recursive build's `site_filter`.
//!
//! [`resolve_included`] is the single source of truth shared by the build
//! and skill pipelines (which recurse per entry) and the `included_sites`
//! builtin (which exposes a `{ name, href, title, summary }` list to WCL for
//! navigation), so built output paths and nav hrefs cannot drift.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use wcl_lang::{Block, Document, disk_loader};

use crate::build::BuildError;
use crate::render::{field_bool, field_utf8, label_string};

/// Maximum depth of nested `include` builds, a backstop against an
/// `include` chain that never terminates (deeper than any real project).
pub(crate) const MAX_INCLUDE_DEPTH: usize = 8;

/// The resolved options of one `include` block (or one `included_sites(...)`
/// call). Exactly one of `pattern` / `entry` must be set.
pub(crate) struct IncludeSpec {
    pub folder: String,
    pub pattern: Option<String>,
    pub entry: Option<String>,
    /// The named site of each member to build (`--site`), if narrowing.
    pub site: Option<String>,
    /// Output prefix override (`<prefix>/<name>/`); defaults to the folder
    /// basename. Lets two `include`s over the same folder (different entries,
    /// e.g. a member's book and its deck) ship to distinct subdirectories.
    pub prefix: Option<String>,
}

/// One discovered sub-site entry point.
pub(crate) struct IncludedSite {
    /// The sub-site name: in `entry` mode the immediate subdirectory; in
    /// `pattern` mode the matching file's parent folder relative to the
    /// scanned folder (with `/` separators). Drives the nav name + output.
    pub name: String,
    /// The sub-site's root-relative URL (`<prefix>/<name>/`).
    pub href: String,
    /// The entry `.wcl` file on disk to build.
    pub src_path: PathBuf,
    /// The sub-site's source directory on disk (`<folder>/<name>`); the page's
    /// source files all live under it. Used to scope a page's rebuild.
    pub src_root: PathBuf,
    /// Output subdirectory relative to the build root (`<prefix>/<name>`).
    pub out_subdir: PathBuf,
    /// The site to build for this member (the include's `site` selector),
    /// passed as the recursive build's `site_filter`.
    pub site: Option<String>,
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

/// The folder basename, used as the output prefix (`<prefix>/<name>/`).
fn prefix_of(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("include")
        .to_string()
}

/// `pattern` mode: recurse `root`, match the glob against each file's
/// basename, name each match by its parent folder relative to `root`.
/// Returns `(name, src_path)` pairs.
fn pattern_entries(root: &Path, pattern: &str) -> Result<Vec<(String, PathBuf)>, BuildError> {
    let mut files = Vec::new();
    walk_files(root, &mut files)?;
    let mut out = Vec::new();
    for file in files {
        let fname = file.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !glob_match(pattern, fname) {
            continue;
        }
        let parent = file.parent().unwrap_or(root);
        // Skip files sitting directly in the scanned folder (depth 0); a
        // sub-site lives in its own subdirectory.
        let Ok(rel) = parent.strip_prefix(root) else {
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
        out.push((name, file));
    }
    Ok(out)
}

/// `entry` mode: read only the immediate subdirectories of `root`; for each
/// `<sub>`, if `<root>/<sub>/<entry>` is a file it's a sub-site named
/// `<sub>`. No recursion — `out/` trees and bundled subtrees are never
/// scanned. Returns `(name, src_path)` pairs, sorted by name.
fn entry_entries(root: &Path, entry: &str) -> Result<Vec<(String, PathBuf)>, BuildError> {
    let mut subdirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map_err(|e| BuildError::Io(e, format!("read_dir {}", root.display())))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();
    let mut out = Vec::new();
    for dir in subdirs {
        let Some(name) = dir.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let candidate = dir.join(entry);
        if candidate.is_file() {
            out.push((name, candidate));
        }
    }
    Ok(out)
}

/// Resolve the sub-sites declared by an `include` / `included_sites(...)`.
///
/// `folder` resolves against `base_dir`; the output prefix is its basename.
/// Branches on `pattern` vs `entry` (exactly one required). Errors when the
/// folder is missing, when neither/both modes are set, or when two matches
/// resolve to the same output subdirectory.
pub(crate) fn resolve_included(
    base_dir: Option<&Path>,
    spec: &IncludeSpec,
) -> Result<Vec<IncludedSite>, BuildError> {
    let root = match base_dir {
        Some(b) => b.join(&spec.folder),
        None => PathBuf::from(&spec.folder),
    };
    if !root.is_dir() {
        return Err(BuildError::BadPage(format!(
            "include folder '{}' not found (or not a directory)",
            root.display()
        )));
    }
    let prefix = spec
        .prefix
        .clone()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| prefix_of(&spec.folder));

    let pairs = match (&spec.pattern, &spec.entry) {
        (Some(_), Some(_)) => {
            return Err(BuildError::BadPage(format!(
                "include \"{}\": set exactly one of `pattern` or `entry`, not both",
                spec.folder
            )));
        }
        (None, None) => {
            return Err(BuildError::BadPage(format!(
                "include \"{}\": set one of `pattern` (recursive filename glob) or \
                 `entry` (a path inside each immediate subdirectory)",
                spec.folder
            )));
        }
        (Some(pattern), None) => pattern_entries(&root, pattern)?,
        (None, Some(entry)) => entry_entries(&root, entry)?,
    };

    let mut out: Vec<IncludedSite> = Vec::new();
    // Two distinct entry files mapping to the same output subdir (two
    // `pattern` matches in one folder) is a build error — no silent overwrite.
    let mut by_subdir: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    for (name, src) in pairs {
        let out_subdir = Path::new(&prefix).join(&name);
        if let Some(prev) = by_subdir.get(&out_subdir)
            && prev != &src
        {
            return Err(BuildError::BadPage(format!(
                "include: two entry files map to the same output '{}' \
                 ('{}' and '{}') — only one match per subdirectory",
                out_subdir.display(),
                prev.display(),
                src.display()
            )));
        }
        by_subdir.insert(out_subdir.clone(), src.clone());
        let src_root = root.join(&name);
        out.push(IncludedSite {
            href: format!("{prefix}/{name}/"),
            name,
            src_path: src,
            src_root,
            out_subdir,
            site: spec.site.clone(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Read every `include` block on `doc`, resolve them, and validate the
/// combined output layout (no collision with a reserved site directory or
/// the shared asset folder; no two includes targeting the same subdir).
/// Shared by the build and skill fan-out steps.
pub(crate) fn collect_includes(
    doc: &Document,
    base_dir: Option<&Path>,
    reserved_dirs: &BTreeSet<String>,
) -> Result<Vec<IncludedSite>, BuildError> {
    let mut all: Vec<IncludedSite> = Vec::new();
    for b in doc.blocks().filter(|b| b.kind() == "include") {
        let folder = label_string(&b).ok_or_else(|| {
            BuildError::BadPage("an `include` block is missing its folder label".to_string())
        })?;
        let spec = IncludeSpec {
            folder,
            pattern: field_utf8(&b, "pattern"),
            entry: field_utf8(&b, "entry"),
            site: field_utf8(&b, "site"),
            prefix: field_utf8(&b, "prefix"),
        };
        all.extend(resolve_included(base_dir, &spec)?);
    }
    let mut targets: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    for s in &all {
        let top = s
            .out_subdir
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        if reserved_dirs.contains(&top) {
            return Err(BuildError::BadPage(format!(
                "include output '{}' collides with the site directory '{top}/' — \
                 rename the include folder or the site",
                s.out_subdir.display()
            )));
        }
        if top == crate::terminal::ASSET_DIR {
            return Err(BuildError::BadPage(format!(
                "include output '{}' collides with the reserved asset folder '{}/'",
                s.out_subdir.display(),
                crate::terminal::ASSET_DIR
            )));
        }
        if let Some(prev) = targets.insert(s.out_subdir.clone(), s.src_path.clone())
            && prev != s.src_path
        {
            return Err(BuildError::BadPage(format!(
                "two included documents target the same output '{}' ('{}' and '{}')",
                s.out_subdir.display(),
                prev.display(),
                s.src_path.display()
            )));
        }
    }
    Ok(all)
}

/// Find the included sub-site whose source folder owns `page_file`, returning
/// its entry `.wcl` (the document the page was built from). `None` when the page
/// belongs to the root document itself (no matching `include`). Used by the
/// `wcl editor` so its object/schema lookups introspect the right document
/// when the user is working on a sub-site (e.g. a wskill) page from the
/// top-level root. Matches the deepest sub-site folder that is an ancestor
/// of the page.
pub(crate) fn entry_for_page(
    doc: &Document,
    base_dir: Option<&Path>,
    page_file: &Path,
) -> Option<IncludedSite> {
    let page = std::fs::canonicalize(page_file).ok()?;
    let mut best: Option<(usize, IncludedSite)> = None;
    for b in doc.blocks().filter(|b| b.kind() == "include") {
        let Some(folder) = label_string(&b) else {
            continue;
        };
        let spec = IncludeSpec {
            folder: folder.clone(),
            pattern: field_utf8(&b, "pattern"),
            entry: field_utf8(&b, "entry"),
            site: field_utf8(&b, "site"),
            prefix: field_utf8(&b, "prefix"),
        };
        let Ok(sites) = resolve_included(base_dir, &spec) else {
            continue;
        };
        for s in sites {
            // The page's source file lives under the sub-site's source folder.
            let Ok(src_root) = std::fs::canonicalize(&s.src_root) else {
                continue;
            };
            if page.starts_with(&src_root) {
                let depth = src_root.components().count();
                if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                    best = Some((depth, s));
                }
            }
        }
    }
    best.map(|(_, s)| s)
}

/// Best-effort: open an included entry document and read its selected site's
/// `title` / `summary` for the `included_sites` record. Picks the site named
/// `site`, else the `root = true` site, else the first `site` block. Any
/// open / parse failure yields `(None, None)` — nav degrades to the folder
/// name rather than failing the parent render; the real error surfaces when
/// the build step actually builds the member.
pub(crate) fn read_entry_meta(
    src_path: &Path,
    site: Option<&str>,
) -> (Option<String>, Option<String>) {
    let Ok(src) = std::fs::read_to_string(src_path) else {
        return (None, None);
    };
    let base_dir = src_path.parent().map(Path::to_path_buf);
    let loader = crate::build::schema_registry().loader(disk_loader());
    let env = crate::build::wdoc_environment(base_dir.as_deref());
    let name = src_path.display().to_string();
    let Ok(doc) = Document::open_at_with_loader(&src, &name, base_dir, &env, loader) else {
        return (None, None);
    };
    let sites: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let block = match site {
        Some(want) => sites
            .iter()
            .find(|b| label_string(b).as_deref() == Some(want)),
        None => sites
            .iter()
            .find(|b| field_bool(b, "root") == Some(true))
            .or_else(|| sites.first()),
    };
    match block {
        Some(b) => (field_utf8(b, "title"), field_utf8(b, "summary")),
        None => (None, None),
    }
}

/// Enter a build of `file` under the `include` cycle / depth guard: reject if
/// the chain is too deep or `file` is already an ancestor being built, else
/// record its canonical path in `seen` and return it. The caller removes it
/// from `seen` on exit (so the same document can build in two independent
/// branches, but not within its own ancestry). Shared by the build and skill
/// guarded-recursion wrappers.
pub(crate) fn guard_enter(
    file: &Path,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<PathBuf, BuildError> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(BuildError::IncludeCycle(format!(
            "include nesting exceeded {MAX_INCLUDE_DEPTH} levels at '{}'",
            file.display()
        )));
    }
    let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    if seen.contains(&canon) {
        return Err(BuildError::IncludeCycle(format!(
            "include cycle: '{}' is already being built",
            file.display()
        )));
    }
    seen.insert(canon.clone());
    Ok(canon)
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
        let spec = IncludeSpec {
            folder: "nope".to_string(),
            pattern: Some("*.wcl".to_string()),
            entry: None,
            site: None,
            prefix: None,
        };
        let r = resolve_included(Some(Path::new("/nonexistent-wdoc-root")), &spec);
        assert!(r.is_err());
    }
}
