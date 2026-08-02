//! Where a new block goes.
//!
//! Creating an object is two decisions: what to write, and where. This
//! module owns the second one — a distinct body of knowledge from request
//! handling, and one the editor derives entirely from the document's own
//! conventions rather than from configuration:
//!
//! - **one-per-file directories.** Where the existing instances of a kind
//!   each sit alone in a file in one directory (the wskill `data/<kind>s/`
//!   layout), a new one gets its own `<id>.wcl` beside them — plus an
//!   `ensure_import` into the sibling `main.wcl` aggregator when there is
//!   one, so it actually gathers.
//! - **multi-block files.** Otherwise the new block joins the file already
//!   holding the most instances of the kind.
//! - **generated files are never written to.** Extractor output carries a
//!   `GENERATED` banner and is overwritten wholesale on the next run.
//! - **the neighbouring-kind fallback.** A first-of-its-kind object must
//!   not land in the entry document: a projection entry (a WAD's book, a
//!   wskill's) is a different namespace, where the block would not even
//!   resolve to this schema. Placement looks for a data file of a
//!   neighbouring kind instead.
//! - **pinning.** A new unit may be appended to an `index` block's
//!   `related` list in the same commit.

use std::path::{Path, PathBuf};

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::{Document, Span, edit as ast_edit, format as wcl_format, parse_for_edit};

use super::kinds::KindModel;
use super::util::{find_block_by_kind_label, first_label};

/// Where a freshly built top-level block should land.
pub(super) enum Placement {
    NewFile {
        dir: PathBuf,
        aggregator: Option<PathBuf>,
    },
    Append {
        file: PathBuf,
    },
    /// An explicitly named file that doesn't exist yet: create it and
    /// import it from the owning entry document.
    NewTarget {
        file: PathBuf,
    },
}

/// Where a new instance of `kind` belongs, derived from where the existing
/// instances live (see the module docs).
pub(super) fn place_unit(
    model: &KindModel<'_>,
    doc: &Document,
    doc_entry: &Path,
    kind: &str,
) -> Result<Placement, String> {
    // A block the doc view can't attribute to a file came from the entry
    // itself, so that is where its instances count.
    let per_file = count_by_file(doc, Some(doc_entry), |k| k == kind);
    if per_file.is_empty() {
        // No instances to learn from. The entry document is the last
        // resort, NOT the first — look for a data file of a neighbouring
        // kind instead.
        return Ok(Placement::Append {
            file: kin_file(model, doc, kind).unwrap_or_else(|| doc_entry.to_path_buf()),
        });
    }
    // One-per-file layout: every instance alone in its file, all in one
    // directory → a fresh `<id>.wcl` beside them.
    let one_per_file = per_file.iter().all(|(_, n)| *n == 1);
    let dirs: Vec<&Path> = per_file.iter().filter_map(|(p, _)| p.parent()).collect();
    if one_per_file
        && dirs.windows(2).all(|w| w[0] == w[1])
        && let Some(dir) = dirs.first()
    {
        let aggregator = dir.join("main.wcl");
        return Ok(Placement::NewFile {
            dir: dir.to_path_buf(),
            aggregator: aggregator.is_file().then_some(aggregator),
        });
    }
    let (file, _) = per_file
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .expect("non-empty");
    Ok(Placement::Append { file })
}

