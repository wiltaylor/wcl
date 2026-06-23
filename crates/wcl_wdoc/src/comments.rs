//! Review comments stored as `@comment` decorators in wdoc source.
//!
//! `wcl wdoc serve --comment` lets a reviewer click a rendered block (or the
//! page) and leave a note; the dev server persists it here by splicing a
//! `@comment(...)` decorator onto a *real, editable* source block. The write is
//! **surgical** — the decorator text is inserted at (or removed from) the
//! block's byte offset and the rest of the file is left untouched, so the
//! author's formatting is preserved and a `format::to_source` round-trip quirk
//! elsewhere in the file can't break the write. `wcl wdoc comments` reads them
//! back ([`list`]) for an AI agent, which acts on them and then [`resolve`]s
//! (deletes) each by its stable `id`.
//!
//! Three comment shapes, all decorators on an editable source block:
//!   - **direct** — on a hand-authored block (it has a source span);
//!   - **page-attached** — on the enclosing `page`, with `loc` + `target`
//!     describing a generated (repeater/component) block that has no span;
//!   - **generic page** — on the `page`, no `loc`.
//!
//! `wcl_wdoc` carries no serde dependency, so [`CommentRecord`] is a plain
//! data struct; the `wcl` crate turns it into JSON for the CLI / dev server.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use miette::Report;
use wcl_lang::{Document, Span, Value, ast, disk_loader, parse_for_edit};

use crate::build::{BuildError, schema_registry, wdoc_environment};
use crate::include::{IncludeSpec, MAX_INCLUDE_DEPTH, resolve_included};
use crate::render::{field_utf8, label_string};

/// Which of the three shapes a stored comment takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentScope {
    /// Decorator written directly onto a hand-authored block.
    Block,
    /// Decorator on the page, targeting a generated block via `loc`/`target`.
    PageBlock,
    /// Decorator on the page, about the page as a whole (no `loc`).
    Page,
}

impl CommentScope {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentScope::Block => "block",
            CommentScope::PageBlock => "page-block",
            CommentScope::Page => "page",
        }
    }
}

/// One `@comment` decorator found in the source, with everything an agent (or
/// the dev-server client) needs to act on it and to relocate it for `resolve`.
#[derive(Debug, Clone)]
pub struct CommentRecord {
    pub id: String,
    pub scope: CommentScope,
    /// The source file that carries the decorator (where `resolve` edits).
    pub file: PathBuf,
    /// Enclosing page name (the page the comment lives on / under).
    pub page: Option<String>,
    /// Kind of the block carrying the decorator (`page` for page-attached).
    pub host_kind: String,
    /// Inline label of the host block, if any.
    pub host_label: Option<String>,
    /// Positional locator of the targeted generated block (page-attached only).
    pub loc: Option<String>,
    /// Auto description of the targeted generated block (page-attached only).
    pub target: Option<String>,
    /// Exact text the reviewer had highlighted when commenting.
    pub quote: Option<String>,
    pub body: String,
    pub author: Option<String>,
    pub status: Option<String>,
    /// Span of the host block in `file` — the handle `resolve` matches on.
    pub span_start: usize,
    pub span_end: usize,
}

/// Open the wdoc document the same way the build does, so imports + the
/// embedded stdlib (and therefore the `@comment` decorator schema) resolve.
fn open_doc(file: &Path) -> Result<Document, BuildError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    let name = file.display().to_string();
    let base_dir = file.parent().map(Path::to_path_buf);
    let loader = schema_registry().loader(disk_loader());
    Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &wdoc_environment(base_dir.as_deref()),
        loader,
    )
    .map_err(|e| BuildError::Parse(Report::new(e)))
}

/// List every `@comment` in the document, its imports, and every `include`d
/// sub-site (so comments left on an included book — e.g. a wskill served under
/// the top-level `docs/main.wcl` — are found from the top).
pub fn list(file: &Path, _site_filter: Option<&str>) -> Result<Vec<CommentRecord>, BuildError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    list_into(file, &mut out, &mut seen, 0)?;
    Ok(out)
}

