//! The **edit path**: parse to an owned AST, find a node, hand it back.
//!
//! The counterpart of the evaluating path ([`crate::Document`]), and
//! deliberately disjoint from it. [`parse_for_edit`] returns an
//! [`ast::Source`] with fully `pub` fields and does
//! *no* evaluation, schema checking or import resolution;
//! [`find_field_by_span`] locates the node a host wants to change; the
//! host mutates it and prints the result back with
//! [`format::to_source`](crate::format::to_source). To evaluate after an
//! edit, reopen the file as a `Document`.
//!
//! [`parse_expr`] is the small companion for supplying a replacement:
//! one expression parsed from a standalone string, ready to drop into
//! the mutated tree.
//!
//! Node lookup is by byte [`Span`] equality, which works because the
//! edit path re-parses the same source bytes a [`crate::Document`] saw —
//! the positions match exactly.
//!
//! # Setting one field
//!
//! The whole round trip — find the field, parse the replacement, rewrite
//! the source, prove the result still parses — is packaged as
//! [`set_field`] for a self-contained source string. It comes apart in
//! two for hosts that open documents their own way (a custom
//! [`Environment`](crate::Environment) or loader, or imports that put the
//! field in another file): [`locate_field`] asks an open [`Document`]
//! where a dotted path is declared, and [`replace_field`] rewrites that
//! file's source. `wcl set` is built from the two halves.

#![allow(unused_assignments)] // miette/thiserror derive triggers spurious lints on variant fields

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::Document;
use crate::ast::{self, Field, Item, Span};
use crate::diagnostics::{ParseError, SyntaxError};
use crate::parser;
use crate::symbols::SymbolIndex;

/// The name [`replace_field`] parses its own output under, so it is the
/// name an [`EditError::Unprintable`] diagnostic carries.
const FORMATTED_OUTPUT: &str = "<formatted output>";

/// The name [`set_field`] parses the replacement value under.
const SET_VALUE: &str = "<set value>";

/// Parse a WCL source string into an owned [`ast::Source`] for inspection
/// or mutation. The returned AST has fully `pub` fields. Hosts walk it,
/// edit it, and print it back to a `.wcl` file with
/// [`crate::format::to_source`].
///
/// This is the **edit-path** entry point. It performs *no* evaluation,
/// schema checks, or import resolution — those happen only when a
/// [`Document`] is opened from the (post-edit) file. The two paths are
/// deliberately disjoint so AST mutations can't invalidate a
/// Document's cached fields silently.
///
/// `name` is used for diagnostics only (it becomes the
/// `NamedSource` label on any [`ParseError`]).
pub fn parse_for_edit(source: &str, name: impl Into<String>) -> Result<ast::Source, ParseError> {
    parser::Parser::new(source, name)
        .parse_source()
        .map(|(src, _idx)| src)
}

/// Parse a WCL source string the way an editor needs it: past its
/// syntax errors. Where [`parse_for_edit`] fails on the first file with
/// any mistake, this keeps every item that parsed and reports every
/// error it met, so a host can still outline, highlight or navigate a
/// file that is half-typed.
///
/// A failed item is dropped and parsing resumes at the next item: the
/// next line starting one at the same nesting level, or the `}` closing
/// the body it sat in. Inside a block body this happens per item, so a
/// broken field keeps its siblings. At most
/// [`MAX_SYNTAX_ERRORS`](crate::MAX_SYNTAX_ERRORS) errors are collected;
/// past that the parse stops and returns what it has.
///
/// For a source with no errors the tree is the one [`parse_for_edit`]
/// returns. Like it, this does no evaluation or import resolution.
pub fn parse_for_edit_recovering(source: &str, name: impl Into<String>) -> PartialParse {
    let (source, symbols, errors) = parser::Parser::new(source, name).parse_source_recovering();
    PartialParse {
        source,
        symbols,
        errors,
    }
}

/// The result of [`parse_for_edit_recovering`]: the tree built from the
/// items that parsed, their symbols, and the syntax errors in between.
#[derive(Debug)]
pub struct PartialParse {
    /// Every item that parsed, in source order.
    pub source: ast::Source,
    /// The declarations those items make, as
    /// [`Document::symbols`](crate::Document::symbols) indexes them for a
    /// file that parses.
    pub symbols: SymbolIndex,
    /// Every syntax error found, in source order. Empty when the source
    /// parsed cleanly.
    pub errors: Vec<SyntaxError>,
}

impl PartialParse {
    /// The all-or-nothing view [`parse_for_edit`] gives: the tree when
    /// there were no errors, otherwise one [`ParseError`] carrying them
    /// all.
    pub fn into_result(self) -> Result<ast::Source, ParseError> {
        match ParseError::from_syntax_errors(self.errors) {
            Some(err) => Err(err),
            None => Ok(self.source),
        }
    }
}

