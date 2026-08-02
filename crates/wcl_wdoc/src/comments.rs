//! Review comments stored in a `comments.wcl` sidecar.
//!
//! The `wcl editor` preview pane lets a reviewer click a rendered block (or
//! the page) and leave a note. Rather than touch the document source, every
//! note persists into a **`comments.wcl` sidecar** keyed by a page, an object
//! address (`object_kind` + `object_id`), or both. Watchers ignore
//! `comments.wcl`, so a comment
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
//!   object_kind = "concept"    // optional object address (the pair is atomic)
//!   object_id = "records"
//!   loc = "0/2/1"              // block locator within the page (absent = whole page)
//!   target = "card — \"3rd\""  // human description of the block (optional)
//!   body = "tighten this"
//!   author = "wil"            // optional
//!   quote = "highlighted"     // optional
//! }
//! ```
//!
//! One sidecar lives beside each **owning document root** (or, for a page not
//! inside one, beside the served root document) — see [`comments_path`]. The
//! caller names the file that marks a root; this crate doesn't know the
//! formats built on top of it.
//!
//! `wcl_wdoc` carries no serde dependency, so [`CommentRecord`] is a plain
//! data struct; the `wcl` crate turns it into JSON for the CLI / dev server.

use std::path::{Path, PathBuf};

use miette::Report;
use wcl_lang::parse_for_edit;

use crate::build::BuildError;
use crate::sidecar::{atomic_write, gen_id, read_blocks, scan_for, sidecar_path, wcl_string};

/// The sidecar file name review comments live in.
const SIDECAR: &str = "comments.wcl";

/// Which shape a stored comment takes — derived from whether it has a `loc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentScope {
    /// Targets a specific block within the page (`loc` set).
    Block,
    /// Targets the page as a whole (no `loc`).
    Page,
    /// Targets a model object without requiring a rendered page.
    Object,
}