/// Scan `file` for comments, then recurse into each `include`d sub-site. `seen`
/// (canonical paths) breaks include cycles; `depth` bounds nesting.
fn list_into(
    file: &Path,
    out: &mut Vec<CommentRecord>,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<(), BuildError> {
    if depth > MAX_INCLUDE_DEPTH {
        return Ok(());
    }
    let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    if !seen.insert(canon) {
        return Ok(());
    }
    let doc = open_doc(file)?;
    // A block and its nested children always live in one file; the source for
    // each top-level block is carried down to its descendants.
    for (src, b) in doc.blocks_with_source() {
        let bfile = src.unwrap_or(file);
        collect_block(&b, None, bfile, out);
    }
    // Gather `include` specs (owned) so the doc borrow ends before we recurse.
    let specs: Vec<IncludeSpec> = doc
        .blocks()
        .filter(|b| b.kind() == "include")
        .filter_map(|b| {
            Some(IncludeSpec {
                folder: label_string(&b)?,
                pattern: field_utf8(&b, "pattern"),
                entry: field_utf8(&b, "entry"),
                site: field_utf8(&b, "site"),
            })
        })
        .collect();
    let base_dir = file.parent().map(Path::to_path_buf);
    drop(doc);
    for spec in &specs {
        // A missing/unscannable include folder is not fatal to listing.
        for s in resolve_included(base_dir.as_deref(), spec).unwrap_or_default() {
            list_into(&s.src_path, out, seen, depth + 1)?;
        }
    }
    Ok(())
}

/// Recurse a block, recording any `@comment` decorators it carries, then its
/// children. `page` is the enclosing page name carried down the tree; `file` is
/// the source file this subtree was parsed from.
fn collect_block(
    b: &wcl_lang::Block<'_>,
    page: Option<&str>,
    file: &Path,
    out: &mut Vec<CommentRecord>,
) {
    // Synthetic / repeater-expanded instances share a source span with their
    // template; never read comments off them.
    if b.binding_scope_depth() > 0 {
        return;
    }
    let kind = b.kind();
    let is_page = kind == "page";
    let label = label_string(b);
    let this_page: Option<String> = if is_page {
        label.clone()
    } else {
        page.map(str::to_string)
    };
    let file: PathBuf = file.to_path_buf();
    let span = b.span();

    for d in b.decorators() {
        if d.name() != "comment" {
            continue;
        }
        let body = dec_str(&d, "body").unwrap_or_default();
        let id = dec_str(&d, "id").unwrap_or_default();
        let loc = dec_str(&d, "loc");
        let scope = if is_page {
            if loc.is_some() {
                CommentScope::PageBlock
            } else {
                CommentScope::Page
            }
        } else {
            CommentScope::Block
        };
        out.push(CommentRecord {
            id,
            scope,
            file: file.clone(),
            // The page the comment was authored on: the decorator's recorded
            // `page` (the resolved render-time name — set for generated pages
            // whose source label can't resolve statically) wins over the
            // statically-walked enclosing page.
            page: dec_str(&d, "page").or_else(|| this_page.clone()),
            host_kind: kind.to_string(),
            host_label: label.clone(),
            loc,
            target: dec_str(&d, "target"),
            quote: dec_str(&d, "quote"),
            body,
            author: dec_str(&d, "author"),
            status: dec_str(&d, "status"),
            span_start: span.start,
            span_end: span.end,
        });
    }

    for child in b.blocks() {
        collect_block(&child, this_page.as_deref(), &file, out);
    }
}

/// Read a string-valued named arg off a decorator view.
fn dec_str(d: &wcl_lang::Decorator<'_>, name: &str) -> Option<String> {
    match d.named_arg(name)? {
        Ok(Value::Utf8(s) | Value::Ascii(s)) => Some(s),
        Ok(Value::Symbol(s) | Value::Identifier(s)) => Some(s),
        _ => None,
    }
}

/// Write a `@comment` directly onto a hand-authored block (the `span` block in
/// `file`). Returns the generated comment id.
pub fn add_to_block(
    file: &Path,
    span: Span,
    body: &str,
    author: Option<&str>,
    quote: Option<&str>,
    page: Option<&str>,
) -> Result<String, BuildError> {
    let id = gen_id();
    let text = comment_decorator_text(&id, body, author, None, None, quote, page);
    insert_decorator(file, span, &text)?;
    Ok(id)
}

/// Write a `@comment` onto the `page` block (the `page_span` block in
/// `page_file`). With `loc`/`target` it is a page-attached block comment; with
/// neither it is a generic page comment. Returns the generated comment id.
#[allow(clippy::too_many_arguments)]
pub fn add_to_page(
    page_file: &Path,
    page_span: Span,
    body: &str,
    author: Option<&str>,
    loc: Option<&str>,
    target: Option<&str>,
    quote: Option<&str>,
    page: Option<&str>,
) -> Result<String, BuildError> {
    let id = gen_id();
    let text = comment_decorator_text(&id, body, author, loc, target, quote, page);
    insert_decorator(page_file, page_span, &text)?;
    Ok(id)
}

/// Delete the comment with the given `id`. Returns `true` if one was removed.
pub fn resolve(file: &Path, site_filter: Option<&str>, id: &str) -> Result<bool, BuildError> {
    let Some(rec) = list(file, site_filter)?.into_iter().find(|r| r.id == id) else {
        return Ok(false);
    };
    let span = Span::new(rec.span_start, rec.span_end);
    remove_decorator(&rec.file, span, id)
}

/// Replace the `body` of the comment with `id`, preserving its other fields
/// (author / loc / target / quote). Returns `true` if one was edited.
pub fn edit(
    file: &Path,
    site_filter: Option<&str>,
    id: &str,
    body: &str,
) -> Result<bool, BuildError> {
    let Some(rec) = list(file, site_filter)?.into_iter().find(|r| r.id == id) else {
        return Ok(false);
    };
    let new_text = comment_decorator_text(
        &rec.id,
        body,
        rec.author.as_deref(),
        rec.loc.as_deref(),
        rec.target.as_deref(),
        rec.quote.as_deref(),
        rec.page.as_deref(),
    );
    let span = Span::new(rec.span_start, rec.span_end);
    replace_decorator(&rec.file, span, id, &new_text)
}

/// Read the `id` named arg off a raw decorator AST (string literal only).
fn decorator_comment_id(d: &ast::Decorator) -> Option<String> {
    if d.name.last().map(String::as_str) != Some("comment") {
        return None;
    }
    d.named
        .iter()
        .find(|n| n.name == "id")
        .and_then(|n| match &n.value {
            ast::Expr::Utf8(s) | ast::Expr::Ascii(s) => Some(s.clone()),
            _ => None,
        })
}

/// Splice `decorator_text` into `file` immediately before the block at `span`,
/// leaving the rest of the file byte-for-byte unchanged. This is a *surgical*
/// edit — no `format::to_source` round-trip — so it preserves the author's
/// formatting and can't be defeated by a formatter quirk elsewhere in the file
/// (e.g. a multi-line interpolation slot the printer can't reproduce).
fn insert_decorator(file: &Path, span: Span, decorator_text: &str) -> Result<(), BuildError> {
    let src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    // Confirm a block really begins at `span` before we splice at its offset.
    let mut source = parse_for_edit(&src, file.display().to_string())
        .map_err(|e| BuildError::Parse(Report::new(e)))?;
    if find_block_by_span(&mut source.items, span).is_none() {
        return Err(BuildError::BadPage(format!(
            "could not locate block at {}..{} in {}",
            span.start,
            span.end,
            file.display()
        )));
    }
    let at = span.start;
    let mut out = String::with_capacity(src.len() + decorator_text.len() + 1);
    out.push_str(&src[..at]);
    out.push_str(decorator_text);
    out.push(' ');
    out.push_str(&src[at..]);
    verify_and_write(file, &out)
}

/// Remove the `@comment` carrying `id` from the block at `span` in `file`,
/// surgically deleting just the decorator text (and the whitespace up to the
/// kind) and leaving the rest of the file untouched.
fn remove_decorator(file: &Path, span: Span, id: &str) -> Result<bool, BuildError> {
    let src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    let mut source = parse_for_edit(&src, file.display().to_string())
        .map_err(|e| BuildError::Parse(Report::new(e)))?;
    let Some(block) = find_block_by_span(&mut source.items, span) else {
        return Ok(false);
    };
    let Some(dec) = block
        .decorators
        .iter()
        .find(|d| decorator_comment_id(d).as_deref() == Some(id))
    else {
        return Ok(false);
    };
    let start = dec.span.start;
    let mut end = dec.span.end;
    // Swallow the horizontal whitespace the decorator left before the kind.
    let bytes = src.as_bytes();
    while end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
        end += 1;
    }
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..start]);
    out.push_str(&src[end..]);
    verify_and_write(file, &out)?;
    Ok(true)
}

