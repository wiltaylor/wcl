//! The `file` block: ship an arbitrary file into the build output and
//! (optionally) link to it.
//!
//! Like [`crate::blocks::image`], a referenced local file is copied into the
//! output and referenced by relative URL. Unlike images, the file keeps
//! its **basename** under a target subdirectory (`dir`), so the emitted
//! path is stable and hand-linkable (`scripts/build.sh`) rather than
//! hashed, so `scripts/` / `assets/` keep the names the author wrote.
//! The [`FileRegistry`] is populated lazily on reference (during
//! rendering); every entry it holds was referenced, so
//! [`FileRegistry::copy_used`] copies them all. An external source
//! (`http(s)://`, `data:`, or a leading `/`) passes through verbatim and
//! is never copied.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::blocks::image::is_external;
use crate::build::BuildError;

/// The asset subdirectory the HTML / Markdown targets route a `file` into
/// when its `dir` is unset (the shared `_wdoc/` folder, alongside images).
fn default_dir() -> &'static str {
    crate::blocks::terminal::ASSET_DIR
}

/// One resolved file reference.
#[derive(Clone)]
pub(crate) struct FileEntry {
    /// The href to emit (a `<dir>/<basename>` relative URL for a copied
    /// local file, or the verbatim source for an external URL).
    pub url: String,
    /// Output path relative to the build root (`<dir>/<basename>`); `None`
    /// ⇒ external (never copied).
    out_rel: Option<String>,
    /// Source path on disk (`Some` for a local file to copy).
    src_path: Option<PathBuf>,
}

/// Lazily-populated registry of referenced files. Keyed by the
/// `(dir, source)` pair so repeat references share one copied file and a
/// basename collision within one `dir` (two different sources mapping to
/// the same output path) is detectable.
pub(crate) struct FileRegistry {
    /// Directory relative sources resolve against. `None` when the
    /// document was opened without one.
    base_dir: Option<PathBuf>,
    /// Resolved entries by `(source, directory)`, so one asset referenced
    /// twice is read and copied once.
    entries: RefCell<BTreeMap<(String, String), FileEntry>>,
}

impl FileRegistry {
    /// An empty registry resolving relative sources against `base_dir`.
    pub(crate) fn new(base_dir: Option<PathBuf>) -> Self {
        FileRegistry {
            base_dir,
            entries: RefCell::new(BTreeMap::new()),
        }
    }

    /// Resolve `source` into `dir`, recording it for copying (when local),
    /// and return its entry. `dir` empty ⇒ the default `_wdoc/` folder.
    /// Idempotent — the same `(dir, source)` resolves once.
    pub(crate) fn register(&self, source: &str, dir: &str) -> FileEntry {
        let dir = if dir.is_empty() {
            default_dir().to_string()
        } else {
            dir.trim_matches('/').to_string()
        };
        let key = (dir.clone(), source.to_string());
        if let Some(e) = self.entries.borrow().get(&key) {
            return e.clone();
        }
        let entry = self.build_entry(source, &dir);
        self.entries.borrow_mut().insert(key, entry.clone());
        entry
    }

    /// Resolve one asset: locate it, hash it, and decide its output path.
    fn build_entry(&self, source: &str, dir: &str) -> FileEntry {
        if is_external(source) {
            return FileEntry {
                url: source.to_string(),
                out_rel: None,
                src_path: None,
            };
        }
        let src_path = match &self.base_dir {
            Some(base) => base.join(source),
            None => PathBuf::from(source),
        };
        let base = src_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let out_rel = format!("{dir}/{base}");
        FileEntry {
            url: out_rel.clone(),
            out_rel: Some(out_rel),
            src_path: Some(src_path),
        }
    }

