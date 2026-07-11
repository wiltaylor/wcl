//! Review comments stored in a `comments.wcl` sidecar.
//!
//! The `wcl editor` preview pane lets a reviewer click a rendered block (or
//! the page) and leave a note. Rather than touch the document source, every
//! note persists into a **`comments.wcl` sidecar** keyed by the page name + a
//! positional block locator. Watchers ignore `comments.wcl`, so a comment
//! writes only the sidecar and the page re-shows it client-side — **no
//! document rebuild**.
//!
//! The sidecar is a small, schemaless WCL file — read leniently off the AST
//! (like `wcl init`'s answer files), so it needs no `import` / `@document`
//! membership and survives hand edits. Each write **regenerates the whole file**
//! deterministically, which makes add / edit / resolve trivial and makes a
//! "generated" comment just a matter of emitting the same `comment { … }` shape:
//!
//! ```wcl
//! comment {
//!   id = "c12ab3"
//!   page = "concept_records"   // page name the comment is on
//!   loc = "0/2/1"              // block locator within the page (absent = whole page)
//!   target = "card — \"3rd\""  // human description of the block (optional)
//!   body = "tighten this"
//!   author = "wil"            // optional
//!   quote = "highlighted"     // optional
//! }
//! ```
//!
//! One sidecar lives beside each wskill's `wskill.wcl` (or, for a page not
//! inside any wskill, beside the served root document) — see [`comments_path`].
//!
//! `wcl_wdoc` carries no serde dependency, so [`CommentRecord`] is a plain
//! data struct; the `wcl` crate turns it into JSON for the CLI / dev server.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use miette::Report;
use wcl_lang::{Document, Value, ast, parse_for_edit};

use crate::build::BuildError;

/// How deep the sidecar tree-scan recurses (a runaway-loop backstop).
const MAX_SCAN_DEPTH: usize = 32;

/// Which shape a stored comment takes — derived from whether it has a `loc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentScope {
    /// Targets a specific block within the page (`loc` set).
    Block,
    /// Targets the page as a whole (no `loc`).
    Page,
}

impl CommentScope {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentScope::Block => "block",
            CommentScope::Page => "page",
        }
    }
}

/// One `comment { … }` record read from a `comments.wcl` sidecar, with
/// everything an agent (or the dev-server client) needs to show it, place its
/// pin, and relocate it for `resolve`.
#[derive(Debug, Clone)]
pub struct CommentRecord {
    pub id: String,
    pub scope: CommentScope,
    /// The `comments.wcl` file that holds this record (where `resolve` edits).
    pub file: PathBuf,
    /// The page name the comment is on.
    pub page: String,
    /// The source file the page is defined in. Together with `page` it
    /// disambiguates same-named pages across different sites / wskills.
    pub page_file: Option<String>,
    /// Positional locator of the targeted block within the page; `None` for a
    /// whole-page comment.
    pub loc: Option<String>,
    /// Human description of the targeted block.
    pub target: Option<String>,
    /// Exact text the reviewer had highlighted when commenting.
    pub quote: Option<String>,
    pub body: String,
    pub author: Option<String>,
    pub status: Option<String>,
}

/// The `comments.wcl` that owns comments for a page defined in `page_file`: the
/// nearest ancestor directory (within `root`) that holds a `wskill.wcl`, else
/// `root/comments.wcl`. Walking from the page's *source* file means a generated
/// wskill page (whose source lives under `…/wdoc/book/`) still resolves to the
/// `comments.wcl` beside that wskill's `wskill.wcl`.
pub fn comments_path(page_file: &Path, root: &Path) -> PathBuf {
    let root_canon = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut cur = page_file.parent().and_then(|d| fs::canonicalize(d).ok());
    while let Some(dir) = cur {
        // Only directories within the served root are candidates.
        if !dir.starts_with(&root_canon) {
            break;
        }
        if dir.join("wskill.wcl").is_file() {
            return dir.join("comments.wcl");
        }
        if dir == root_canon {
            break;
        }
        cur = dir.parent().map(Path::to_path_buf);
    }
    root_canon.join("comments.wcl")
}

/// List every comment stored in any `comments.wcl` under `root` (so a server
/// rooted at the top `docs/` finds every wskill's sidecar plus the root one).
pub fn list(root: &Path) -> Result<Vec<CommentRecord>, BuildError> {
    let mut out = Vec::new();
    scan(root, &mut out, 0);
    Ok(out)
}

/// Recurse `dir` for files named `comments.wcl`, reading each. Hidden (`.`) and
/// generated (`_site` / `_wdoc`, any `_`-prefixed) directories are skipped.
fn scan(dir: &Path, out: &mut Vec<CommentRecord>, depth: usize) {
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
                scan(&path, out, depth + 1);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some("comments.wcl") {
            out.extend(read_file(&path));
        }
    }
}