/// Replace the `@comment` carrying `id` on the block at `span` in `file` with
/// `new_text`, surgically swapping just the decorator's source range.
fn replace_decorator(
    file: &Path,
    span: Span,
    id: &str,
    new_text: &str,
) -> Result<bool, BuildError> {
    let src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    let mut source = parse_for_edit(&src, file.display().to_string())
        .map_err(|e| BuildError::Parse(Report::new(e)))?;
    let Some(block) = find_block_by_span(&mut source.items, span) else {
        return Ok(false);
    };
    let Some(dec) = block
        .decorators
        .iter()
        .find(|d| decorator_comment_id(d).as_deref() == Some(id))
    else {
        return Ok(false);
    };
    let (start, end) = (dec.span.start, dec.span.end);
    let mut out = String::with_capacity(src.len() + new_text.len());
    out.push_str(&src[..start]);
    out.push_str(new_text);
    out.push_str(&src[end..]);
    verify_and_write(file, &out)?;
    Ok(true)
}

/// Re-parse `out` (refusing to write anything that no longer parses) and
/// atomically replace `file` with it.
fn verify_and_write(file: &Path, out: &str) -> Result<(), BuildError> {
    parse_for_edit(out, "<comment output>").map_err(|e| BuildError::Parse(Report::new(e)))?;
    atomic_write(file, out).map_err(|e| BuildError::Io(e, format!("write {}", file.display())))
}

