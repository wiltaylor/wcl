//! Workspace-wide symbol search (`workspace/symbol`).
//!
//! Symbols come from the root document's eager-import graph
//! ([`Document::all_symbols`]) plus any open buffers whose path falls
//! outside it; with no root configured, the open buffers alone are
//! searched. Walking the workspace directory for orphan `.wcl` files is
//! a deliberate non-goal: a file that is neither imported nor open is
//! not part of the document.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[allow(deprecated)] // SymbolInformation::deprecated is required by lsp-types
use tower_lsp::lsp_types::{Location, SymbolInformation, Url};
use wcl_lang::{Document, SymbolRecord};

use crate::convert::span_to_range;
use crate::symbols::classify;

/// Hard cap on returned matches, mirroring what editors render.
const MAX_RESULTS: usize = 256;

/// Resolve `workspace/symbol`: fuzzy-match `query` against every symbol
/// visible from the root document and the open buffers, best first.
pub(crate) fn workspace_symbols(
    query: &str,
    root_doc: Option<&Document>,
    root_path: Option<&Path>,
    open_buffers: &HashMap<PathBuf, String>,
) -> Vec<SymbolInformation> {
    // One text read per file (overlay first, then disk) — span→range
    // conversion needs the declaring file's bytes.
    let mut texts: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut text_for = |path: &Path| -> Option<String> {
        texts
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                open_buffers
                    .get(path)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(path).ok())
            })
            .clone()
    };

    // (score, path, record) triples; resolved to locations only for the
    // entries that survive the cap.
    let mut hits: Vec<(u32, PathBuf, SymbolRecord)> = Vec::new();
    let mut in_graph: Vec<PathBuf> = Vec::new();

    if let (Some(doc), Some(root)) = (root_doc, root_path) {
        in_graph.push(root.to_path_buf());
        for (path, rec) in doc.all_symbols() {
            let path = path.unwrap_or(root);
            if !in_graph.iter().any(|p| p == path) {
                in_graph.push(path.to_path_buf());
            }
            if let Some(score) = fuzzy_score(query, &rec.fqn) {
                hits.push((score, path.to_path_buf(), rec.clone()));
            }
        }
    }

    // Open buffers outside the graph (or all of them in per-file mode)
    // are parsed individually, the same way the outline does.
    for (path, text) in open_buffers {
        if in_graph.iter().any(|p| p == path) {
            continue;
        }
        let Ok(doc) = Document::open(text, &path.display().to_string()) else {
            continue;
        };
        for rec in doc.symbols().iter() {
            if let Some(score) = fuzzy_score(query, &rec.fqn) {
                hits.push((score, path.clone(), rec.clone()));
            }
        }
    }

    // Best score first, name as the tiebreak for a stable order.
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.fqn.cmp(&b.2.fqn)));
    hits.truncate(MAX_RESULTS);

    hits.into_iter()
        .filter_map(|(_, path, rec)| {
            let text = text_for(&path)?;
            let uri = Url::from_file_path(&path).ok()?;
            let (kind, container_name) = classify(&rec.kind);
            #[allow(deprecated)]
            Some(SymbolInformation {
                name: short_name(&rec.fqn).to_string(),
                kind,
                tags: None,
                deprecated: None,
                location: Location {
                    uri,
                    range: span_to_range(&text, rec.span),
                },
                container_name,
            })
        })
        .collect()
}

/// Case-insensitive fuzzy score against the symbol's short name and its
/// FQN: exact > prefix > contiguous substring > in-order subsequence;
/// the short name outranks the FQN at every tier. `None` means no
/// match; an empty query matches everything (editors send it to list
/// all symbols).
fn fuzzy_score(query: &str, fqn: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(1);
    }
    let query = query.to_ascii_lowercase();
    let short = short_name(fqn).to_ascii_lowercase();
    let full = fqn.to_ascii_lowercase();
    let tier = |hay: &str| -> Option<u32> {
        if *hay == query {
            Some(8)
        } else if hay.starts_with(&query) {
            Some(6)
        } else if hay.contains(&query) {
            Some(4)
        } else if is_subsequence(&query, hay) {
            Some(2)
        } else {
            None
        }
    };
    match (tier(&short), tier(&full)) {
        (Some(s), f) => Some(s + 1 + f.unwrap_or(0) / 8),
        (None, Some(f)) => Some(f),
        (None, None) => None,
    }
}

/// Whether `needle`'s characters appear in `hay` in order — the
/// fuzzy match behind workspace symbol search.
fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut chars = hay.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

/// The last segment of a dotted name.
fn short_name(fqn: &str) -> &str {
    fqn.rsplit('.').next().unwrap_or(fqn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_tiers_rank_exact_over_prefix_over_substring_over_subsequence() {
        let exact = fuzzy_score("color", "shared.Color").unwrap();
        let prefix = fuzzy_score("col", "shared.Color").unwrap();
        let substr = fuzzy_score("olo", "shared.Color").unwrap();
        let subseq = fuzzy_score("clr", "shared.Color").unwrap();
        assert!(exact > prefix, "{exact} > {prefix}");
        assert!(prefix > substr, "{prefix} > {substr}");
        assert!(substr > subseq, "{substr} > {subseq}");
        assert!(fuzzy_score("xyz", "shared.Color").is_none());
    }

    #[test]
    fn fqn_segments_match_too() {
        // Matching the namespace prefix only reaches the FQN tier.
        assert!(fuzzy_score("shared", "shared.Color").is_some());
        // Empty query matches everything.
        assert!(fuzzy_score("", "anything").is_some());
    }

    #[test]
    fn open_buffer_symbols_searchable_without_a_root() {
        let mut buffers = HashMap::new();
        buffers.insert(
            PathBuf::from("/tmp/standalone.wcl"),
            "type Widget {\n  size: i64\n}\n".to_string(),
        );
        let hits = workspace_symbols("Widg", None, None, &buffers);
        // The member `Widget.size` also matches through its FQN, but
        // the short-name prefix hit ranks first.
        assert!(!hits.is_empty());
        assert_eq!(hits[0].name, "Widget");
    }
}
