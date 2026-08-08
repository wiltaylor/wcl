//! The editing core a writing command uses.
//!
//! Every write goes through the same edit pipeline `wcl set` uses —
//! `parse_for_edit` → mutate the owned AST by byte span →
//! `wcl_lang::format::to_source` → [`crate::verify_reparses`] →
//! [`crate::write_atomic`] — so a save produces a real `.wcl` edit. The
//! centrepiece is [`commit`]: write atomically, reopen the owning document,
//! and roll back any change that *introduces* a schema error over the
//! on-disk baseline. Validation reopens the document the way the build does,
//! via [`wcl_wdoc::open_doc_for_edit`], so a write is checked against the
//! same `@block` / `@table` schemas the renderer uses.
//!
//! No caller is left in the binary: the last one went with the command that
//! wrote structural edits. The module stays, tested, until the end-of-strip
//! cleanup pass (#215) decides what survives.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

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
}
