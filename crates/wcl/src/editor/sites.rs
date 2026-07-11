//! Repo-wide wdoc site discovery for `wcl editor`'s preview picker.
//!
//! Scans the served tree for `.wcl` files declaring top-level `site`
//! blocks (a cheap parse-level prefilter — no evaluation), then
//! introspects each candidate with [`wcl_wdoc::entry_site_info`] to get
//! its site names and `include` members. Members nest under their
//! including entry, so the picker shows which selections pull in whole
//! sub-site trees (and will take correspondingly long to build).
//!
//! The scan runs per-request with no cache: the UI fetches once at load
//! and on explicit refresh, and candidate entries are few. If that ever
//! hurts, memoize `entry_site_info` by `(path, mtime)`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Mirror `include`'s recursion backstop.
const MAX_DEPTH: usize = 8;

/// `GET /api/sites` — every previewable site under `root_dir`, nested.
pub(crate) fn scan_sites(
    root_dir: &Path,
    root_file: Option<&Path>,
) -> Result<serde_json::Value, String> {
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    let walk = ignore::WalkBuilder::new(root_dir)
        .hidden(false)
        .require_git(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| e.file_name() != std::ffi::OsStr::new(".git"))
        .sort_by_file_name(std::ffi::OsStr::cmp)
        .build();
    for entry in walk.flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file())
            || path.extension().and_then(|e| e.to_str()) != Some("wcl")
        {
            continue;
        }
        if declares_site_block(path) {
            candidates.insert(canon(path));
        }
    }
    if let Some(rf) = root_file {
        candidates.insert(canon(rf));
    }

    // Build every candidate's node tree, remembering which entries got
    // claimed as include members — those nest instead of listing as roots.
    let mut claimed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut trees: Vec<(PathBuf, Vec<serde_json::Value>)> = Vec::new();
    for entry in &candidates {
        let mut visited = BTreeSet::new();
        let nodes = nodes_for_entry(root_dir, entry, None, 0, &mut visited, &mut claimed);
        trees.push((entry.clone(), nodes));
    }
    let sites: Vec<serde_json::Value> = trees
        .into_iter()
        .filter(|(entry, _)| !claimed.contains(entry))
        .flat_map(|(_, nodes)| nodes)
        .collect();
    Ok(serde_json::json!({ "sites": sites }))
}

/// Parse-level prefilter: does the file declare a top-level `site` block?
/// Textual `contains` first so most files skip the parse entirely.
fn declares_site_block(path: &Path) -> bool {
    let Ok(src) = std::fs::read_to_string(path) else {
        return false;
    };
    if !src.contains("site") {
        return false;
    }
    let Ok(ast) = wcl_lang::parse_for_edit(&src, path.display().to_string()) else {
        return false;
    };
    ast.items
        .iter()
        .any(|i| matches!(i, wcl_lang::ast::Item::Block(b) if b.kind == "site"))
}

/// The picker nodes one entry document contributes: one per declared site
/// (or the synthetic default), with the entry's `include` members nested
/// under the primary node. `site_filter` narrows to one named site when the
/// claiming `include` selects one.
fn nodes_for_entry(
    root_dir: &Path,
    entry: &Path,
    site_filter: Option<&str>,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    claimed: &mut BTreeSet<PathBuf>,
) -> Vec<serde_json::Value> {
    if depth > MAX_DEPTH || !visited.insert(entry.to_path_buf()) {
        return Vec::new();
    }
    let Ok((mut sites, includes)) = wcl_wdoc::entry_site_info(entry) else {
        return Vec::new();
    };
    if let Some(f) = site_filter
        && sites.iter().any(|s| s.site.as_deref() == Some(f))
    {
        sites.retain(|s| s.site.as_deref() == Some(f));
    }
    if sites.is_empty() {
        return Vec::new();
    }

    let mut children: Vec<serde_json::Value> = Vec::new();
    for m in &includes {
        claimed.insert(m.entry.clone());
        children.extend(nodes_for_entry(
            root_dir,
            &m.entry,
            m.site.as_deref(),
            depth + 1,
            visited,
            claimed,
        ));
    }

    // The include members hang off the primary site: the `root = true`
    // one, else the first non-skill one (a full build of it also builds
    // the includes; the siblings are narrower site_filter selections).
    let primary = sites
        .iter()
        .position(|s| s.root)
        .or_else(|| sites.iter().position(|s| !s.skill))
        .unwrap_or(0);
    let rel = entry
        .strip_prefix(root_dir)
        .unwrap_or(entry)
        .to_string_lossy()
        .replace('\\', "/");
    let dir_name = entry
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.clone());
    sites
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = s
                .title
                .clone()
                .or_else(|| s.site.clone())
                .unwrap_or_else(|| dir_name.clone());
            serde_json::json!({
                "entry": rel,
                "site": s.site,
                "label": label,
                "skill": s.skill,
                "children": if i == primary { children.clone() } else { Vec::new() },
            })
        })
        .collect()
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}
