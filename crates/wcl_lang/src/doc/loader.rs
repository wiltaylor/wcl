//! File-loading abstraction. Lets callers (e.g. the LSP) overlay
//! in-memory buffer contents on top of the on-disk file tree so a
//! [`Document`](super::Document) parses against the user's *open*
//! source even before they hit save.
//!
//! The default loader is [`disk_loader`] — it just calls
//! `std::fs::read_to_string`. Anywhere `wcl_lang` would otherwise
//! read a file (top-level eager imports and in-block lazy imports
//! alike) goes through the document's `FileLoader` instead.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::imports::SYSTEM_IMPORT_ROOT;

/// Reader of source files for the loader. Implementations are
/// expected to be cheap to clone and safe to share across threads —
/// they're shaped as `Arc<dyn Fn>` so the closure form composes
/// naturally with `move`-captured state (e.g. an editor's open
/// buffers).
pub type FileLoader = Arc<dyn Fn(&Path) -> std::io::Result<String> + Send + Sync>;

/// Default loader: read every file straight from disk.
pub fn disk_loader() -> FileLoader {
    Arc::new(|p: &Path| std::fs::read_to_string(p))
}

/// Loader that overlays an in-memory map on top of disk. Paths in
/// `overlay` are checked first (matched by canonicalised key when
/// possible, then by raw key); anything not found in the overlay
/// falls through to `std::fs::read_to_string`.
///
/// Keys should be canonical absolute paths to match the way imports
/// are resolved internally. Keys and lookups both pass through
/// [`path_key`], so on Windows any spelling of a canonical path
/// matches: `std::fs::canonicalize`'s `\\?\C:\...` or the plain
/// `C:\...` imports resolve to, with either drive-letter case or
/// separator. The overlay also accepts raw keys as a convenience for
/// callers that have not canonicalised.
pub fn overlay_loader(overlay: HashMap<PathBuf, String>) -> FileLoader {
    let overlay: HashMap<PathBuf, String> = overlay
        .into_iter()
        .map(|(path, text)| (path_key(&path), text))
        .collect();
    Arc::new(move |p: &Path| {
        if let Some(s) = overlay.get(&path_key(p)) {
            return Ok(s.clone());
        }
        if let Ok(canon) = canonical_path(p)
            && let Some(s) = overlay.get(&canon)
        {
            return Ok(s.clone());
        }
        std::fs::read_to_string(p)
    })
}

/// The spelling of `path` that every spelling of it agrees on, so a
/// path can key a map whichever way it arrived. On Windows: the `\\?\`
/// verbatim prefix `std::fs::canonicalize` adds is dropped (`\\?\C:\x`
/// becomes `C:\x`, `\\?\UNC\host\share\x` becomes `\\host\share\x`)
/// where the plain form names the same file, the drive letter is
/// uppercased, and `/` becomes `\`. Nothing else changes case, since a
/// directory can be case-sensitive. Elsewhere `path` is returned as is.
///
/// A verbatim path keeps its prefix when dropping it would change what
/// it names: a component that is empty, `.` or `..`, ends in a space or
/// a dot, holds a character Windows forbids in a name, or is a device
/// name such as `CON`; or a path over 260 UTF-16 units.
pub fn path_key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        return PathBuf::from(windows_path_key(text));
    }
    path.to_path_buf()
}

/// `path` with symlinks and `..` resolved, in its [`path_key`]
/// spelling: the form imports resolve to, and the one diagnostics, a
/// document's `source_path` and an editor's URIs show.
pub fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|canon| path_key(&canon))
}

/// [`path_key`]'s Windows rules on a path's text, compiled on every
/// platform so they can be tested anywhere.
#[cfg(any(windows, test))]
fn windows_path_key(path: &str) -> String {
    let Some(verbatim) = path.strip_prefix(r"\\?\") else {
        return upper_drive(&path.replace('/', "\\"));
    };
    match plain_form(verbatim) {
        Some(plain) => upper_drive(&plain),
        None => format!(r"\\?\{}", upper_drive(verbatim)),
    }
}

