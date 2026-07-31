//! Shared editing core for `wcl editor`.
//!
//! Every write goes through the same edit pipeline `wcl set` uses —
//! [`parse_for_edit`] → mutate the owned AST by byte span →
//! [`wcl_format::to_source`] → [`crate::verify_reparses`] → [`crate::write_atomic`]
//! — so a save produces a real `.wcl` edit. The centrepiece is [`commit`]:
//! write atomically, reopen the owning document, and roll back any change that
//! *introduces* a schema error over the on-disk baseline.
//!
//! Reads reopen the document the way the build does, via
//! [`wcl_wdoc::open_doc_for_edit`], so the editor sees the same `@block` /
//! `@table` schemas the renderer does. [`locate_object`] resolves an
//! `edit_object` button's `kind` + `target` to the declaring file and byte
//! span, so the editor can open the source at that instance.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wcl_lang::{Span, Value, format as wcl_format, parse_for_edit};

/// Resolve a data-object reference (`kind` + optional `target` label) to the
/// file and byte span of its declaring block — the editor-side backend of the
/// `edit_object` "Edit this …" button. Unsaved buffers are overlaid so spans
/// agree with what the user sees. Matching mirrors the block's display label:
/// its first label value, falling back to the kind name. Without a `target`,
/// the kind must have exactly one instance.
pub(crate) fn locate_object(
    doc_entry: &Path,
    kind: &str,
    target: Option<&str>,
    overlay: HashMap<PathBuf, String>,
) -> Result<(PathBuf, Span), String> {
    let doc = wcl_wdoc::open_doc_for_edit_with_overlay(doc_entry, overlay).map_err(render_err)?;
    let mut hits: Vec<(String, PathBuf, Span)> = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != kind {
            continue;
        }
        let label = block
            .labels()
            .ok()
            .and_then(|ls| ls.first().map(value_label))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| kind.to_string());
        let file = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| doc_entry.to_path_buf());
        hits.push((label, file, block.span()));
    }
    match target {
        Some(t) => hits
            .into_iter()
            .find(|(label, _, _)| label == t)
            .map(|(_, file, span)| (file, span))
            .ok_or_else(|| format!("no `{kind}` with id/label `{t}`")),
        None => {
            if hits.len() == 1 {
                let (_, file, span) = hits.remove(0);
                Ok((file, span))
            } else if hits.is_empty() {
                Err(format!("no `{kind}` instances in this document"))
            } else {
                let labels: Vec<&str> = hits.iter().map(|(l, _, _)| l.as_str()).collect();
                Err(format!(
                    "multiple `{kind}` instances — pass a target: {}",
                    labels.join(", ")
                ))
            }
        }
    }
}

/// Canonically format WCL source (the `wcl fmt` core: parse for edit,
/// re-render). A syntax error comes back as `Err` so the caller keeps the
/// buffer untouched.
pub(crate) fn format_source(body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let text = str_field(body, "text")?;
    let ast = parse_for_edit(text, "<editor>".to_string()).map_err(render_err)?;
    Ok(serde_json::json!({ "text": wcl_format::to_source(&ast) }))
}

/// A stable-within-this-process content hash used as the save etag.
pub(crate) fn content_etag(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:x}", h.finish())
}

// ---------------------------------------------------------------------------
// Commit pipeline (write → validate → rollback)
// ---------------------------------------------------------------------------

