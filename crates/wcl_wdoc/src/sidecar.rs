//! Shared mechanics for the `.wcl` sidecar files.
//!
//! A sidecar records data *about* a document without touching its source:
//! review notes ([`crate::comments`]) and course answers ([`crate::training`]).
//! Both follow the same rules, which live here so the two stay in step:
//!
//! - the file sits beside the owning `wskill.wcl` (else the served root),
//! - it is **schemaless** — read leniently off the AST, so it needs no
//!   `import` / `@document` membership and survives hand edits,
//! - every write **regenerates the whole file** deterministically and verifies
//!   it re-parses before an atomic rename,
//! - watchers ignore it, so writing one never triggers a document rebuild.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use wcl_lang::{Document, Value, ast, parse_for_edit};

/// How deep a sidecar tree-scan recurses (a runaway-loop backstop).
const MAX_SCAN_DEPTH: usize = 32;

/// The sidecar named `name` that owns records for a page defined in
/// `page_file`: the nearest ancestor directory (within `root`) that holds a
/// `wskill.wcl`, else `root/<name>`. Walking from the page's *source* file
/// means a generated wskill page (whose source lives under `…/wdoc/book/`)
/// still resolves to the sidecar beside that wskill's `wskill.wcl`.
pub(crate) fn sidecar_path(page_file: &Path, root: &Path, name: &str) -> PathBuf {
    let root_canon = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut cur = page_file.parent().and_then(|d| fs::canonicalize(d).ok());
    while let Some(dir) = cur {
        // Only directories within the served root are candidates.
        if !dir.starts_with(&root_canon) {
            break;
        }
        if dir.join("wskill.wcl").is_file() {
            return dir.join(name);
        }
        if dir == root_canon {
            break;
        }
        cur = dir.parent().map(Path::to_path_buf);
    }
    root_canon.join(name)
}

/// The nearest directory at or above `start` that holds a `wskill.wcl` — the
/// wskill that owns whatever lives under it.
///
/// Unlike [`sidecar_path`], this walks *up* out of the starting directory,
/// because a server is often rooted below the wskill root (`wcl wdoc serve
/// wdoc/training/main.wcl` watches `wdoc/training/`, whose owner is two levels
/// up). Bounded by [`MAX_SCAN_DEPTH`] and the filesystem root; `None` when the
/// path is not inside a wskill at all.
pub(crate) fn owner_dir(start: &Path) -> Option<PathBuf> {
    let mut cur = fs::canonicalize(start).ok()?;
    for _ in 0..MAX_SCAN_DEPTH {
        if cur.join("wskill.wcl").is_file() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
    None
}

/// Every sidecar named `name` under `dir`, so a server rooted at the top
/// `docs/` finds every wskill's sidecar plus the root one. Hidden (`.`) and
/// generated (`_site` / `_wdoc`, any `_`-prefixed) directories are skipped.
pub(crate) fn scan_for(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    scan_inner(dir, name, out, 0);
}

fn scan_inner(dir: &Path, name: &str, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let skip = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.') || n.starts_with('_'));
            if !skip {
                scan_inner(&path, name, out, depth + 1);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            out.push(path);
        }
    }
}

/// Every `<kind> { … }` block in `path` as a field-name → scalar-value map,
/// read off the AST with no schema (the same bypass `wcl init` uses for answer
/// files). A malformed / missing file yields no records rather than erroring —
/// a sidecar is non-critical metadata.
pub(crate) fn read_blocks(path: &Path, kind: &str) -> Vec<BTreeMap<String, String>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(parsed) = parse_for_edit(&text, path.display().to_string()) else {
        return Vec::new();
    };
    // A scratch document supplies the evaluation context (literals need none).
    let Ok(scratch) = Document::open("", "<sidecar>") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &parsed.items {
        let ast::Item::Block(b) = item else { continue };
        if b.kind != kind {
            continue;
        }
        let mut fields: BTreeMap<String, String> = BTreeMap::new();
        for it in &b.items {
            if let ast::Item::Field(f) = it
                && let Ok(v) = scratch.eval_expr(&f.expr)
                && let Some(s) = value_string(&v)
            {
                fields.insert(f.name.clone(), s);
            }
        }
        out.push(fields);
    }
    out
}

/// Stringify a scalar field value; non-scalars are skipped.
pub(crate) fn value_string(v: &Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        Value::Symbol(s) | Value::Identifier(s) => Some(s.clone()),
        _ => None,
    }
}

/// Render `s` as a double-quoted WCL string literal (mirrors the language's
/// own `EscapeString`).
pub(crate) fn wcl_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A short, unique-enough record id under `prefix`: time-mixed with a
/// process counter.
pub(crate) fn gen_id(prefix: char) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mix = nanos
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(n.wrapping_mul(0x1_0000_0001));
    let s = base36(mix);
    let tail = &s[s.len().saturating_sub(7)..];
    format!("{prefix}{tail}")
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_string();
    }
    let mut buf = Vec::new();
    while n > 0 {
        buf.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).expect("base36 digits are ASCII")
}

/// Write `contents` to `target` via a same-directory temp file + rename, so an
/// interrupted write never leaves a half-written sidecar.
pub(crate) fn atomic_write(target: &Path, contents: &str) -> std::io::Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".wcl-sidecar-{pid}-{stamp}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, target)
}
