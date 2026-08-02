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

use wcl_lang::ast::{Expr, Item};

/// Mirror `include`'s recursion backstop.
const MAX_DEPTH: usize = 8;

/// `GET /api/sites` — every previewable site under `root_dir`, nested.
/// A wskill's projection entries (registered by its `artifact` blocks)
/// collapse into one `{ wskill: true, views: […] }` node.
pub(crate) fn scan_sites(ws: &super::Workspace) -> Result<serde_json::Value, String> {
    let root_dir = ws.root_dir();
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    let mut registries: Vec<PathBuf> = Vec::new();
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
        if declares_wskill_registry(path) {
            registries.push(canon(path));
        }
    }
    if let Some(rf) = ws.root_file() {
        candidates.insert(canon(rf));
    }
    // A wskill's projection entries are candidates because its registry says
    // so, not because they declare a `site` in their own text: since the
    // shared templates were embedded, `wdoc/book/main.wcl` is two imports and
    // its site arrives with `import <wskill/book.wcl>`. The prefilter above
    // would never even parse it.
    for registry in &registries {
        let Some(reg) = read_wskill_registry(registry) else {
            continue;
        };
        let dir = registry.parent().unwrap_or(registry);
        for (_, _, entry) in &reg.artifacts {
            let abs = canon(&dir.join(entry));
            if abs.is_file() {
                candidates.insert(abs);
            }
        }
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
    let mut sites: Vec<serde_json::Value> = trees
        .into_iter()
        .filter(|(entry, _)| !claimed.contains(entry))
        .flat_map(|(_, nodes)| nodes)
        .collect();
    for registry in &registries {
        group_wskill(root_dir, registry, &mut sites);
    }
    Ok(serde_json::json!({ "sites": sites }))
}

/// Collapse a wskill's projection entries into one picker node with a
/// `views` list, ordered by the artifact registry. Matching nodes are taken
/// from **anywhere** in the tree — a projection also shows up nested under
/// a top-level site that `include`s the wskill (the docs site pulls every
/// wskill book in), and listing it there *and* in the grouped node would
/// show everything twice.
fn group_wskill(root_dir: &Path, registry: &Path, sites: &mut Vec<serde_json::Value>) {
    let Some(reg) = read_wskill_registry(registry) else {
        return;
    };
    let dir = registry.parent().unwrap_or(registry);
    // Artifact entry (registry-relative) → repo-relative entry string.
    let rel_of = |entry: &str| -> Option<String> {
        let abs = canon(&dir.join(entry));
        abs.strip_prefix(root_dir)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    };
    let mut views: Vec<serde_json::Value> = Vec::new();
    for (id, kind, entry) in &reg.artifacts {
        let Some(rel) = rel_of(entry) else { continue };
        // Every node of this entry, wherever it sits, becomes a view (a
        // projection file with several sites yields one view per site).
        let mut matched: Vec<serde_json::Value> = Vec::new();
        take_nodes_with_entry(sites, &rel, &mut matched);
        for node in matched {
            // The same projection can be pulled in by several includes —
            // one view per (entry, site) is enough.
            if views
                .iter()
                .any(|v| v["entry"] == node["entry"] && v["site"] == node["site"])
            {
                continue;
            }
            views.push(serde_json::json!({
                "id": id,
                "kind": kind,
                "entry": node["entry"],
                "site": node["site"],
                "label": node["label"],
                "skill": node["skill"],
                "children": node["children"],
            }));
        }
    }
    if views.is_empty() {
        return;
    }
    let root_rel = dir
        .strip_prefix(root_dir)
        .unwrap_or(dir)
        .to_string_lossy()
        .replace('\\', "/");
    sites.push(serde_json::json!({
        "wskill": true,
        "label": reg.topic_name,
        "root": root_rel,
        "registry": registry
            .strip_prefix(root_dir)
            .unwrap_or(registry)
            .to_string_lossy()
            .replace('\\', "/"),
        "views": views,
    }));
}

/// Remove every node with `entry` from the tree (any depth), collecting the
/// removed nodes.
fn take_nodes_with_entry(
    nodes: &mut Vec<serde_json::Value>,
    entry: &str,
    out: &mut Vec<serde_json::Value>,
) {
    nodes.retain(|n| {
        if n["entry"] == entry {
            out.push(n.clone());
            false
        } else {
            true
        }
    });
    for n in nodes.iter_mut() {
        if let Some(children) = n.get_mut("children").and_then(|c| c.as_array_mut()) {
            take_nodes_with_entry(children, entry, out);
        }
    }
}