    /// Copy every referenced local file into `out_dir/<dir>/<basename>`.
    /// No-op when no local file was referenced. Two distinct sources that
    /// would land on the same output path are a build error (no silent
    /// overwrite).
    pub(crate) fn copy_used(&self, out_dir: &Path) -> Result<(), BuildError> {
        let entries = self.entries.borrow();
        let mut seen: BTreeMap<&str, &Path> = BTreeMap::new();
        for entry in entries.values() {
            let (Some(out_rel), Some(src)) = (&entry.out_rel, &entry.src_path) else {
                continue;
            };
            if let Some(prev) = seen.get(out_rel.as_str())
                && *prev != src.as_path()
            {
                return Err(BuildError::BadPage(format!(
                    "two different files map to the output path '{out_rel}' \
                     ('{}' and '{}') — give them distinct names or `dir`s",
                    prev.display(),
                    src.display()
                )));
            }
            seen.insert(out_rel.as_str(), src.as_path());
            let dest = out_dir.join(out_rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    BuildError::Io(e, format!("create_dir_all {}", parent.display()))
                })?;
            }
            std::fs::copy(src, &dest).map_err(|e| {
                BuildError::Io(e, format!("copy {} -> {}", src.display(), dest.display()))
            })?;
        }
        Ok(())
    }
}

/// Register a page `@block("file")` and render its HTML. The source is
/// recorded for copying into `dir`; the block renders a download `<a>` when
/// `as` (link text) is set, and nothing (just ships the file) otherwise.
pub(crate) fn render_html(block: &wcl_lang::Block<'_>, registry: &FileRegistry) -> String {
    use crate::render::{escape_html, field_id, field_utf8, field_utf8_list, label_string};
    use std::fmt::Write as _;

    let Some(source) = label_string(block) else {
        return String::new();
    };
    if source.is_empty() {
        return String::new();
    }
    let dir = field_utf8(block, "dir").unwrap_or_default();
    let entry = registry.register(&source, &dir);
    let Some(text) = field_utf8(block, "as") else {
        return String::new();
    };
    let mut classes = vec!["wdoc-file".to_string()];
    classes.extend(field_utf8_list(block, "class"));
    let cls = classes
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!("<a class=\"{cls}\" href=\"{}\"", escape_html(&entry.url));
    if let Some(id) = field_id(block, "id") {
        let _ = write!(out, " id=\"{}\"", escape_html(&id));
    }
    let _ = write!(out, ">{}</a>", escape_html(&text));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_sources_pass_through_uncopied() {
        let reg = FileRegistry::new(None);
        for src in [
            "https://x/y.sh",
            "http://x/y.sh",
            "data:text/plain,hi",
            "/abs.sh",
        ] {
            let e = reg.register(src, "scripts");
            assert_eq!(e.url, src, "{src} should pass through verbatim");
        }
        let tmp = std::env::temp_dir().join("wdoc-file-test-noop");
        assert!(reg.copy_used(&tmp).is_ok());
    }

    #[test]
    fn local_source_keeps_basename_under_dir() {
        let reg = FileRegistry::new(Some(PathBuf::from("/docs")));
        let a = reg.register("src/build.sh", "scripts");
        assert_eq!(a.url, "scripts/build.sh");
        // Same (dir, source) ⇒ same entry (idempotent + deterministic).
        assert_eq!(reg.register("src/build.sh", "scripts").url, a.url);
    }

    #[test]
    fn empty_dir_routes_to_asset_folder() {
        let reg = FileRegistry::new(Some(PathBuf::from("/docs")));
        let e = reg.register("notes.txt", "");
        assert_eq!(
            e.url,
            format!("{}/notes.txt", crate::blocks::terminal::ASSET_DIR)
        );
    }

    #[test]
    fn basename_collision_within_dir_errors() {
        let reg = FileRegistry::new(Some(PathBuf::from("/docs")));
        reg.register("a/run.sh", "scripts");
        reg.register("b/run.sh", "scripts");
        let tmp = std::env::temp_dir().join("wdoc-file-test-collision");
        assert!(reg.copy_used(&tmp).is_err());
    }
}