/// The plain spelling of the verbatim path `rest` (the text after
/// `\\?\`), or `None` when there is none that names the same file.
#[cfg(any(windows, test))]
fn plain_form(rest: &str) -> Option<String> {
    let (plain, names) = if rest
        .get(..4)
        .is_some_and(|p| p.eq_ignore_ascii_case(r"UNC\"))
    {
        // A share path needs both its host and its share.
        let tail = &rest[4..];
        if tail.split('\\').filter(|name| !name.is_empty()).count() < 2 {
            return None;
        }
        (format!(r"\\{tail}"), tail)
    } else {
        let bytes = rest.as_bytes();
        let is_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        if !is_drive || bytes.get(2).is_some_and(|&b| b != b'\\') {
            return None;
        }
        (rest.to_string(), rest.get(3..).unwrap_or_default())
    };
    let names = names.strip_suffix('\\').unwrap_or(names);
    let every_name_plain = names.is_empty()
        || names
            .split('\\')
            .all(|name| is_plain_name(name) && !is_device_name(name));
    let fits = rest.encode_utf16().count() + 4 <= 260;
    (every_name_plain && fits).then_some(plain)
}

/// A name Windows reads the same with or without the verbatim prefix:
/// non-empty, at most 255 UTF-16 units, free of `<>:"/\|?*` and control
/// characters, and not ending in a space or a dot (so not `.` or `..`).
#[cfg(any(windows, test))]
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.encode_utf16().count() <= 255
        && !name
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        && !name.ends_with([' ', '.'])
}

/// A DOS device name (`CON`, `nul.txt`, `COM1 `…), which only the
/// verbatim prefix lets a path name as a file.
#[cfg(any(windows, test))]
fn is_device_name(name: &str) -> bool {
    const DEVICES: [&str; 22] = [
        "AUX", "NUL", "PRN", "CON", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = match name.rfind('.') {
        Some(0) | None => name,
        Some(dot) => &name[..dot],
    };
    let stem = stem.trim_end_matches([' ', '.']);
    DEVICES
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
}

/// `path` with a leading drive letter uppercased.
#[cfg(any(windows, test))]
fn upper_drive(path: &str) -> String {
    let mut out = path.to_string();
    let bytes = out.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        out[..1].make_ascii_uppercase();
    }
    out
}

/// A set of named source files embedded in the binary, addressable from
/// WCL via the angle-bracket system import: `import <wdoc/core.wcl>`.
///
/// Register files under registry-relative keys, then turn the registry
/// into a [`FileLoader`] with [`Registry::loader`]. The resulting loader
/// serves `<wcl-system>`-rooted virtual paths (the form
/// `resolve_import_path_kind` produces for system
/// imports) out of the registry and delegates every other path to a
/// fallback loader (usually [`disk_loader`]).
///
/// ```
/// # use wcl_lang::{Registry, disk_loader, Document, Environment};
/// let mut reg = Registry::new();
/// reg.register("lib/prelude.wcl", "@schemaless\nanswer = 42");
/// let loader = reg.loader(disk_loader());
/// let doc = Document::open_at_with_loader(
///     "import <lib/prelude.wcl>",
///     "<doc>",
///     None,
///     &Environment::new(),
///     loader,
/// )
/// .unwrap();
/// assert!(doc.get("answer").is_some());
/// ```
#[derive(Default, Clone)]
pub struct Registry {
    /// Registered sources by import path. `Cow` so a host can register
    /// embedded `&'static str` schemas without copying them.
    files: HashMap<String, Cow<'static, str>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `content` under the registry-relative `name` (e.g.
    /// `"wdoc/core.wcl"`). A later registration under the same name wins.
    pub fn register(&mut self, name: impl Into<String>, content: impl Into<Cow<'static, str>>) {
        self.files.insert(name.into(), content.into());
    }

    /// `true` if no files have been registered.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The content registered under `name`, if any. For a caller that reads a
    /// registered file directly rather than through a [`FileLoader`] — the
    /// same lookup [`loader`] does for a system import.
    ///
    /// [`loader`]: Registry::loader
    pub fn get(&self, name: &str) -> Option<&str> {
        self.files.get(name).map(|c| c.as_ref())
    }

    /// Every registered `(name, content)` pair, in unspecified order. Lets a
    /// library that embeds a stdlib check its own files (they parse, their
    /// imports resolve) without a second list to keep in step.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files.iter().map(|(k, v)| (k.as_str(), v.as_ref()))
    }

    /// Fold `other`'s files into this registry, `other` winning on a shared
    /// name — the same "later registration wins" rule [`register`] follows.
    /// This is how one embedded stdlib layers onto another.
    ///
    /// [`register`]: Registry::register
    pub fn extend(&mut self, other: Registry) {
        self.files.extend(other.files);
    }

    /// Build a [`FileLoader`] that serves registered files for system
    /// imports and delegates everything else to `fallback`. A system
    /// import naming an unregistered file fails with a `NotFound` error.
    pub fn loader(self, fallback: FileLoader) -> FileLoader {
        let files = self.files;
        Arc::new(move |p: &Path| {
            if let Ok(rel) = p.strip_prefix(SYSTEM_IMPORT_ROOT) {
                // Registry names always use forward slashes, but the
                // resolved import path arrives with platform separators
                // (backslashes on Windows); normalise before lookup.
                let key = rel.to_string_lossy().replace('\\', "/");
                return files
                    .get(key.as_str())
                    .map(|c| c.clone().into_owned())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("no system import registered for <{key}>"),
                        )
                    });
            }
            fallback(p)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::windows_path_key as key;

    #[test]
    fn a_verbatim_drive_path_takes_its_plain_spelling() {
        assert_eq!(key(r"\\?\C:\Users\me\a.wcl"), r"C:\Users\me\a.wcl");
        assert_eq!(key(r"\\?\c:\x"), r"C:\x");
        assert_eq!(key(r"\\?\C:\"), r"C:\");
    }

    #[test]
    fn a_verbatim_share_path_takes_its_unc_spelling() {
        assert_eq!(key(r"\\?\UNC\h\s\a"), r"\\h\s\a");
        assert_eq!(
            key(r"\\?\unc\server\share\dir\a.wcl"),
            r"\\server\share\dir\a.wcl"
        );
        assert_eq!(key(r"\\?\UNC\h\s"), r"\\h\s");
    }

    #[test]
    fn a_plain_path_gets_an_uppercase_drive_and_backslashes() {
        assert_eq!(key("c:/x"), r"C:\x");
        assert_eq!(key(r"c:\x/y.wcl"), r"C:\x\y.wcl");
        assert_eq!(key("//h/s/a"), r"\\h\s\a");
        assert_eq!(key(r"\\h\s\a"), r"\\h\s\a");
    }

    #[test]
    fn nothing_but_the_drive_letter_changes_case() {
        assert_eq!(key(r"c:\Users\ME\Main.WCL"), r"C:\Users\ME\Main.WCL");
        assert_eq!(key(r"\\?\UNC\Host\Share\A"), r"\\Host\Share\A");
    }

    #[test]
    fn a_verbatim_path_the_plain_form_would_misread_keeps_its_prefix() {
        // Each of these names a different file, or none, without `\\?\`.
        for path in [
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"\\?\C:\trailing.",
            r"\\?\C:\trailing \x",
            r"\\?\C:\dir\CON",
            r"\\?\C:\dir\nul.txt",
            r"\\?\C:\dir\com1 ",
            r"\\?\C:\a\\b",
            r"\\?\C:\a/b",
            r"\\?\UNC\h",
            r"\\?\UNC\h\s\..\t",
            r"\\?\Volume{0000}\x",
        ] {
            assert_eq!(key(path), path, "{path}");
        }
        // Still keyed with an uppercase drive.
        assert_eq!(key(r"\\?\c:\a\..\b"), r"\\?\C:\a\..\b");
    }

    #[test]
    fn a_verbatim_path_over_the_length_limit_keeps_its_prefix() {
        let long = format!(r"\\?\C:\{}\{}", "a".repeat(200), "b".repeat(60));
        assert_eq!(key(&long), long);
        let fits = format!(r"\\?\C:\{}\{}", "a".repeat(200), "b".repeat(40));
        assert_eq!(key(&fits), &fits[4..]);
    }

    #[test]
    fn names_that_only_look_like_devices_are_plain() {
        for name in ["CONSOLE", "com10", "nullable.txt", "AUXX"] {
            let path = format!(r"\\?\C:\{name}");
            assert_eq!(key(&path), &path[4..], "{name}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_every_spelling_of_a_path_shares_its_key() {
        use std::path::Path;
        let key = |path: &str| super::path_key(Path::new(path)).into_os_string();
        assert_eq!(key(r"\\?\UNC\h\s\a"), r"\\h\s\a");
        assert_eq!(key(r"\\?\c:\x"), r"C:\x");
        assert_eq!(key("c:/x"), r"C:\x");
        let dir = tempfile::tempdir().unwrap();
        let canonical = super::canonical_path(dir.path()).unwrap();
        assert!(
            !canonical.to_str().unwrap().starts_with(r"\\?\"),
            "{canonical:?}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_every_path_is_its_own_key() {
        use std::path::Path;
        for path in [r"\\?\C:\x", "c:/x", "/tmp/a//b"] {
            assert_eq!(super::path_key(Path::new(path)), Path::new(path));
        }
    }
}