/// Walk `items` (recursing into nested blocks) for the block whose span
/// matches. Span equality holds because we re-parse the same source bytes the
/// document was parsed from (same rationale as `wcl set`).
fn find_block_by_span(items: &mut [ast::Item], span: Span) -> Option<&mut ast::Block> {
    for item in items {
        if let ast::Item::Block(b) = item {
            if b.span == span {
                return Some(b);
            }
            if let Some(found) = find_block_by_span(&mut b.items, span) {
                return Some(found);
            }
        }
    }
    None
}

/// Build the `@comment(...)` decorator source text. String values are escaped
/// as WCL string literals, so the result re-parses cleanly when spliced in.
#[allow(clippy::too_many_arguments)]
fn comment_decorator_text(
    id: &str,
    body: &str,
    author: Option<&str>,
    loc: Option<&str>,
    target: Option<&str>,
    quote: Option<&str>,
    page: Option<&str>,
) -> String {
    let mut parts = vec![
        format!("id = {}", wcl_string(id)),
        format!("body = {}", wcl_string(body)),
    ];
    for (name, val) in [
        ("author", author),
        ("loc", loc),
        ("target", target),
        ("quote", quote),
        ("page", page),
    ] {
        if let Some(v) = val {
            parts.push(format!("{name} = {}", wcl_string(v)));
        }
    }
    format!("@comment({})", parts.join(", "))
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
/// interrupted write never leaves a half-written source file.
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

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    const DOC: &str = "import <wdoc.wcl>\n\nsite s { default_template = :webpage title = \"S\" root = true }\n\npage home {\n  heading \"Hi\" { level = 1 }\n}\n";

    #[test]
    fn add_to_block_then_list_then_resolve_round_trips() {
        let dir = tempdir();
        let file = write(&dir, "main.wcl", DOC);

        // The heading block's span — read it via the document view.
        let span = {
            let doc = ok(open_doc(&file));
            let page = doc.blocks().find(|b| b.kind() == "page").unwrap();
            let heading = page.blocks().find(|b| b.kind() == "heading").unwrap();
            heading.span()
        };

        let id = ok(add_to_block(
            &file,
            span,
            "tighten this",
            Some("wil"),
            Some("Hi"),
            Some("home"),
        ));

        let recs = ok(list(&file, None));
        let rec = recs.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.scope, CommentScope::Block);
        assert_eq!(rec.body, "tighten this");
        assert_eq!(rec.author.as_deref(), Some("wil"));
        assert_eq!(rec.quote.as_deref(), Some("Hi"));
        assert_eq!(rec.host_kind, "heading");
        // The recorded page (the resolved render-time name) round-trips.
        assert_eq!(rec.page.as_deref(), Some("home"));

        assert!(ok(resolve(&file, None, &id)));
        assert!(ok(list(&file, None)).iter().all(|r| r.id != id));
    }

    #[test]
    fn write_is_surgical_and_resolve_restores_byte_for_byte() {
        // The single-line `site s { … }` would be reflowed by a `format::to_source`
        // round-trip; the surgical write must leave every byte but the inserted
        // decorator untouched, and resolve must restore the original exactly.
        let dir = tempdir();
        let file = write(&dir, "main.wcl", DOC);

        let span = {
            let doc = ok(open_doc(&file));
            doc.blocks().find(|b| b.kind() == "page").unwrap().span()
        };
        let id = ok(add_to_page(
            &file, span, "note", None, None, None, None, None,
        ));

        let after = std::fs::read_to_string(&file).unwrap();
        // Exactly one decorator was spliced in; the rest is identical to DOC.
        assert!(after.contains(&format!(
            "@comment(id = \"{id}\", body = \"note\") page home"
        )));
        assert_eq!(
            after.replace(&format!("@comment(id = \"{id}\", body = \"note\") "), ""),
            DOC
        );
        // The single-line site block was not reflowed.
        assert!(after.contains("site s { default_template = :webpage title = \"S\" root = true }"));

        assert!(ok(resolve(&file, None, &id)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), DOC);
    }

    #[test]
    fn edit_changes_body_and_preserves_other_fields() {
        let dir = tempdir();
        let file = write(&dir, "main.wcl", DOC);
        let span = {
            let doc = ok(open_doc(&file));
            doc.blocks().find(|b| b.kind() == "page").unwrap().span()
        };
        let id = ok(add_to_page(
            &file,
            span,
            "old body",
            Some("wil"),
            Some("0/1"),
            Some("2nd p"),
            Some("a quote"),
            Some("home"),
        ));

        assert!(ok(edit(&file, None, &id, "new body")));

        let recs = ok(list(&file, None));
        let rec = recs.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.body, "new body");
        // Everything else is untouched.
        assert_eq!(rec.author.as_deref(), Some("wil"));
        assert_eq!(rec.loc.as_deref(), Some("0/1"));
        assert_eq!(rec.target.as_deref(), Some("2nd p"));
        assert_eq!(rec.quote.as_deref(), Some("a quote"));
        assert_eq!(rec.page.as_deref(), Some("home"));

        // Editing an unknown id is a no-op.
        assert!(!ok(edit(&file, None, "nope", "x")));
    }

    #[test]
    fn add_to_page_with_and_without_loc() {
        let dir = tempdir();
        let file = write(&dir, "main.wcl", DOC);
        // Each edit shifts the page block's span, so re-derive it per write —
        // exactly what the dev server does (a fresh build before each comment).
        let page_span = |file: &Path| {
            let doc = ok(open_doc(file));
            doc.blocks().find(|b| b.kind() == "page").unwrap().span()
        };

        let block_id = ok(add_to_page(
            &file,
            page_span(&file),
            "fix card",
            None,
            Some("0/2/1"),
            Some("3rd card"),
            None,
            None,
        ));
        let generic_id = ok(add_to_page(
            &file,
            page_span(&file),
            "page-wide note",
            None,
            None,
            None,
            None,
            None,
        ));

        let recs = ok(list(&file, None));
        let block = recs.iter().find(|r| r.id == block_id).unwrap();
        assert_eq!(block.scope, CommentScope::PageBlock);
        assert_eq!(block.loc.as_deref(), Some("0/2/1"));
        assert_eq!(block.target.as_deref(), Some("3rd card"));
        let generic = recs.iter().find(|r| r.id == generic_id).unwrap();
        assert_eq!(generic.scope, CommentScope::Page);
        assert!(generic.loc.is_none());
    }

    #[test]
    fn body_with_quotes_and_newlines_survives() {
        let dir = tempdir();
        let file = write(&dir, "main.wcl", DOC);
        let span = {
            let doc = ok(open_doc(&file));
            doc.blocks().find(|b| b.kind() == "page").unwrap().span()
        };
        let tricky = "say \"hi\"\nand more\\stuff";
        let id = ok(add_to_page(
            &file, span, tricky, None, None, None, None, None,
        ));
        let recs = ok(list(&file, None));
        assert_eq!(recs.iter().find(|r| r.id == id).unwrap().body, tricky);
    }

    fn tempdir() -> PathBuf {
        // A unique scratch dir under the system temp without an extra dep.
        let base = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = base.join(format!("wcl-comment-test-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
