//! File operations for `wcl editor`: the gitignore-aware repo tree, text
//! reads with save etags, and the save dispatch (validating commit for
//! `.wcl` under a root document, plain atomic write otherwise).

use std::path::{Path, PathBuf};

use super::Workspace;
use super::preview::Sessions;
use crate::edit::{commit, content_etag, read, str_field};

/// `GET /api/files` — every file and directory under `root_dir`, honouring
/// `.gitignore` (also outside a git repo), excluding `.git` itself. Paths are
/// `root_dir`-relative and `/`-normalized; directories come with
/// `type: "dir"` so the client can render empty folders.
pub(crate) fn list_tree(ws: &Workspace) -> Result<serde_json::Value, String> {
    let root_dir = ws.root_dir();
    let mut files = Vec::new();
    let walk = ignore::WalkBuilder::new(root_dir)
        .hidden(false) // show dotfiles (.gitignore, .github, …)
        .require_git(false) // honour .gitignore even outside a repo
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| e.file_name() != std::ffi::OsStr::new(".git"))
        .sort_by_file_name(std::ffi::OsStr::cmp)
        .build();
    for entry in walk {
        let entry = entry.map_err(|e| format!("walk: {e}"))?;
        let Ok(rel) = entry.path().strip_prefix(root_dir) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue; // the root itself
        }
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        files.push(serde_json::json!({
            "path": rel.to_string_lossy().replace('\\', "/"),
            "type": if is_dir { "dir" } else { "file" },
        }));
    }
    Ok(serde_json::json!({
        "root": root_dir.display().to_string(),
        "files": files,
    }))
}