struct WskillRegistry {
    topic_name: String,
    /// `(artifact id, kind symbol, entry path)` in registry order.
    artifacts: Vec<(String, String, String)>,
}

/// Parse-level prefilter + reader for a wskill registry file: a `topic`
/// block plus `artifact` blocks with literal `entry` paths.
fn declares_wskill_registry(path: &Path) -> bool {
    let Ok(src) = std::fs::read_to_string(path) else {
        return false;
    };
    src.contains("artifact") && src.contains("topic") && read_wskill_registry(path).is_some()
}

fn read_wskill_registry(path: &Path) -> Option<WskillRegistry> {
    let src = std::fs::read_to_string(path).ok()?;
    let ast = wcl_lang::parse_for_edit(&src, path.display().to_string()).ok()?;
    let mut topic_name = None;
    let mut artifacts = Vec::new();
    for item in &ast.items {
        let Item::Block(b) = item else { continue };
        match b.kind.as_str() {
            "topic" => {
                let name = b.items.iter().find_map(|it| match it {
                    Item::Field(f) if f.name == "name" => match &f.expr {
                        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                });
                topic_name = name.or_else(|| super::util::ast_label(b)).or(topic_name);
            }
            "artifact" => {
                let Some(id) = super::util::ast_label(b) else {
                    continue;
                };
                let field = |name: &str| {
                    b.items.iter().find_map(|it| match it {
                        Item::Field(f) if f.name == name => Some(&f.expr),
                        _ => None,
                    })
                };
                let kind = match field("kind") {
                    Some(Expr::Symbol(s)) => s.clone(),
                    _ => continue,
                };
                let entry = match field("entry") {
                    Some(Expr::Utf8(s) | Expr::Ascii(s)) => s.clone(),
                    _ => continue,
                };
                artifacts.push((id, kind, entry));
            }
            _ => {}
        }
    }
    if artifacts.is_empty() {
        return None;
    }
    Some(WskillRegistry {
        topic_name: topic_name.unwrap_or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "wskill".to_string())
        }),
        artifacts,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Workspace;
    use crate::editor::testsupport::workspace_built_by;

    fn scan(ws: &Workspace) -> serde_json::Value {
        scan_sites(ws).expect("scan")
    }

    #[test]
    fn lists_nested_sites() {
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::write(
                root.join("main.wcl"),
                "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\nsite deck {\n  default_template = :ai_skill\n}\n\npage index {\n  title = \"Hi\"\n  sites = [:docs]\n\n  h1 \"Hi\"\n}\n\ninclude \"members\" {\n  entry = \"main.wcl\"\n}\n",
            )
            .unwrap();
            std::fs::create_dir_all(root.join("members/alpha")).unwrap();
            std::fs::write(
                root.join("members/alpha/main.wcl"),
                "import <wdoc.wcl>\n\nsite book {\n  title = \"Alpha Book\"\n}\n\npage index {\n  title = \"Alpha\"\n\n  h1 \"Alpha\"\n}\n",
            )
            .unwrap();
        });

        let v = scan(&ws);
        let sites = v["sites"].as_array().unwrap();
        // The member is claimed by the include, so only main.wcl's two
        // sites list at the top level.
        assert_eq!(sites.len(), 2, "{v:#}");
        let docs = &sites[0];
        assert_eq!(docs["entry"], "main.wcl");
        assert_eq!(docs["site"], "docs");
        assert_eq!(docs["label"], "The Docs");
        assert_eq!(docs["skill"], false);
        let children = docs["children"].as_array().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["entry"], "members/alpha/main.wcl");
        assert_eq!(children[0]["label"], "Alpha Book");
        let deck = &sites[1];
        assert_eq!(deck["site"], "deck");
        assert_eq!(deck["skill"], true);
        assert!(deck["children"].as_array().unwrap().is_empty());
    }

    #[test]
    fn groups_wskill_views() {
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::create_dir_all(root.join("wdoc/book")).unwrap();
            std::fs::create_dir_all(root.join("wdoc/skill")).unwrap();
            std::fs::write(
                root.join("wskill.wcl"),
                "topic demo {\n  name = \"Demo Topic\"\n}\n\n\
                 artifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n\n\
                 artifact ai_skill {\n  kind = :ai_skill\n  entry = \"wdoc/skill/main.wcl\"\n}\n",
            )
            .unwrap();
            // The book entry declares no `site` of its own — it imports one,
            // the way every wskill's does since the shared book template was
            // embedded. It is a candidate because the registry names it.
            std::fs::write(
                root.join("wdoc/book/main.wcl"),
                "import <wdoc.wcl>\nimport \"./projection.wcl\"\n",
            )
            .unwrap();
            std::fs::write(
                root.join("wdoc/book/projection.wcl"),
                "site book {\n  title = \"Demo Book\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
            )
            .unwrap();
            std::fs::write(
                root.join("wdoc/skill/main.wcl"),
                "import <wdoc.wcl>\n\nsite skill {\n  default_template = :ai_skill\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
            )
            .unwrap();
            // A plain, unrelated site stays a normal node.
            std::fs::write(
                root.join("other.wcl"),
                "import <wdoc.wcl>\n\nsite docs {\n  title = \"Other\"\n  root = true\n}\n\npage index {\n  title = \"O\"\n\n  h1 \"O\"\n}\n",
            )
            .unwrap();
        });

        let v = scan(&ws);
        let sites = v["sites"].as_array().unwrap();
        let wskill = sites
            .iter()
            .find(|s| s["wskill"] == true)
            .unwrap_or_else(|| panic!("no grouped wskill node: {v:#}"));
        assert_eq!(wskill["label"], "Demo Topic");
        assert_eq!(wskill["root"], "");
        let views = wskill["views"].as_array().unwrap();
        assert_eq!(views.len(), 2, "{v:#}");
        assert_eq!(views[0]["kind"], "book");
        assert_eq!(views[0]["entry"], "wdoc/book/main.wcl");
        assert_eq!(views[0]["site"], "book");
        assert_eq!(views[0]["skill"], false);
        assert_eq!(views[1]["kind"], "ai_skill");
        assert_eq!(views[1]["skill"], true);
        // The projections no longer list as separate top-level nodes; the
        // unrelated site does.
        assert!(
            !sites
                .iter()
                .any(|s| s["entry"] == "wdoc/book/main.wcl" || s["entry"] == "wdoc/skill/main.wcl"),
            "{v:#}"
        );
        assert!(sites.iter().any(|s| s["entry"] == "other.wcl"));
    }

    /// A projection pulled in by a parent site's `include` must not list
    /// twice: it moves into the grouped wskill node and disappears from
    /// the parent's children.
    #[test]
    fn groups_wskill_views_nested_under_include() {
        let (_td, ws) = workspace_built_by(|root| {
            std::fs::create_dir_all(root.join("skills/demo/wdoc/book")).unwrap();
            std::fs::write(
                root.join("main.wcl"),
                "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n\ninclude \"skills\" {\n  entry = \"wdoc/book/main.wcl\"\n}\n",
            )
            .unwrap();
            std::fs::write(
                root.join("skills/demo/wskill.wcl"),
                "topic demo {\n  name = \"Demo Topic\"\n}\n\nartifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n",
            )
            .unwrap();
            std::fs::write(
                root.join("skills/demo/wdoc/book/main.wcl"),
                "import <wdoc.wcl>\n\nsite book {\n  title = \"Demo Book\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n",
            )
            .unwrap();
        });

        let v = scan(&ws);
        let sites = v["sites"].as_array().unwrap();
        let docs = sites.iter().find(|s| s["entry"] == "main.wcl").unwrap();
        // The included book left the docs node's children…
        assert!(
            docs["children"].as_array().unwrap().is_empty(),
            "included projection must move into the wskill node: {v:#}"
        );
        // …and lives exactly once, as the grouped wskill's view.
        let wskill = sites.iter().find(|s| s["wskill"] == true).unwrap();
        assert_eq!(wskill["label"], "Demo Topic");
        let views = wskill["views"].as_array().unwrap();
        assert_eq!(views.len(), 1, "{v:#}");
        assert_eq!(views[0]["entry"], "skills/demo/wdoc/book/main.wcl");
        fn count_entry(nodes: &[serde_json::Value], entry: &str) -> usize {
            nodes
                .iter()
                .map(|n| {
                    usize::from(n["entry"] == entry)
                        + n["children"]
                            .as_array()
                            .map(|c| count_entry(c, entry))
                            .unwrap_or(0)
                })
                .sum()
        }
        assert_eq!(count_entry(sites, "skills/demo/wdoc/book/main.wcl"), 0);
    }
}