/// Parse every `comment { … }` block out of one `comments.wcl`, reading each
/// field's literal value off the AST (no `@document` schema needed — same
/// bypass `wcl init` uses for answer files). A malformed / missing file yields
/// no records rather than erroring (a sidecar is non-critical metadata).
fn read_file(path: &Path) -> Vec<CommentRecord> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(parsed) = parse_for_edit(&text, path.display().to_string()) else {
        return Vec::new();
    };
    // A scratch document supplies the evaluation context (literals need none).
    let Ok(scratch) = Document::open("", "<comments>") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &parsed.items {
        let ast::Item::Block(b) = item else { continue };
        if b.kind != "comment" {
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
        let Some(id) = fields.remove("id") else {
            continue;
        };
        let loc = fields.remove("loc").filter(|s| !s.is_empty());
        out.push(CommentRecord {
            scope: if loc.is_some() {
                CommentScope::Block
            } else {
                CommentScope::Page
            },
            id,
            file: path.to_path_buf(),
            page: fields.remove("page").unwrap_or_default(),
            page_file: fields.remove("page_file"),
            loc,
            target: fields.remove("target"),
            quote: fields.remove("quote"),
            body: fields.remove("body").unwrap_or_default(),
            author: fields.remove("author"),
            status: fields.remove("status"),
        });
    }
    out
}

/// Stringify a scalar field value; non-scalars are skipped.
fn value_string(v: &Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        Value::Symbol(s) | Value::Identifier(s) => Some(s.clone()),
        _ => None,
    }
}

/// Append a comment to `comments_file` (creating it if absent) and return the
/// generated id. `loc` empty / `None` ⇒ a whole-page comment.
#[allow(clippy::too_many_arguments)]
pub fn add(
    comments_file: &Path,
    page: &str,
    page_file: Option<&str>,
    loc: Option<&str>,
    target: Option<&str>,
    body: &str,
    author: Option<&str>,
    quote: Option<&str>,
) -> Result<String, BuildError> {
    let id = gen_id();
    let loc = loc.map(str::to_string).filter(|s| !s.is_empty());
    let mut recs = read_file(comments_file);
    recs.push(CommentRecord {
        scope: if loc.is_some() {
            CommentScope::Block
        } else {
            CommentScope::Page
        },
        id: id.clone(),
        file: comments_file.to_path_buf(),
        page: page.to_string(),
        page_file: page_file.map(str::to_string),
        loc,
        target: target.map(str::to_string),
        quote: quote.map(str::to_string),
        body: body.to_string(),
        author: author.map(str::to_string),
        status: None,
    });
    write_file(comments_file, &recs)?;
    Ok(id)
}

/// Delete the comment with `id` from whichever sidecar under `root` holds it.
/// Returns `true` if one was removed.
pub fn resolve(root: &Path, id: &str) -> Result<bool, BuildError> {
    let Some(rec) = list(root)?.into_iter().find(|r| r.id == id) else {
        return Ok(false);
    };
    let mut recs = read_file(&rec.file);
    let before = recs.len();
    recs.retain(|r| r.id != id);
    if recs.len() == before {
        return Ok(false);
    }
    write_file(&rec.file, &recs)?;
    Ok(true)
}

/// Replace the `body` of the comment with `id`, preserving its other fields.
/// Returns `true` if one was edited.
pub fn edit(root: &Path, id: &str, body: &str) -> Result<bool, BuildError> {
    let Some(rec) = list(root)?.into_iter().find(|r| r.id == id) else {
        return Ok(false);
    };
    let mut recs = read_file(&rec.file);
    let mut found = false;
    for r in &mut recs {
        if r.id == id {
            r.body = body.to_string();
            found = true;
        }
    }
    if !found {
        return Ok(false);
    }
    write_file(&rec.file, &recs)?;
    Ok(true)
}

/// Regenerate `path` from `recs` (deterministic field order), verify it
/// re-parses, then write it atomically.
fn write_file(path: &Path, recs: &[CommentRecord]) -> Result<(), BuildError> {
    let mut out = String::from(
        "// Review comments for this wskill / doc — written from the `wcl editor`\n\
         // preview pane and read back by `wcl wdoc comments`. Each `comment` block is\n\
         // keyed by page name + block locator; safe to hand-edit or generate.\n\n",
    );
    for r in recs {
        out.push_str("comment {\n");
        out.push_str(&format!("  id = {}\n", wcl_string(&r.id)));
        out.push_str(&format!("  page = {}\n", wcl_string(&r.page)));
        if let Some(pf) = &r.page_file {
            out.push_str(&format!("  page_file = {}\n", wcl_string(pf)));
        }
        if let Some(loc) = &r.loc {
            out.push_str(&format!("  loc = {}\n", wcl_string(loc)));
        }
        if let Some(t) = &r.target {
            out.push_str(&format!("  target = {}\n", wcl_string(t)));
        }
        out.push_str(&format!("  body = {}\n", wcl_string(&r.body)));
        if let Some(a) = &r.author {
            out.push_str(&format!("  author = {}\n", wcl_string(a)));
        }
        if let Some(s) = &r.status {
            out.push_str(&format!("  status = {}\n", wcl_string(s)));
        }
        if let Some(q) = &r.quote {
            out.push_str(&format!("  quote = {}\n", wcl_string(q)));
        }
        out.push_str("}\n\n");
    }
    parse_for_edit(&out, "<comments output>").map_err(|e| BuildError::Parse(Report::new(e)))?;
    atomic_write(path, &out).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))
}