impl CommentScope {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentScope::Block => "block",
            CommentScope::Page => "page",
            CommentScope::Object => "object",
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
    /// The page name the comment is on. Object-only comments have no page.
    pub page: Option<String>,
    /// The source file the page is defined in. Together with `page` it
    /// disambiguates same-named pages across different sites / sub-documents.
    pub page_file: Option<String>,
    /// The kind half of an object address (`concept`, `index`, …).
    pub object_kind: Option<String>,
    /// The id half of an object address.
    pub object_id: Option<String>,
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

/// The `comments.wcl` that owns comments for a page defined in `page_file`:
/// the nearest ancestor directory (within `root`) holding `marker` — the
/// file the caller uses to mark a document root — else `root/comments.wcl`.
/// Walking from the page's *source* file means a generated page (whose source
/// lives under `…/wdoc/book/`) still resolves to the `comments.wcl` beside
/// the root that owns it.
pub fn comments_path(page_file: &Path, root: &Path, marker: &str) -> PathBuf {
    sidecar_path(page_file, root, marker, SIDECAR)
}

/// List every comment stored in any `comments.wcl` under `root` (so a server
/// rooted at the top `docs/` finds every owned sidecar plus the root one).
pub fn list(root: &Path) -> Result<Vec<CommentRecord>, BuildError> {
    let mut files = Vec::new();
    scan_for(root, SIDECAR, &mut files);
    Ok(files.iter().flat_map(|p| read_file(p)).collect())
}

/// Parse every `comment { … }` block out of one `comments.wcl`, reading each
/// field's literal value off the AST (no `@document` schema needed — same
/// bypass `wcl init` uses for answer files). A malformed / missing file yields
/// no records rather than erroring (a sidecar is non-critical metadata).
fn read_file(path: &Path) -> Vec<CommentRecord> {
    let mut out = Vec::new();
    for mut fields in read_blocks(path, "comment") {
        let Some(id) = fields.remove("id") else {
            continue;
        };
        let page = fields.remove("page").filter(|s| !s.is_empty());
        let loc = fields.remove("loc").filter(|s| !s.is_empty());
        let object_kind = fields.remove("object_kind").filter(|s| !s.is_empty());
        let object_id = fields.remove("object_id").filter(|s| !s.is_empty());
        if (object_kind.is_some() != object_id.is_some())
            || (page.is_none() && object_kind.is_none())
            || (page.is_none() && loc.is_some())
        {
            continue;
        }
        out.push(CommentRecord {
            scope: if loc.is_some() {
                CommentScope::Block
            } else if page.is_some() {
                CommentScope::Page
            } else {
                CommentScope::Object
            },
            id,
            file: path.to_path_buf(),
            page,
            page_file: fields.remove("page_file"),
            object_kind,
            object_id,
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
    add_addressed(
        comments_file,
        Some(page),
        page_file,
        loc,
        target,
        None,
        None,
        body,
        author,
        quote,
    )
}

/// Append a comment addressed to a rendered page, a model object, or both.
///
/// An object address is indivisible: `object_kind` and `object_id` must be
/// supplied together. At least one complete address (`page` or object) is
/// required, and a positional `loc` only makes sense when a page is present.
#[allow(clippy::too_many_arguments)]
pub fn add_addressed(
    comments_file: &Path,
    page: Option<&str>,
    page_file: Option<&str>,
    loc: Option<&str>,
    target: Option<&str>,
    object_kind: Option<&str>,
    object_id: Option<&str>,
    body: &str,
    author: Option<&str>,
    quote: Option<&str>,
) -> Result<String, BuildError> {
    let page = page.filter(|s| !s.is_empty());
    let object_kind = object_kind.filter(|s| !s.is_empty());
    let object_id = object_id.filter(|s| !s.is_empty());
    if object_kind.is_some() != object_id.is_some() {
        return Err(BuildError::BadPage(
            "a comment object address needs both `object_kind` and `object_id`".into(),
        ));
    }
    if page.is_none() && object_kind.is_none() {
        return Err(BuildError::BadPage(
            "a comment needs a `page` or an object address".into(),
        ));
    }
    let id = gen_id('c');
    let loc = loc.map(str::to_string).filter(|s| !s.is_empty());
    if page.is_none() && loc.is_some() {
        return Err(BuildError::BadPage("a comment `loc` needs a `page`".into()));
    }
    let mut recs = read_file(comments_file);
    recs.push(CommentRecord {
        scope: if loc.is_some() {
            CommentScope::Block
        } else if page.is_some() {
            CommentScope::Page
        } else {
            CommentScope::Object
        },
        id: id.clone(),
        file: comments_file.to_path_buf(),
        page: page.map(str::to_string),
        page_file: page_file.map(str::to_string),
        object_kind: object_kind.map(str::to_string),
        object_id: object_id.map(str::to_string),
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
        "// Review comments for this document — written from the `wcl editor`\n\
         // preview pane and read back by `wcl wdoc comments`. Each `comment` block is\n\
         // addressed to a page, an object, or both; safe to hand-edit or generate.\n\n",
    );
    for r in recs {
        out.push_str("comment {\n");
        out.push_str(&format!("  id = {}\n", wcl_string(&r.id)));
        if let Some(page) = &r.page {
            out.push_str(&format!("  page = {}\n", wcl_string(page)));
        }
        if let Some(pf) = &r.page_file {
            out.push_str(&format!("  page_file = {}\n", wcl_string(pf)));
        }
        if let Some(kind) = &r.object_kind {
            out.push_str(&format!("  object_kind = {}\n", wcl_string(kind)));
        }
        if let Some(id) = &r.object_id {
            out.push_str(&format!("  object_id = {}\n", wcl_string(id)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert_eq!(rec.page.as_deref(), Some("home"));
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
    fn object_only_comment_round_trips() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");
        fs::write(
            &file,
            "comment {\n  id = \"cobject\"\n  object_kind = \"concept\"\n  \
             object_id = \"records\"\n  body = \"This unit should not exist.\"\n  \
             author = \"curator\"\n}\n",
        )
        .unwrap();

        let recs = ok(list(&dir));
        let rec = recs.iter().find(|r| r.id == "cobject").unwrap();
        assert_eq!(rec.scope, CommentScope::Object);
        assert!(rec.page.is_none());
        assert_eq!(rec.object_kind.as_deref(), Some("concept"));
        assert_eq!(rec.object_id.as_deref(), Some("records"));
        assert_eq!(rec.author.as_deref(), Some("curator"));

        assert!(ok(edit(&dir, "cobject", "Sharper finding.")));
        let written = fs::read_to_string(&file).unwrap();
        assert!(written.contains("object_kind = \"concept\""), "{written}");
        assert!(written.contains("object_id = \"records\""), "{written}");
        assert!(!written.contains("page ="), "{written}");
    }

    #[test]
    fn addressed_add_requires_a_page_or_a_complete_object_address() {
        let dir = tempdir();
        let file = dir.join("comments.wcl");

        for (kind, id) in [(None, None), (Some("concept"), None), (None, Some("alpha"))] {
            let result = add_addressed(
                &file,
                None,
                None,
                None,
                None,
                kind,
                id,
                "finding",
                Some("curator"),
                None,
            );
            assert!(result.is_err(), "accepted partial address {kind:?}:{id:?}");
        }

        let id = ok(add_addressed(
            &file,
            None,
            None,
            None,
            None,
            Some("index"),
            Some("reference"),
            "This index has no body.",
            Some("curator"),
            None,
        ));
        let rec = ok(list(&dir)).into_iter().find(|r| r.id == id).unwrap();
        assert_eq!(rec.scope, CommentScope::Object);
        assert_eq!(rec.object_kind.as_deref(), Some("index"));
        assert_eq!(rec.object_id.as_deref(), Some("reference"));
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
        assert_eq!(rec.page.as_deref(), Some("home"));

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
    fn comments_path_walks_up_to_the_owning_root_else_the_served_root() {
        let root = tempdir();
        // An owned sub-document at root/owned/x — marked by the caller's
        // marker file — with a generated page under wdoc/book/.
        const MARKER: &str = "owner.wcl";
        let owner = root.join("owned").join("x");
        let book = owner.join("wdoc").join("book");
        fs::create_dir_all(&book).unwrap();
        fs::write(owner.join(MARKER), "").unwrap();
        let page_file = book.join("main.wcl");
        fs::write(&page_file, "").unwrap();
        assert_eq!(
            comments_path(&page_file, &root, MARKER),
            owner.join("comments.wcl")
        );

        // A page inside no owned root falls back to root/comments.wcl.
        let pages = root.join("pages");
        fs::create_dir_all(&pages).unwrap();
        let plain = pages.join("index.wcl");
        fs::write(&plain, "").unwrap();
        let want = fs::canonicalize(&root).unwrap().join("comments.wcl");
        assert_eq!(comments_path(&plain, &root, MARKER), want);
    }
}