/// Parse a single WCL expression from a standalone string. Returns the
/// parsed [`ast::Expr`] ready to drop into a host-mutated AST
/// (e.g. `field.expr = parse_expr(...)?`).
///
/// Fails if the input is empty, has trailing tokens after the
/// expression, or contains a lex/parse error. `name` is used only for
/// diagnostics — typically `"<cli>"` or `"<set value>"` when there's
/// no real source location.
///
/// Useful for CLI flows like `wcl set file path <value>`, where
/// `<value>` is a literal expression supplied on the command line.
pub fn parse_expr(source: &str, name: impl Into<String>) -> Result<ast::Expr, ParseError> {
    parser::Parser::new(source, name).parse_expr_only()
}

/// Walk `items` (recursing into [`Item::Block`] bodies) to find the
/// [`crate::ast::Field`] whose `span` matches `span`.
pub fn find_field_by_span(items: &mut [Item], span: Span) -> Option<&mut Field> {
    for item in items {
        match item {
            Item::Field(f) if f.span == span => return Some(f),
            Item::Block(b) => {
                if let Some(found) = find_field_by_span(&mut b.items, span) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Why a field edit could not be made. Each variant is a failure a host
/// may want to report differently; the ones that carry a [`ParseError`]
/// keep it whole so it can still be rendered against its source.
#[derive(Debug, Error)]
pub enum EditError {
    /// Nothing in the document answers to the path.
    #[error("no such path: {path}")]
    NoSuchPath {
        /// The dotted path that was asked for.
        path: String,
        /// The closest top-level name, when the path's first segment
        /// looks like a typo of one (see [`suggest_path`]).
        suggestion: Option<String>,
    },
    /// The path names something other than a field — a block, a type,
    /// a list of blocks. Only a field has one expression to replace.
    #[error("`{path}` resolved to a {kind}, not a field")]
    NotAField {
        /// The dotted path that was asked for.
        path: String,
        /// What the path resolved to, as
        /// [`DataRef::kind`](crate::DataRef::kind) names it (`block`,
        /// `type`, …).
        kind: &'static str,
    },
    /// The field is declared in an imported file. [`set_field`] holds
    /// only the entry source, so it cannot rewrite it; call
    /// [`locate_field`] and [`replace_field`] on that file instead.
    #[error("`{path}` is declared in {}, not in this source", .file.display())]
    Imported {
        /// The dotted path that was asked for.
        path: String,
        /// The file that declares the field.
        file: PathBuf,
    },
    /// The replacement value is not a single WCL expression.
    #[error("invalid value: {0}")]
    InvalidValue(#[source] ParseError),
    /// The source being edited did not parse.
    #[error("{0}")]
    InvalidSource(#[source] ParseError),
    /// No field in the source sits at the span the document reported,
    /// so the source is not the text the document was opened from.
    #[error("no field at span {}..{} in {name}", .span.start, .span.end)]
    FieldNotFound {
        /// The span that was looked for.
        span: Span,
        /// The name the source was parsed under.
        name: String,
    },
    /// The edited tree printed to text that does not parse. That is a
    /// formatter bug, so the text is withheld rather than returned.
    #[error("the edited source does not re-parse: {0}")]
    Unprintable(#[source] ParseError),
}

/// Where a field is declared: the file, and the byte span of its
/// `name = value` item there. Returned by [`locate_field`] and consumed
/// by [`replace_field`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTarget {
    /// Byte span of the field in the source that declares it.
    pub span: Span,
    /// The imported file that declares the field, or `None` when the
    /// document's own source declares it.
    pub source_path: Option<PathBuf>,
}

impl FieldTarget {
    /// The file to rewrite: [`source_path`](Self::source_path) when the
    /// field was imported, otherwise `entry`, the file the document was
    /// opened from.
    pub fn file<'a>(&'a self, entry: &'a Path) -> &'a Path {
        self.source_path.as_deref().unwrap_or(entry)
    }
}

/// Find where the field at a dotted `path` is declared in an open
/// document, following imports to the file that declares it.
///
/// Fails with [`EditError::NoSuchPath`], carrying a typo suggestion when
/// there is one, or with [`EditError::NotAField`] when the path names a
/// block or a declaration. The result owns its data, so the document can
/// be dropped before the file is rewritten.
///
/// ```
/// use wcl_lang::{Document, edit};
///
/// let doc = Document::open("@schemaless port = 80\n", "site.wcl").unwrap();
/// let target = edit::locate_field(&doc, "port").unwrap();
/// assert_eq!(target.source_path, None); // declared in site.wcl itself
///
/// let err = edit::locate_field(&doc, "prot").unwrap_err();
/// assert!(matches!(
///     err,
///     edit::EditError::NoSuchPath { suggestion: Some(s), .. } if s == "port"
/// ));
/// ```
pub fn locate_field(doc: &Document, path: &str) -> Result<FieldTarget, EditError> {
    let dr = doc.get(path).ok_or_else(|| EditError::NoSuchPath {
        path: path.to_string(),
        suggestion: suggest_path(doc, path),
    })?;
    let field = dr.as_field().ok_or_else(|| EditError::NotAField {
        path: path.to_string(),
        kind: dr.kind(),
    })?;
    Ok(FieldTarget {
        span: field.span(),
        source_path: field.source_path().map(Path::to_path_buf),
    })
}

/// Replace the expression of the field at `span` in `source` with
/// `value`, and return the whole source reprinted in canonical form.
///
/// `span` comes from [`locate_field`] on a document opened from the same
/// bytes; `name` labels any diagnostic. The output is re-parsed before it
/// is returned, so a caller never receives text that does not parse
/// ([`EditError::Unprintable`]). As with [`crate::format::to_source`],
/// the whole file is normalised, not only the edited line.
///
/// ```
/// use wcl_lang::{Document, edit, parse_expr};
///
/// let source = "@schemaless port = 80\n";
/// let doc = Document::open(source, "site.wcl").unwrap();
/// let target = edit::locate_field(&doc, "port").unwrap();
/// let value = parse_expr("8080", "<value>").unwrap();
/// let edited = edit::replace_field(source, "site.wcl", target.span, value).unwrap();
/// assert_eq!(edited, "@schemaless port = 8080\n");
/// ```
pub fn replace_field(
    source: &str,
    name: impl Into<String>,
    span: Span,
    value: ast::Expr,
) -> Result<String, EditError> {
    let name = name.into();
    let mut ast = parse_for_edit(source, name.clone()).map_err(EditError::InvalidSource)?;
    let slot =
        find_field_by_span(&mut ast.items, span).ok_or(EditError::FieldNotFound { span, name })?;
    slot.expr = value;
    let printed = crate::format::to_source(&ast);
    parse_for_edit(&printed, FORMATTED_OUTPUT).map_err(EditError::Unprintable)?;
    Ok(printed)
}

/// Set the field at a dotted `path` in a self-contained WCL source to
/// `value`, written as a WCL expression, and return the edited source.
///
/// Opens `source` as a [`Document`] in the default environment to find
/// the field, then rewrites it with [`replace_field`]. A field declared
/// in an imported file is refused with [`EditError::Imported`]. Hosts
/// that follow imports, or that need their own environment to open the
/// file, call [`locate_field`] and [`replace_field`] themselves.
///
/// ```
/// use wcl_lang::edit;
///
/// let source = "server {\n  port = 80\n}\n";
/// let edited = edit::set_field(source, "site.wcl", "server.port", "8080u32").unwrap();
/// assert!(edited.contains("port = 8080u32"));
///
/// let err = edit::set_field(source, "site.wcl", "server", "1").unwrap_err();
/// assert!(matches!(err, edit::EditError::NotAField { kind: "block", .. }));
/// ```
pub fn set_field(source: &str, name: &str, path: &str, value: &str) -> Result<String, EditError> {
    let doc = Document::open(source, name).map_err(EditError::InvalidSource)?;
    let target = locate_field(&doc, path)?;
    if let Some(file) = target.source_path {
        return Err(EditError::Imported {
            path: path.to_string(),
            file,
        });
    }
    let value = parse_expr(value, SET_VALUE).map_err(EditError::InvalidValue)?;
    replace_field(source, name, target.span, value)
}

/// The top-level field or block kind closest to the first segment of
/// `needle`, when it is within two edits of it and not an exact match.
///
/// Only the first segment of a dotted path is compared. That catches the
/// common typo (`prot` for `port`) without walking the tree.
///
/// ```
/// use wcl_lang::{Document, edit};
///
/// let doc = Document::open("@schemaless ports = [80]\n", "site.wcl").unwrap();
/// assert_eq!(edit::suggest_path(&doc, "port").as_deref(), Some("ports"));
/// assert_eq!(edit::suggest_path(&doc, "ports"), None);
/// ```
pub fn suggest_path(doc: &Document, needle: &str) -> Option<String> {
    let first = needle.split('.').next().unwrap_or(needle);
    let mut candidates: Vec<String> = Vec::new();
    candidates.extend(doc.fields().map(|f| f.name().to_string()));
    candidates.extend(doc.blocks().map(|b| b.kind().to_string()));
    candidates
        .into_iter()
        .filter_map(|c| {
            let d = levenshtein(first, &c);
            (d > 0 && d <= 2).then_some((d, c))
        })
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// Edit distance between two strings, counted in characters.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