/// Realise a [`Placement`] for a freshly built top-level block: stage the
/// file writes (a new `<id>.wcl` plus its aggregator import, an append to
/// an existing file, or a named new target imported from the entry) into
/// `changes` and answer the file the block landed in.
pub(super) fn write_new_block(
    placement: Placement,
    id: &str,
    block: ast::Block,
    doc_entry: &Path,
    changes: &mut Vec<(PathBuf, String)>,
) -> Result<PathBuf, String> {
    let new_file = match placement {
        Placement::NewFile { dir, aggregator } => {
            let file = dir.join(format!("{id}.wcl"));
            if file.exists() {
                return Err(format!("{} already exists", file.display()));
            }
            let mut src = ast::Source {
                items: Vec::new(),
                trailing_trivia: Vec::new(),
            };
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            if let Some(agg) = aggregator {
                let text = crate::edit::read(&agg)?;
                let mut asrc =
                    parse_for_edit(&text, agg.display().to_string()).map_err(super::err_str)?;
                ast_edit::ensure_import(&mut asrc, &format!("./{id}.wcl"));
                changes.push((agg, wcl_format::to_source(&asrc)));
            }
            file
        }
        Placement::Append { file } => {
            let text = crate::edit::read(&file)?;
            let mut src =
                parse_for_edit(&text, file.display().to_string()).map_err(super::err_str)?;
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            file
        }
        Placement::NewTarget { file } => {
            let mut src = ast::Source {
                items: Vec::new(),
                trailing_trivia: Vec::new(),
            };
            ast_edit::append_top_level_block(&mut src, block);
            changes.push((file.clone(), wcl_format::to_source(&src)));
            // Import it from the entry so the new instances gather.
            let entry_dir = doc_entry.parent().unwrap_or(doc_entry);
            if let Ok(rel) = file.strip_prefix(entry_dir) {
                let text = crate::edit::read(doc_entry)?;
                let mut esrc = parse_for_edit(&text, doc_entry.display().to_string())
                    .map_err(super::err_str)?;
                if ast_edit::ensure_import(&mut esrc, &rel.to_string_lossy().replace('\\', "/")) {
                    changes.push((doc_entry.to_path_buf(), wcl_format::to_source(&esrc)));
                }
            }
            file
        }
    };
    Ok(new_file)
}

/// Is this file written by a generator? Extractor output carries a
/// `GENERATED` banner and is overwritten wholesale on the next run — an
/// object created there would be silently lost (and, where a CI gate checks
/// the tree is fresh, would fail the build). Placement skips such files
/// entirely; objects already in them stay editable in place.
fn is_generated(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    // The banner is a leading comment, so only the head of the file matters.
    text.lines()
        .take(5)
        .take_while(|l| {
            let t = l.trim_start();
            t.is_empty() || t.starts_with("//") || t.starts_with('#')
        })
        .any(|l| l.contains("GENERATED"))
}

/// The data file a brand-new instance of `kind` should join when the kind
/// has no instances of its own: the file holding the most instances of a
/// neighbouring kind, tried in order — the kinds it nests into, the kinds
/// that nest into it, then any other kind declared in the same schema
/// namespace. `None` when the document holds no such data at all.
fn kin_file(model: &KindModel<'_>, doc: &Document, kind: &str) -> Option<PathBuf> {
    let me = model.get(kind)?;
    let ns = me.namespace();
    let parents: Vec<&str> = me.parents().iter().map(|p| p.kind.as_str()).collect();
    let children: Vec<&str> = model
        .kinds()
        .iter()
        .filter(|k| k.parents().iter().any(|p| p.kind == kind))
        .map(|k| k.kind())
        .collect();
    let same_ns: Vec<&str> = model
        .kinds()
        .iter()
        .filter(|k| k.kind() != kind && k.namespace() == ns)
        .map(|k| k.kind())
        .collect();

    for tier in [&parents, &children, &same_ns] {
        // A block with no source file came from the entry, which is exactly
        // the file this fallback exists to avoid — so it doesn't count.
        let per_file = count_by_file(doc, None, |k| tier.contains(&k));
        // Ties go to the first file in document order, so placement is
        // deterministic rather than dependent on iteration order.
        if let Some((file, _)) = per_file.into_iter().max_by_key(|(_, n)| *n) {
            return Some(file);
        }
    }
    None
}

/// How many blocks matching `want` each file holds, in document order.
/// Generated files never count — placement must not write to them. A block
/// the doc view can't attribute to a file counts against `fallback`, or is
/// skipped when there is none.
fn count_by_file(
    doc: &Document,
    fallback: Option<&Path>,
    want: impl Fn(&str) -> bool,
) -> Vec<(PathBuf, usize)> {
    let mut per_file: Vec<(PathBuf, usize)> = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if !want(block.kind()) {
            continue;
        }
        let Some(file) = path
            .map(Path::to_path_buf)
            .or(fallback.map(Path::to_path_buf))
        else {
            continue;
        };
        if is_generated(&file) {
            continue;
        }
        match per_file.iter_mut().find(|(p, _)| *p == file) {
            Some((_, n)) => *n += 1,
            None => per_file.push((file, 1)),
        }
    }
    per_file
}