/// `GET /api/file?path=` — a text file's contents plus the etag the save
/// endpoint checks. Binary content (NUL bytes / invalid UTF-8) is refused
/// with a pointer at `/api/raw`.
pub(crate) fn read_text(ws: &Workspace, path: &str) -> Result<serde_json::Value, String> {
    let file = resolve_existing(ws, path)?;
    let bytes = std::fs::read(&file).map_err(|e| format!("read {path}: {e}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("binary file: fetch /api/raw?path={path} instead"))?;
    if text.contains('\0') {
        return Err(format!("binary file: fetch /api/raw?path={path} instead"));
    }
    Ok(serde_json::json!({
        "path": path,
        "text": text,
        "etag": content_etag(&text),
    }))
}

/// `GET /api/raw?path=` — a file's raw bytes (image previews and other
/// binary assets).
pub(crate) fn read_raw(ws: &Workspace, path: &str) -> Result<(PathBuf, Vec<u8>), String> {
    let file = resolve_existing(ws, path)?;
    let bytes = std::fs::read(&file).map_err(|e| format!("read {path}: {e}"))?;
    Ok((file, bytes))
}

/// `POST /api/file` — save `{path, text, base_etag?}`. `base_etag` (from the
/// read) rejects the save when the file changed on disk underneath the
/// buffer. `.wcl` saves route through the validating [`commit`] pipeline
/// when a root document exists (rollback on newly introduced schema errors);
/// without one they still pass the syntax gate. Everything else is a plain
/// atomic write.
pub(crate) fn save_file(
    ws: &Workspace,
    previews: &Sessions,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = str_field(body, "path")?;
    let text = str_field(body, "text")?;
    let file = ws.abs_new(path)?;
    if let Some(base) = body.get("base_etag").and_then(serde_json::Value::as_str) {
        let current = content_etag(&read(&file)?);
        if current != base {
            return Err(
                "conflict: the file changed on disk — reload it and re-apply your edit".to_string(),
            );
        }
    }
    let is_wcl = file.extension().and_then(|s| s.to_str()) == Some("wcl");
    match (is_wcl, ws.root_file()) {
        (true, Some(root)) => {
            // Gate syntax here with a user-facing message — `commit` words its
            // own syntax gate as an internal error (in the WYSIWYG pipeline the
            // server generates the source; here the user typed it).
            crate::verify_reparses(text).map_err(|e| format!("syntax error: {e}"))?;
            commit(root, vec![(file, text.to_string())])?;
        }
        (true, None) => {
            crate::verify_reparses(text).map_err(|e| format!("syntax error: {e}"))?;
            write_plain(&file, text)?;
        }
        (false, _) => write_plain(&file, text)?,
    }
    // The disk moved under every built preview.
    previews.invalidate();
    Ok(serde_json::json!({ "ok": true, "etag": content_etag(text) }))
}

/// Atomic write with parent-directory creation (new files can live in new
/// folders).
fn write_plain(file: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create directory {}: {e}", parent.display()))?;
    }
    crate::write_atomic(file, text).map_err(|e| format!("write {}: {e}", file.display()))
}

/// Resolve a request-relative path to an existing file inside `root_dir`.
fn resolve_existing(ws: &Workspace, path: &str) -> Result<PathBuf, String> {
    let file = ws
        .abs(path)
        .map_err(|_| format!("no such file in the served tree: {path}"))?;
    if !file.is_file() {
        return Err(format!("not a file: {path}"));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace over a fresh temp dir — the one line a module-local test
    /// needs (no preview scratch tree, no session map).
    fn ws_at(dir: &Path) -> Workspace {
        Workspace::at(dir)
    }

    fn tree_paths(v: &serde_json::Value) -> Vec<(String, String)> {
        v["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["path"].as_str().unwrap().to_string(),
                    f["type"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn list_tree_honours_gitignore_and_skips_git_dir() {
        let td = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(td.path()).unwrap();
        std::fs::write(root.join(".gitignore"), "ignored.txt\ntarget/\n").unwrap();
        std::fs::write(root.join("kept.wcl"), "name = \"x\"\n").unwrap();
        std::fs::write(root.join("ignored.txt"), "nope").unwrap();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target").join("junk.bin"), "nope").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git").join("HEAD"), "ref").unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("lib.wcl"), "x = 1\n").unwrap();

        let v = list_tree(&ws_at(&root)).unwrap();
        let paths = tree_paths(&v);
        assert!(paths.contains(&(".gitignore".into(), "file".into())));
        assert!(paths.contains(&("kept.wcl".into(), "file".into())));
        assert!(paths.contains(&("src".into(), "dir".into())));
        assert!(paths.contains(&("src/lib.wcl".into(), "file".into())));
        assert!(!paths.iter().any(|(p, _)| p.contains("ignored.txt")));
        assert!(!paths.iter().any(|(p, _)| p.starts_with("target")));
        assert!(
            !paths
                .iter()
                .any(|(p, _)| p.starts_with(".git/") || p == ".git")
        );
    }

    #[test]
    fn read_text_rejects_binary() {
        let td = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(td.path()).unwrap();
        std::fs::write(root.join("img.png"), b"\x89PNG\0\0").unwrap();
        let err = read_text(&ws_at(&root), "img.png").unwrap_err();
        assert!(err.contains("/api/raw"), "{err}");
    }

    #[test]
    fn save_rejects_traversal_and_stale_etag() {
        let td = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(td.path()).unwrap();
        std::fs::write(root.join("a.txt"), "one").unwrap();

        let ws = ws_at(&root);
        let previews = Sessions::default();
        let esc = serde_json::json!({ "path": "../escape.txt", "text": "x" });
        assert!(save_file(&ws, &previews, &esc).is_err());

        let stale = serde_json::json!({
            "path": "a.txt", "text": "two", "base_etag": "not-the-etag",
        });
        let err = save_file(&ws, &previews, &stale).unwrap_err();
        assert!(err.contains("conflict"), "{err}");
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "one");
    }

    #[test]
    fn save_writes_new_file_in_new_directory() {
        let td = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(td.path()).unwrap();
        let body = serde_json::json!({ "path": "notes/new.md", "text": "# hi\n" });
        let v = save_file(&ws_at(&root), &Sessions::default(), &body).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(
            std::fs::read_to_string(root.join("notes").join("new.md")).unwrap(),
            "# hi\n"
        );
    }

    #[test]
    fn save_wcl_without_root_still_gates_syntax() {
        let td = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(td.path()).unwrap();
        let body = serde_json::json!({ "path": "bad.wcl", "text": "block {{{" });
        assert!(save_file(&ws_at(&root), &Sessions::default(), &body).is_err());
        assert!(!root.join("bad.wcl").exists());
    }
}
