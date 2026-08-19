//! Source printer for the editing path.
//!
//! Walks an [`ast::Source`] and produces canonical WCL source text. The
//! input is whatever came out of [`crate::parse_for_edit`] (possibly
//! after host mutation); the output is what a host writes to disk.
//!
//! The two trivia kinds that [`ast::Trivia`] carries — [`Trivia::LineComment`]
//! and [`Trivia::BlankLine`] — survive round-tripping. Everything else
//! (indentation, brace style, number radix, underscore separators,
//! string-delimiter choice, decorator ordering vs the previous item) is
//! reformatted canonically.
//!
//! The printer is deliberately self-contained — no host context, no
//! evaluation. Constructed AST nodes (synthesised from scratch via the
//! public `ast::*` types) work the same as parsed ones.

mod decls;
mod expr;
mod literals;
mod pattern;
mod types;

use crate::ast::{Expr, Item, Source, SymbolEntry, Trivia, TypeField, UnionVariant};
use crate::value::EscapeString;

/// Knobs that customize [`to_source_with`]. [`Default`] matches the
/// historical behaviour of [`to_source`] (two-space indent, trailing
/// commas in match arms, blank lines preserved).
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Spaces per indentation level. Default: 2.
    pub indent: usize,
    /// Emit a trailing comma after every `match` arm. Default: true.
    /// The parser tolerates either form; flipping this only affects
    /// the printer's output style.
    pub trailing_comma_in_match: bool,
    /// Maximum consecutive blank lines preserved from source trivia.
    /// `0` collapses all blank lines; `>= 1` preserves one (the lexer
    /// already coalesces runs of blank lines to a single marker, so
    /// any value `>= 1` is currently equivalent). Default: 1.
    pub blank_line_cap: usize,
    /// Column at which a long single-line string label on a prose block
    /// (see [`TEXT_WRAP_KINDS`]) is converted to a wrapped heredoc.
    /// Wrapping inserts real newlines into the value — safe for prose
    /// blocks because every renderer treats a newline inside paragraph
    /// text as a space. Default: 100.
    pub text_wrap_width: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            trailing_comma_in_match: true,
            blank_line_cap: 1,
            text_wrap_width: 100,
        }
    }
}

/// Block kinds whose inline string label is *prose* — long text the
/// formatter may wrap into a heredoc (a value-changing edit: wrapping
/// inserts `\n`s, which paragraph rendering treats as spaces). Only
/// kinds with markdown-style flowing text belong here; everything else
/// keeps its label byte-exact.
pub const TEXT_WRAP_KINDS: &[&str] = &["p"];

/// Block kinds that double as item-starter keywords: an empty-body
/// block of one of these must keep explicit `{}` (see `print_block`),
/// which also makes a heredoc label illegal (the closing tag must be
/// followed by a bare newline).
pub(super) const ITEM_STARTERS: &[&str] = &[
    "import",
    "use",
    "namespace",
    "type",
    "interface",
    "union",
    "symbol_set",
    "let",
    "fn",
    "table",
    "connection",
];

/// Canonical source for an AST using the default [`FormatConfig`].
/// Mutate the AST via the public `ast::*` types, then call this to
/// render a `.wcl` file body.
pub fn to_source(ast: &Source) -> String {
    to_source_with(ast, &FormatConfig::default())
}

/// Render an AST with explicit formatting options. See [`FormatConfig`]
/// for the available knobs.
pub fn to_source_with(ast: &Source, cfg: &FormatConfig) -> String {
    let mut p = Printer::new(cfg.clone());
    p.print_source(ast);
    p.buf
}

/// Canonical source for a single top-level [`Item`] (type / interface /
/// union / symbol_set / block / field). Used by `ast_string` to render the
/// declaration behind a dataref. Trailing newline trimmed.
pub fn to_source_item(item: &Item) -> String {
    let mut p = Printer::new(FormatConfig::default());
    p.print_item(item);
    p.buf.trim_end().to_string()
}

/// Canonical source for a single expression — notably a function literal
/// (`fn(params) -> ret body`), used to render a `Value::Function`.
pub fn to_source_expr(expr: &Expr) -> String {
    let mut p = Printer::new(FormatConfig::default());
    p.print_expr(expr, 0);
    p.buf.trim_end().to_string()
}

/// Canonical source for a single type/interface field declaration (a
/// sub-node that isn't a valid standalone [`Item`]).
pub fn to_source_type_field(field: &TypeField) -> String {
    let mut p = Printer::new(FormatConfig::default());
    p.print_type_field(field);
    p.buf.trim_end().to_string()
}

/// Canonical source for a single union variant.
pub fn to_source_union_variant(variant: &UnionVariant) -> String {
    let mut p = Printer::new(FormatConfig::default());
    p.print_union_variant(variant);
    p.buf.trim_end().to_string()
}

/// Canonical source for a single symbol-set entry.
pub fn to_source_symbol_entry(entry: &SymbolEntry) -> String {
    let mut p = Printer::new(FormatConfig::default());
    p.print_symbol_entry(entry);
    p.buf.trim_end().to_string()
}

/// The source printer: an output buffer plus the indentation
/// state that emitting a nested item needs.
struct Printer {
    /// Accumulated output.
    buf: String,
    /// Current indentation level, in units of `indent_str`.
    depth: u16,
    /// Formatting options this run was built with.
    cfg: FormatConfig,
    /// One level of indentation, precomputed from `cfg`.
    indent_str: String,
    /// `true` only while printing a value position a heredoc may
    /// legally occupy: the direct value of a field/let with no trailing
    /// comment, where a bare newline follows the closing tag. Heredoc
    /// closers must sit alone on their line, so emitting one inside a
    /// call argument / list (the next token would glue onto the tag) or
    /// before a trailing comment produces output that fails to
    /// re-parse. Consumed (reset to `false`) at the top of every
    /// `print_expr`, so only the outermost expression sees it.
    allow_heredoc: bool,
    /// Depth of `${…}` interpolation slots currently being printed. A
    /// slot must not span multiple lines, so while inside one every
    /// construct with a multi-line form (`match` arms, block
    /// expressions with `let`s, comment-carrying records / lists /
    /// parameter lists) prints its single-line form instead — a
    /// multi-line render there is unparseable output, not style.
    slot_depth: u16,
}