/// Write every `(path, contents)` change atomically, then reopen the root
/// document and run schema validation. If anything fails to re-parse or the
/// document has schema errors, restore the originals and return the message —
/// so a constraint violation surfaces as an error and never lands on disk.
pub(crate) fn commit(
    root_file: &Path,
    changes: Vec<(PathBuf, String)>,
) -> Result<serde_json::Value, String> {
    use std::collections::HashSet;

    // Syntax gate before touching disk.
    for (path, content) in &changes {
        crate::verify_reparses(content).map_err(|e| {
            format!(
                "internal: produced unparseable WCL for {}: {e}",
                path.display()
            )
        })?;
    }
    // Pre-existing schema errors (unrelated to this edit) must not block it —
    // capture them so we only reject errors the edit *introduces*.
    let baseline: HashSet<String> = wcl_wdoc::open_doc_for_edit(root_file)
        .map(|d| d.schema_errors().iter().map(|e| e.to_string()).collect())
        .unwrap_or_default();
    // Back up originals (None = file did not exist → rollback deletes it).
    let backups: Vec<(PathBuf, Option<String>)> = changes
        .iter()
        .map(|(p, _)| (p.clone(), std::fs::read_to_string(p).ok()))
        .collect();
    for (path, content) in &changes {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            restore(&backups);
            return Err(format!("create directory {}: {e}", parent.display()));
        }
        if let Err(e) = crate::write_atomic(path, content) {
            restore(&backups);
            return Err(format!("write {}: {e}", path.display()));
        }
    }
    // Semantic gate: reopen + validate; roll everything back if the edit added
    // any schema error not already present at baseline.
    match wcl_wdoc::open_doc_for_edit(root_file) {
        Ok(doc) => {
            let introduced: Vec<String> = doc
                .schema_errors()
                .iter()
                .map(|e| e.to_string())
                .filter(|m| !baseline.contains(m))
                .collect();
            if !introduced.is_empty() {
                restore(&backups);
                return Err(introduced.join("; "));
            }
        }
        Err(e) => {
            restore(&backups);
            return Err(render_err(e));
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

fn restore(backups: &[(PathBuf, Option<String>)]) {
    for (path, original) in backups {
        match original {
            Some(content) => {
                let _ = crate::write_atomic(path, content);
            }
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

pub(crate) fn str_field<'a>(body: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    body.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing `{key}`"))
}

pub(crate) fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// A short, plain rendering of a scalar value (labels, defaults, enum names).
fn value_label(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

fn render_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with a `page` (whose `title` the schema requires) plus a
    /// second file to commit alongside it.
    const DOC: &str = "import <wdoc.wcl>\nimport \"extra.wcl\"\n\nsite docs {\n  title = \"D\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hello\"\n}\n";

    /// A temp dir holding `main.wcl` + `extra.wcl`, and the root file's path.
    fn project() -> (tempfile::TempDir, PathBuf) {
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("main.wcl");
        std::fs::write(&root, DOC).unwrap();
        std::fs::write(td.path().join("extra.wcl"), "let unused = 1\n").unwrap();
        (td, root)
    }

    #[test]
    fn etag_tracks_content_and_format_is_canonical() {
        assert_eq!(content_etag("a = 1\n"), content_etag("a = 1\n"));
        assert_ne!(content_etag("a = 1\n"), content_etag("a = 2\n"));

        let v = format_source(&serde_json::json!({ "text": "name=\"x\"" })).expect("format");
        assert_eq!(v["text"], "name = \"x\"\n");
        // A syntax error is an error, so the caller keeps the buffer.
        assert!(format_source(&serde_json::json!({ "text": "block {{{" })).is_err());
        // …as is a body with no `text` at all.
        assert!(format_source(&serde_json::json!({ "nope": true })).is_err());
    }

    #[test]
    fn commit_writes_every_change_atomically() {
        let (td, root) = project();
        let edited = DOC.replace("Hello", "Edited");
        commit(
            &root,
            vec![
                (root.clone(), edited.clone()),
                (td.path().join("extra.wcl"), "let unused = 2\n".to_string()),
            ],
        )
        .expect("commit");
        assert_eq!(std::fs::read_to_string(&root).unwrap(), edited);
        assert_eq!(
            std::fs::read_to_string(td.path().join("extra.wcl")).unwrap(),
            "let unused = 2\n"
        );
    }

    /// The semantic gate: an edit that *introduces* a schema error is rolled
    /// back across every file it touched, not just the offending one.
    #[test]
    fn commit_rolls_back_a_schema_violation_across_all_files() {
        let (td, root) = project();
        let extra = td.path().join("extra.wcl");
        let before = (
            std::fs::read_to_string(&root).unwrap(),
            std::fs::read_to_string(&extra).unwrap(),
        );

        let e = commit(
            &root,
            vec![
                // `title` must be a string.
                (root.clone(), DOC.replace("title = \"Hi\"", "title = 42")),
                (extra.clone(), "let unused = 3\n".to_string()),
            ],
        )
        .unwrap_err();
        assert!(
            e.contains("title"),
            "the schema error names the offending field: {e}"
        );
        assert_eq!(std::fs::read_to_string(&root).unwrap(), before.0);
        assert_eq!(
            std::fs::read_to_string(&extra).unwrap(),
            before.1,
            "the innocent file rolls back too"
        );
    }

    /// Rollback of a file that did not exist deletes it — a half-created
    /// unit must not survive a refused commit.
    #[test]
    fn commit_rollback_deletes_files_it_created() {
        let (td, root) = project();
        let fresh = td.path().join("new/unit.wcl");
        assert!(
            commit(
                &root,
                vec![
                    (root.clone(), DOC.replace("title = \"Hi\"", "title = 42")),
                    (fresh.clone(), "let x = 1\n".to_string()),
                ],
            )
            .is_err()
        );
        assert!(!fresh.exists(), "a created file must not survive rollback");
    }

    /// Schema errors already on disk are the caller's problem, not this
    /// edit's: the baseline is captured before the write so a pre-existing
    /// violation cannot block an unrelated change.
    #[test]
    fn commit_allows_an_edit_over_a_pre_existing_schema_error() {
        let (_td, root) = project();
        let broken = DOC.replace("title = \"Hi\"", "title = 42");
        std::fs::write(&root, &broken).unwrap();

        let edited = broken.replace("h1 \"Hello\"", "h1 \"Still broken, still saved\"");
        commit(&root, vec![(root.clone(), edited.clone())])
            .expect("a pre-existing error must not block an unrelated edit");
        assert_eq!(std::fs::read_to_string(&root).unwrap(), edited);
    }

    /// Unparseable output is caught before anything reaches disk — the
    /// syntax gate runs over every change first.
    #[test]
    fn commit_refuses_unparseable_output_before_writing() {
        let (_td, root) = project();
        let before = std::fs::read_to_string(&root).unwrap();
        let e = commit(&root, vec![(root.clone(), "page index {{{".to_string())]).unwrap_err();
        assert!(e.contains("unparseable"), "{e}");
        assert_eq!(std::fs::read_to_string(&root).unwrap(), before);
    }

    #[test]
    fn locate_object_resolves_by_label_and_reports_ambiguity() {
        let doc = "import <wdoc.wcl>\n\n\
            @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
            @block(\"thing\")\ntype Thing {\n  @inline(0) name: utf8\n}\n\n\
            site docs {\n  title = \"D\"\n  root = true\n}\n\n\
            thing \"alpha\" {}\n\nthing \"beta\" {}\n\n\
            page index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n";
        let td = tempfile::tempdir().unwrap();
        let root = td.path().join("main.wcl");
        std::fs::write(&root, doc).unwrap();

        let (file, span) =
            locate_object(&root, "thing", Some("beta"), HashMap::new()).expect("locate");
        assert_eq!(file, root);
        assert!(doc[span.start..span.end].starts_with("thing \"beta\""));

        // No target with two instances lists the labels; an unknown kind
        // says so.
        let e = locate_object(&root, "thing", None, HashMap::new()).unwrap_err();
        assert!(e.contains("alpha") && e.contains("beta"), "{e}");
        let e = locate_object(&root, "nothing", None, HashMap::new()).unwrap_err();
        assert!(e.contains("no `nothing` instances"), "{e}");
    }
}