/// Render `s` as a double-quoted WCL string literal (mirrors the language's
/// own `EscapeString`).
fn wcl_string(s: &str) -> String {
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

/// A short, unique-enough comment id: time-mixed with a process counter.
fn gen_id() -> String {
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
    format!("c{tail}")
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
fn atomic_write(target: &Path, contents: &str) -> std::io::Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".wcl-comment-{pid}-{stamp}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unwrap a `Result<T, BuildError>` (BuildError isn't `Debug`).
    fn ok<T>(r: Result<T, BuildError>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("{}", e.render_plain()),
        }
    }

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = base.join(format!("wcl-comment-test-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn add_then_list_then_resolve_round_trips() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");

        let id = ok(add(
            &file,
            "home",
            Some("book/main.wcl"),
            Some("0/2/1"),
            Some("card — \"3rd\""),
            "tighten this",
            Some("wil"),
            Some("Hi"),
        ));

        let recs = ok(list(&dir));
        let rec = recs.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.scope, CommentScope::Block);
        assert_eq!(rec.page, "home");
        assert_eq!(rec.page_file.as_deref(), Some("book/main.wcl"));
        assert_eq!(rec.loc.as_deref(), Some("0/2/1"));
        assert_eq!(rec.target.as_deref(), Some("card — \"3rd\""));
        assert_eq!(rec.body, "tighten this");
        assert_eq!(rec.author.as_deref(), Some("wil"));
        assert_eq!(rec.quote.as_deref(), Some("Hi"));

        assert!(ok(resolve(&dir, &id)));
        assert!(ok(list(&dir)).iter().all(|r| r.id != id));
        // Resolving an unknown id is a no-op.
        assert!(!ok(resolve(&dir, "nope")));
    }

    #[test]
    fn loc_distinguishes_block_and_page() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");
        let block = ok(add(
            &file,
            "home",
            None,
            Some("0/2"),
            Some("p"),
            "b",
            None,
            None,
        ));
        let page = ok(add(
            &file,
            "home",
            None,
            None,
            None,
            "whole page",
            None,
            None,
        ));

        let recs = ok(list(&dir));
        assert_eq!(
            recs.iter().find(|r| r.id == block).unwrap().scope,
            CommentScope::Block
        );
        let p = recs.iter().find(|r| r.id == page).unwrap();
        assert_eq!(p.scope, CommentScope::Page);
        assert!(p.loc.is_none());
    }

    #[test]
    fn edit_changes_body_and_preserves_other_fields() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");
        let id = ok(add(
            &file,
            "home",
            None,
            Some("0/1"),
            Some("2nd p"),
            "old body",
            Some("wil"),
            Some("a quote"),
        ));

        assert!(ok(edit(&dir, &id, "new body")));

        let recs = ok(list(&dir));
        let rec = recs.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.body, "new body");
        assert_eq!(rec.author.as_deref(), Some("wil"));
        assert_eq!(rec.loc.as_deref(), Some("0/1"));
        assert_eq!(rec.target.as_deref(), Some("2nd p"));
        assert_eq!(rec.quote.as_deref(), Some("a quote"));
        assert_eq!(rec.page, "home");

        assert!(!ok(edit(&dir, "nope", "x")));
    }

    #[test]
    fn body_with_quotes_and_newlines_survives() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");
        let tricky = "say \"hi\"\nand more\\stuff";
        let id = ok(add(&file, "home", None, None, None, tricky, None, None));
        let recs = ok(list(&dir));
        assert_eq!(recs.iter().find(|r| r.id == id).unwrap().body, tricky);
    }

    #[test]
    fn comments_path_walks_up_to_wskill_else_root() {
        let root = tempdir();
        // A wskill at root/wskills/x with a generated page under wdoc/book/.
        let wskill = root.join("wskills").join("x");
        let book = wskill.join("wdoc").join("book");
        fs::create_dir_all(&book).unwrap();
        fs::write(wskill.join("wskill.wcl"), "").unwrap();
        let page_file = book.join("main.wcl");
        fs::write(&page_file, "").unwrap();
        assert_eq!(
            comments_path(&page_file, &root),
            wskill.join("comments.wcl")
        );

        // A page not inside any wskill falls back to root/comments.wcl.
        let pages = root.join("pages");
        fs::create_dir_all(&pages).unwrap();
        let plain = pages.join("index.wcl");
        fs::write(&plain, "").unwrap();
        let want = fs::canonicalize(&root).unwrap().join("comments.wcl");
        assert_eq!(comments_path(&plain, &root), want);
    }
}