impl Printer {
    /// Start an empty printer with the given options.
    fn new(cfg: FormatConfig) -> Self {
        let indent_str = " ".repeat(cfg.indent);
        Self {
            buf: String::new(),
            depth: 0,
            cfg,
            indent_str,
            allow_heredoc: false,
            slot_depth: 0,
        }
    }

    /// Whether printing is inside a `${…}` interpolation slot, where the
    /// output must stay on one line to re-parse.
    fn in_slot(&self) -> bool {
        self.slot_depth > 0
    }

    /// Append a string verbatim.
    fn push(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    /// Append a single character.
    fn push_ch(&mut self, c: char) {
        self.buf.push(c);
    }

    /// Append indentation for the current depth.
    fn write_indent(&mut self) {
        for _ in 0..self.depth {
            self.buf.push_str(&self.indent_str);
        }
    }

    /// End the current line.
    fn newline(&mut self) {
        self.buf.push('\n');
    }

    /// Emit one trivia run as it should appear above an Item. Each
    /// `LineComment` becomes one `# body\n` line at the current
    /// indent. Each `BlankLine` is exactly one blank `\n`.
    fn print_leading_trivia(&mut self, trivia: &[Trivia]) {
        let mut consecutive_blanks: usize = 0;
        for t in trivia {
            match t {
                Trivia::BlankLine => {
                    // Canonical output never *starts* with blank lines —
                    // they'd be re-lexed as one fewer blank on the next
                    // pass (the file's final newline is a terminator,
                    // not a blank), breaking idempotence for
                    // whitespace-only files.
                    if self.buf.is_empty() {
                        continue;
                    }
                    consecutive_blanks += 1;
                    if consecutive_blanks <= self.cfg.blank_line_cap {
                        self.newline();
                    }
                }
                Trivia::LineComment(body) => {
                    consecutive_blanks = 0;
                    self.write_indent();
                    self.buf.push_str("# ");
                    self.buf.push_str(body);
                    self.newline();
                }
            }
        }
    }

    /// Emit a same-line trailing comment after a node's content, before
    /// its terminating newline: two spaces, `# `, then the body. No-op
    /// when there is no trailing comment.
    fn print_trailing_comment(&mut self, comment: &Option<String>) {
        if let Some(body) = comment {
            self.push("  # ");
            self.push(body);
        }
    }

    // ---------- source / items ----------

    /// Print a whole file: every item, then the trailing trivia.
    fn print_source(&mut self, s: &Source) {
        for item in &s.items {
            self.print_item(item);
        }
        self.print_leading_trivia(&s.trailing_trivia);
    }

    /// Print one top-level item, dispatching on its variant.
    fn print_item(&mut self, item: &Item) {
        match item {
            Item::Field(f) => self.print_field(f),
            Item::Let(l) => self.print_let_item(l),
            Item::Block(b) => self.print_block(b),
            Item::TypeDecl(t) => self.print_type_decl(t),
            Item::InterfaceDecl(t) => self.print_interface_decl(t),
            Item::UnionDecl(u) => self.print_union_decl(u),
            Item::SymbolSetDecl(s) => self.print_symbol_set_decl(s),
            Item::NamespaceDecl(n) => self.print_namespace_decl(n),
            Item::UseDecl(u) => self.print_use_decl(u),
            Item::Import(i) => self.print_import_decl(i),
            Item::Table(t) => self.print_table_item(t),
            Item::ConnectionDecl(c) => self.print_connection_decl(c),
            Item::Connection(c) => self.print_connection_stmt(c),
        }
    }

    // ---------- decorators ----------

    // ---------- expressions ----------

    // ---------- patterns ----------

    // ---------- type refs ----------
}

/// True when a trivia run contains at least one line comment (blank
/// lines alone don't force a single-line collection to break).
pub(super) fn trivia_has_comment(trivia: &[Trivia]) -> bool {
    trivia.iter().any(|t| matches!(t, Trivia::LineComment(_)))
}

/// Join dotted path segments back into source form.
pub(super) fn join_path(parts: &[String]) -> String {
    // Mirrors the `Expr::Member` printing rules: a variant type path can
    // carry numeric member segments, and gluing them re-lexes wrongly —
    // `.` + `-2` is not a valid member start, and a digit-leading segment
    // after a digit-ending one merges into a float (`0.80`). A space
    // after the dot keeps the reparse on the member-chain path.
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push('.');
            let glue = p.starts_with('-')
                || (p.as_bytes().first().is_some_and(u8::is_ascii_digit)
                    && out
                        .len()
                        .checked_sub(2)
                        .and_then(|i| out.as_bytes().get(i))
                        .is_some_and(u8::is_ascii_digit));
            if glue {
                out.push(' ');
            }
        }
        out.push_str(p);
    }
    out
}

/// Render a field key: bare when it is a valid identifier, otherwise as a
/// double-quoted string. String-literal keys (e.g. `"allowed-tools"` in a
/// `@schemaless` block) round-trip through the quoted form; identifier keys
/// are unchanged.
pub(super) fn field_key(name: &str) -> String {
    if crate::is_identifier(name) {
        name.to_string()
    } else {
        format!("\"{}\"", EscapeString(name))
    }
}