/// Append `id` to the `related` list of the `index` block labelled
/// `index_id`, layering on top of any pending change to the same file.
pub(super) fn pin_into_index(
    doc: &Document,
    doc_entry: &Path,
    index_id: &str,
    id: &str,
    changes: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let (ifile, _) = doc
        .blocks_with_source()
        .find(|(_, b)| b.kind() == "index" && first_label(b).as_deref() == Some(index_id))
        .map(|(p, b)| {
            (
                p.map(Path::to_path_buf)
                    .unwrap_or_else(|| doc_entry.to_path_buf()),
                b.span(),
            )
        })
        .ok_or_else(|| format!("no `index` with id `{index_id}`"))?;
    // Base text: a pending change to the same file, else disk. Located by
    // kind + label (not span) because pending edits shift spans.
    let base = match changes.iter().find(|(p, _)| *p == ifile) {
        Some((_, text)) => text.clone(),
        None => crate::edit::read(&ifile)?,
    };
    let mut src = parse_for_edit(&base, ifile.display().to_string()).map_err(super::err_str)?;
    let block = find_block_by_kind_label(&mut src.items, "index", index_id)
        .ok_or_else(|| format!("could not relocate index `{index_id}`"))?;
    let related = block.items.iter_mut().find_map(|it| match it {
        Item::Field(f) if f.name == "related" => Some(f),
        _ => None,
    });
    let new_ident = Expr::Identifier(id.to_string(), Span::new(0, 0));
    match related {
        Some(f) => match &mut f.expr {
            Expr::ListLit {
                elements,
                elem_trivia,
                ..
            } => {
                elements.push(new_ident);
                elem_trivia.push(Default::default());
            }
            _ => {
                return Err(format!(
                    "index `{index_id}`'s related list is computed — edit its source instead"
                ));
            }
        },
        None => ast_edit::set_or_insert_field(
            block,
            "related",
            Expr::ListLit {
                elements: vec![new_ident],
                elem_trivia: vec![Default::default()],
                trailing_trivia: Vec::new(),
                span: Span::new(0, 0),
            },
        ),
    }
    let text = wcl_format::to_source(&src);
    match changes.iter_mut().find(|(p, _)| *p == ifile) {
        Some((_, pending)) => *pending = text,
        None => changes.push((ifile, text)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A first-of-its-kind object must land in a DATA file, never in the
    /// projection entry that renders it: the entry is a different
    /// namespace, where the block wouldn't even resolve to this schema.
    #[test]
    fn a_kind_with_no_instances_lands_beside_its_neighbours() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canon");
        std::fs::write(
            root.join("schema.wcl"),
            "namespace app\n\
             @block(\"zone\")\n\
             type Zone { @inline(0) id: identifier  name: utf8 }\n\
             @block(\"system\")\n\
             type System { @inline(0) id: identifier  name: utf8  zone: identifier? }\n\
             @document\n\
             type D {\n\
               @children(\"zone\")   zones:   list<Zone>\n\
               @children(\"system\") systems: list<System>\n\
             }\n",
        )
        .expect("write schema");
        std::fs::write(
            root.join("data.wcl"),
            "namespace app\n\nsystem s { name = \"S\"  zone = z }\n",
        )
        .expect("write data");
        // The entry is the projection: it imports the model but declares
        // none of it, and carries no `namespace`.
        std::fs::write(
            root.join("main.wcl"),
            "import \"./schema.wcl\"\nimport \"./data.wcl\"\n",
        )
        .expect("write main");
        let entry = root.join("main.wcl");
        let doc = wcl_wdoc::open_doc_for_edit(&entry).expect("open");
        let model = KindModel::new(&doc);

        // `zone` has no instances; `system` (which nests into it) lives in
        // data.wcl, so that is where a new zone belongs.
        match place_unit(&model, &doc, &entry, "zone").expect("placement") {
            Placement::Append { file } => {
                assert_eq!(file.file_name().and_then(|f| f.to_str()), Some("data.wcl"));
            }
            _ => panic!("expected an append placement"),
        }
    }
}
