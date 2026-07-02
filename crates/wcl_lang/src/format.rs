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

use std::fmt::Write as _;

use crate::ast::{
    Block, CALL_BP, ConnectionDecl, ConnectionStmt, Decorator, ElemTrivia, Expr, Field,
    FunctionLit, ImportDecl, InterfaceDecl, Item, LetBinding, MEMBER_BP, MatchArm, NamedArg,
    NamespaceDecl, Parameter, Pattern, Row, Source, SymbolEntry, SymbolSetDecl, TableItem,
    TemplatePart, Trivia, TypeDecl, TypeField, UNARY_BP, UnaryOp, UnionDecl, UnionVariant, UseDecl,
    UseForm, UseItem, VariantArgs, VariantBody, VariantPatArgs,
};
use crate::lexer::StringEncoding;
use crate::value::{BuiltinType, EscapeString, TensorDim, TypeRef};

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
const ITEM_STARTERS: &[&str] = &[
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

struct Printer {
    buf: String,
    depth: u16,
    cfg: FormatConfig,
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
}

impl Printer {
    fn new(cfg: FormatConfig) -> Self {
        let indent_str = " ".repeat(cfg.indent);
        Self {
            buf: String::new(),
            depth: 0,
            cfg,
            indent_str,
            allow_heredoc: false,
        }
    }

    fn push(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    fn push_ch(&mut self, c: char) {
        self.buf.push(c);
    }

    fn write_indent(&mut self) {
        for _ in 0..self.depth {
            self.buf.push_str(&self.indent_str);
        }
    }

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

    fn print_source(&mut self, s: &Source) {
        for item in &s.items {
            self.print_item(item);
        }
        self.print_leading_trivia(&s.trailing_trivia);
    }

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

    fn print_field(&mut self, f: &Field) {
        self.print_leading_trivia(&f.leading_trivia);
        self.write_indent();
        self.print_decorators_inline(&f.decorators);
        self.push(&field_key(&f.name));
        self.push(" = ");
        // A heredoc closer must be followed by a bare newline — legal
        // here only when no trailing comment will share its line.
        self.allow_heredoc = f.trailing_comment.is_none();
        self.print_expr(&f.expr, 0);
        self.print_trailing_comment(&f.trailing_comment);
        self.newline();
    }

    fn print_let_item(&mut self, l: &crate::ast::LetItem) {
        self.print_leading_trivia(&l.leading_trivia);
        // The `fn name(…)` item form round-trips as written: decorators
        // (own line, like a type decl's), then `fn name` spliced before
        // the literal's parameter list.
        if l.fn_syntax
            && let Expr::Function(f) = &l.value
        {
            self.print_decorators_block(&l.decorators);
            self.write_indent();
            self.push("fn ");
            self.push(&l.name);
            self.print_function_signature_and_body(f);
            self.print_trailing_comment(&l.trailing_comment);
            self.newline();
            return;
        }
        self.write_indent();
        self.push("let ");
        self.push(&l.name);
        self.push(" = ");
        self.allow_heredoc = l.trailing_comment.is_none();
        self.print_expr(&l.value, 0);
        self.print_trailing_comment(&l.trailing_comment);
        self.newline();
    }

    fn print_block(&mut self, b: &Block) {
        self.print_leading_trivia(&b.leading_trivia);
        self.write_indent();
        self.print_decorators_inline(&b.decorators);
        if !b.kind_ns.is_empty() {
            self.push(&b.kind_ns.join("."));
            self.push("::");
        }
        self.push(&b.kind);
        // A single string label may print as a heredoc — legal only when
        // nothing can follow it on the line (empty body, no trailing
        // comment: the closing tag needs a bare newline, and item-starter
        // kinds keep `{}`). Two cases:
        //   - value-preserving: a multi-line label (an authored heredoc
        //     label) keeps its heredoc form instead of degrading to a
        //     `\n`-escaped one-liner — any kind;
        //   - prose wrapping: a TEXT_WRAP_KINDS block whose quoted label
        //     would overflow `text_wrap_width` is wrapped at safe break
        //     points (never inside an inline-markup construct — the
        //     stdlib inline patterns deliberately don't match `\n`).
        if let Some(body) = self.heredoc_label(b) {
            self.push_ch(' ');
            self.print_heredoc(&body, "", false);
            self.newline();
            return;
        }
        for label in &b.labels {
            self.push_ch(' ');
            self.print_expr(label, 0);
        }
        // Empty-body shorthand: omit `{}` entirely when there are no items.
        // The parser accepts both `kind labels` (no braces) and
        // `kind labels {}` (explicit empty braces); the canonical form is
        // the shorter one. Exception: a kind that doubles as an item-starter
        // keyword must keep its braces — a bare `namespace` line followed by
        // an identifier re-dispatches as a namespace *declaration* (the
        // keyword forms are recognised by `kind` + identifier lookahead,
        // which doesn't stop at line breaks), silently rewriting the tree.
        if b.items.is_empty() {
            if b.kind_ns.is_empty() && ITEM_STARTERS.contains(&b.kind.as_str()) {
                self.push(" {}");
            }
            self.print_trailing_comment(&b.trailing_comment);
            self.newline();
            return;
        }
        self.push(" {");
        self.newline();
        self.depth += 1;
        for item in &b.items {
            self.print_item(item);
        }
        self.print_leading_trivia(&b.trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.print_trailing_comment(&b.trailing_comment);
        self.newline();
    }

    /// The heredoc body to print for `b`'s label, when a heredoc label is
    /// both legal and wanted (see the comment at the call site). `None`
    /// falls back to ordinary label printing.
    fn heredoc_label(&self, b: &Block) -> Option<String> {
        if !b.items.is_empty() || b.trailing_comment.is_some() {
            return None;
        }
        if b.kind_ns.is_empty() && ITEM_STARTERS.contains(&b.kind.as_str()) {
            return None;
        }
        let [Expr::Utf8(s)] = b.labels.as_slice() else {
            return None;
        };
        // Authored multi-line label: keep the heredoc form (value-preserving).
        if s.lines().count() >= 2 {
            return (heredoc_round_trips(s)).then(|| s.clone());
        }
        // Prose wrapping: only for text-bearing kinds, and only when the
        // quoted one-liner would overflow the configured width.
        if !TEXT_WRAP_KINDS.contains(&b.kind.as_str()) {
            return None;
        }
        let line_len = self.cfg.indent * self.depth as usize
            + b.kind.chars().count()
            + 3 // ` "` + closing `"`
            + EscapeString(s).to_string().chars().count();
        if line_len <= self.cfg.text_wrap_width {
            return None;
        }
        // The heredoc body prints indented one level deeper than the block,
        // so wrap to the width that remains after that indent.
        let body_indent = self.cfg.indent * (self.depth as usize + 1);
        let content_width = self.cfg.text_wrap_width.saturating_sub(body_indent).max(20);
        let wrapped = wrap_prose(s, content_width);
        (wrapped.lines().count() >= 2 && heredoc_round_trips(&wrapped)).then_some(wrapped)
    }

    fn print_type_decl(&mut self, t: &TypeDecl) {
        self.print_leading_trivia(&t.leading_trivia);
        self.print_decorators_block(&t.decorators);
        self.write_indent();
        self.push("type ");
        self.push(&join_path(&t.name));
        // Alias form: `type Name = TypeRef`, one line.
        if let Some(target) = &t.alias {
            self.push(" = ");
            self.print_type_ref(target);
            self.print_trailing_comment(&t.trailing_comment);
            self.newline();
            return;
        }
        self.print_extends(&t.extends);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for f in &t.fields {
            self.print_type_field(f);
        }
        self.print_leading_trivia(&t.trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.print_trailing_comment(&t.trailing_comment);
        self.newline();
    }

    fn print_interface_decl(&mut self, t: &InterfaceDecl) {
        self.print_leading_trivia(&t.leading_trivia);
        self.print_decorators_block(&t.decorators);
        self.write_indent();
        self.push("interface ");
        self.push(&join_path(&t.name));
        self.print_extends(&t.extends);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for f in &t.fields {
            self.print_type_field(f);
        }
        self.print_leading_trivia(&t.trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.print_trailing_comment(&t.trailing_comment);
        self.newline();
    }

    fn print_union_decl(&mut self, u: &UnionDecl) {
        self.print_leading_trivia(&u.leading_trivia);
        self.print_decorators_block(&u.decorators);
        self.write_indent();
        self.push("union ");
        self.push(&join_path(&u.name));
        self.print_extends(&u.extends);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for v in &u.variants {
            self.print_union_variant(v);
        }
        self.print_leading_trivia(&u.trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.print_trailing_comment(&u.trailing_comment);
        self.newline();
    }

    fn print_union_variant(&mut self, v: &UnionVariant) {
        self.print_leading_trivia(&v.leading_trivia);
        self.print_decorators_block(&v.decorators);
        self.write_indent();
        self.push(&v.name);
        match &v.body {
            VariantBody::Unit => {
                self.push(" none");
            }
            VariantBody::TypeRef { ty, .. } => {
                self.push_ch(' ');
                self.print_type_ref(ty);
            }
            VariantBody::InterfaceRef { iface, .. } => {
                self.push(" &");
                self.push(&join_path(iface));
            }
            VariantBody::Record {
                fields,
                trailing_trivia,
            } => {
                self.push(" {");
                self.newline();
                self.depth += 1;
                for f in fields {
                    self.print_type_field(f);
                }
                self.print_leading_trivia(trailing_trivia);
                self.depth -= 1;
                self.write_indent();
                self.push("}");
            }
        }
        self.print_trailing_comment(&v.trailing_comment);
        self.newline();
    }

    fn print_symbol_set_decl(&mut self, s: &SymbolSetDecl) {
        self.print_leading_trivia(&s.leading_trivia);
        self.print_decorators_block(&s.decorators);
        self.write_indent();
        self.push("symbol_set ");
        self.push(&join_path(&s.name));
        self.push(" {");
        self.newline();
        self.depth += 1;
        for sym in &s.symbols {
            self.print_symbol_entry(sym);
        }
        self.print_leading_trivia(&s.trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.print_trailing_comment(&s.trailing_comment);
        self.newline();
    }

    fn print_symbol_entry(&mut self, s: &SymbolEntry) {
        self.print_leading_trivia(&s.leading_trivia);
        self.print_decorators_block(&s.decorators);
        self.write_indent();
        self.push(&s.name);
        self.print_trailing_comment(&s.trailing_comment);
        self.newline();
    }

    fn print_namespace_decl(&mut self, n: &NamespaceDecl) {
        self.print_leading_trivia(&n.leading_trivia);
        self.write_indent();
        self.push("namespace ");
        self.push(&join_path(&n.path));
        self.print_trailing_comment(&n.trailing_comment);
        self.newline();
    }

    fn print_use_decl(&mut self, u: &UseDecl) {
        self.print_leading_trivia(&u.leading_trivia);
        self.write_indent();
        self.push("use ");
        self.push(&join_path(&u.path));
        match &u.form {
            UseForm::Bare(None) => {}
            UseForm::Bare(Some(alias)) => {
                self.push(" as ");
                self.push(alias);
            }
            UseForm::List(items) => {
                self.push(".{");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.print_use_item(it);
                }
                self.push("}");
            }
        }
        self.print_trailing_comment(&u.trailing_comment);
        self.newline();
    }

    fn print_use_item(&mut self, u: &UseItem) {
        self.push(&u.name);
        if let Some(alias) = &u.alias {
            self.push(" as ");
            self.push(alias);
        }
    }

    fn print_import_decl(&mut self, i: &ImportDecl) {
        self.print_leading_trivia(&i.leading_trivia);
        self.write_indent();
        self.push("import ");
        if i.system {
            // System import: `import <path>`. The path is bare identifier /
            // `/` / `.` / `-` / `_` segments, so it needs no escaping.
            self.push("<");
            self.push(&i.path);
            self.push(">");
        } else {
            self.print_string_lit(&i.path, StringEncoding::Utf8);
        }
        self.print_trailing_comment(&i.trailing_comment);
        self.newline();
    }

    fn print_table_item(&mut self, t: &TableItem) {
        self.print_leading_trivia(&t.leading_trivia);
        self.write_indent();
        self.push(&t.field_name);
        self.push(":");
        // A trailing comment belongs after the *last row* (where it
        // round-trips back to the table), not on the header line — a
        // comment after `:` would re-attach to the first row's tokens.
        // With no rows there's nowhere stable below, so it stays inline.
        if t.rows.is_empty() {
            self.print_trailing_comment(&t.trailing_comment);
        }
        self.newline();
        self.depth += 1;
        let last = t.rows.len().saturating_sub(1);
        for (i, r) in t.rows.iter().enumerate() {
            self.print_row(r);
            if i == last {
                self.print_trailing_comment(&t.trailing_comment);
            }
            self.newline();
        }
        self.depth -= 1;
    }

    fn print_row(&mut self, r: &Row) {
        self.write_indent();
        for v in &r.values {
            self.push("| ");
            self.print_expr(v, 0);
            self.push(" ");
        }
        self.push("|");
    }

    fn print_connection_decl(&mut self, c: &ConnectionDecl) {
        self.print_leading_trivia(&c.leading_trivia);
        self.print_decorators_block(&c.decorators);
        self.write_indent();
        self.push("connection ");
        self.push(&join_path(&c.name));
        self.push(" : ");
        self.print_type_ref(&c.source);
        self.push(" -> ");
        self.print_type_ref(&c.destination);
        self.push(" : ");
        self.push(&join_path(&c.kind_set));
        self.print_trailing_comment(&c.trailing_comment);
        self.newline();
    }

    fn print_connection_stmt(&mut self, c: &ConnectionStmt) {
        self.print_leading_trivia(&c.leading_trivia);
        self.write_indent();
        self.push(&c.lhs);
        self.push(" -> ");
        self.push(&c.rhs);
        if let Some(kind) = &c.kind {
            self.push(" :");
            self.push(kind);
        }
        self.print_trailing_comment(&c.trailing_comment);
        self.newline();
    }

    fn print_extends(&mut self, extends: &[Vec<String>]) {
        if extends.is_empty() {
            return;
        }
        self.push(" extends ");
        for (i, e) in extends.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&join_path(e));
        }
    }

    fn print_type_field(&mut self, f: &TypeField) {
        self.print_leading_trivia(&f.leading_trivia);
        self.write_indent();
        self.print_decorators_inline(&f.decorators);
        self.push(&f.name);
        if let Some(expr) = &f.default_expr {
            self.push(" = ");
            self.print_expr(expr, 0);
        } else {
            self.push(": ");
            self.print_type_ref(&f.ty);
            if f.optional {
                self.push("?");
            }
        }
        self.print_trailing_comment(&f.trailing_comment);
        self.newline();
    }

    // ---------- decorators ----------

    /// Decorators preceding an item that sits on its own line (type,
    /// interface, union, block, …): each decorator is emitted on its
    /// own preceding line. Indentation matches the item.
    fn print_decorators_block(&mut self, decs: &[Decorator]) {
        for d in decs {
            self.write_indent();
            self.print_decorator(d);
            self.newline();
        }
    }

    /// Decorators preceding a single-line construct (field, type
    /// field, symbol entry, …): emitted inline, space-separated, in
    /// front of the construct. Caller writes the leading indent.
    fn print_decorators_inline(&mut self, decs: &[Decorator]) {
        for d in decs {
            self.print_decorator(d);
            self.push_ch(' ');
        }
    }

    fn print_decorator(&mut self, d: &Decorator) {
        self.push("@");
        self.push(&join_path(&d.name));
        let has_args = !d.positional.is_empty() || !d.named.is_empty();
        if !has_args {
            return;
        }
        self.push("(");
        let mut first = true;
        for p in &d.positional {
            if !first {
                self.push(", ");
            }
            first = false;
            self.print_expr(p, 0);
        }
        for n in &d.named {
            if !first {
                self.push(", ");
            }
            first = false;
            self.print_named_arg(n);
        }
        self.push(")");
    }

    fn print_named_arg(&mut self, n: &NamedArg) {
        self.push(&n.name);
        self.push(" = ");
        self.print_expr(&n.value, 0);
    }

    // ---------- expressions ----------

    /// Pratt-style precedence printing: caller passes the minimum
    /// binding power required by the parent context. `Binary` wraps
    /// itself in parens when its left-bp falls below `min_bp`, so
    /// `(a + b) * c` survives round-trip.
    fn print_expr(&mut self, e: &Expr, min_bp: u8) {
        // Consume the heredoc allowance: only the outermost expression
        // of a field/let value may be printed as a heredoc — anything
        // nested (call args, list elements, operators) follows the
        // string with another token on the same line, which would glue
        // onto the closing tag.
        let allow_heredoc = std::mem::take(&mut self.allow_heredoc);
        match e {
            // ----- atoms -----
            Expr::Bool(b) => self.push(if *b { "true" } else { "false" }),
            Expr::None => self.push("none"),
            Expr::Identifier(name, _) => self.push(name),
            Expr::Symbol(name) => {
                self.push(":");
                self.push(name);
            }
            Expr::SelfKw(_) => self.push("self"),
            Expr::ParentKw(_) => self.push("parent"),

            // ----- numeric literals -----
            // write! into String is infallible; let _ = ... drops the Result.
            Expr::I8(v) => {
                let _ = write!(self.buf, "{v}i8");
            }
            Expr::I16(v) => {
                let _ = write!(self.buf, "{v}i16");
            }
            Expr::I32(v) => {
                let _ = write!(self.buf, "{v}i32");
            }
            Expr::I64(v) => {
                let _ = write!(self.buf, "{v}"); // i64 is the default suffix
            }
            Expr::I128(v) => {
                let _ = write!(self.buf, "{v}i128");
            }
            Expr::Isize(v) => {
                let _ = write!(self.buf, "{v}isize");
            }
            Expr::U8(v) => {
                let _ = write!(self.buf, "{v}u8");
            }
            Expr::U16(v) => {
                let _ = write!(self.buf, "{v}u16");
            }
            Expr::U32(v) => {
                let _ = write!(self.buf, "{v}u32");
            }
            Expr::U64(v) => {
                let _ = write!(self.buf, "{v}u64");
            }
            Expr::U128(v) => {
                let _ = write!(self.buf, "{v}u128");
            }
            Expr::Usize(v) => {
                let _ = write!(self.buf, "{v}usize");
            }
            Expr::F32(v) => {
                self.print_float(*v as f64);
                self.push("f32");
            }
            Expr::F64(v) => self.print_float(*v),

            // A literal unit prints as `<magnitude><unit>` (the suffix form
            // it was parsed from), reusing suffix-aware numeric printing.
            Expr::UnitLiteral { value, unit, .. } => {
                // A bare `0` glued to a unit that starts like a radix
                // prefix (`0` + unit `xa` → `0xa`) re-lexes as a
                // radix-prefixed number instead of a unit literal. A
                // doubled zero (`00xa`) keeps the lexer on the
                // decimal-then-unit path. Only the default-suffix
                // integer zero renders as a bare `0`; every other
                // value ends in a type suffix or a fractional part.
                if matches!(value, crate::lexer::NumberLit::I64(0))
                    && unit.starts_with(['x', 'X', 'b', 'B', 'o', 'O'])
                {
                    self.push("0");
                }
                let mark = self.buf.len();
                self.print_expr(&number_lit_to_expr(value), 0);
                // An `e<digit>…` unit glued to an exponent-less float
                // body re-lexes as an exponent (`210.0` + unit `e2e` →
                // `210.0e2e` → 21000.0 + unit `e`). Force an explicit
                // no-op exponent so the unit survives the round trip.
                if matches!(value, crate::lexer::NumberLit::F64(_))
                    && !self.buf[mark..].contains('e')
                    && unit.starts_with(['e', 'E'])
                    && unit.as_bytes().get(1).is_some_and(|b| b.is_ascii_digit())
                {
                    self.push("e0");
                }
                self.push(unit);
            }

            // ----- strings -----
            Expr::Utf8(s) => self.print_string_lit_in(s, StringEncoding::Utf8, allow_heredoc),
            Expr::Ascii(s) => {
                let utf8 = s.clone();
                self.print_string_lit_in(&utf8, StringEncoding::Ascii, allow_heredoc);
            }
            Expr::Utf16(units) => {
                let s = String::from_utf16_lossy(units);
                self.print_string_lit_in(&s, StringEncoding::Utf16, allow_heredoc);
            }
            Expr::Utf32(chars) => {
                let s: String = chars.iter().collect();
                self.print_string_lit_in(&s, StringEncoding::Utf32, allow_heredoc);
            }
            Expr::InterpolatedString {
                encoding, parts, ..
            } => self.print_interpolated(*encoding, parts, allow_heredoc),

            // ----- composites -----
            Expr::Paren { inner, .. } => {
                self.push("(");
                self.print_expr(inner, 0);
                self.push(")");
            }
            Expr::ListLit {
                elements,
                elem_trivia,
                trailing_trivia,
                ..
            } => {
                self.push("[");
                if Self::elem_seq_multiline(elem_trivia, trailing_trivia) {
                    self.print_elem_seq_multiline(
                        elements.len(),
                        elem_trivia,
                        trailing_trivia,
                        |p, i| p.print_expr(&elements[i], 0),
                    );
                } else {
                    for (i, el) in elements.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.print_expr(el, 0);
                    }
                }
                self.push("]");
            }
            Expr::Member { recv, name, .. } => {
                let mark = self.buf.len();
                self.print_expr(recv, MEMBER_BP);
                // A numeric member segment (`steps.1` label access) glued
                // to a receiver that rendered digit-last re-lexes as a
                // float (`8 . 80` → `8.80`, `x.0.80` → `x` . `0.80`) —
                // parenthesize the receiver so the member chain survives.
                if name.as_bytes().first().is_some_and(|b| b.is_ascii_digit())
                    && self
                        .buf
                        .as_bytes()
                        .last()
                        .is_some_and(|b| b.is_ascii_digit())
                    && self.buf.len() > mark
                {
                    self.buf.insert(mark, '(');
                    self.buf.push(')');
                }
                // A negative numeric segment (`x. -2`) needs the space:
                // flush against the dot, `-` is not a valid member start
                // (the signed-number lexer form requires a separator).
                if name.starts_with('-') {
                    self.push(". ");
                } else {
                    self.push(".");
                }
                self.push(name);
            }
            Expr::Call {
                callee,
                args,
                arg_trivia,
                trailing_trivia,
                ..
            } => {
                self.print_expr(callee, CALL_BP);
                self.push("(");
                if Self::elem_seq_multiline(arg_trivia, trailing_trivia) {
                    self.print_elem_seq_multiline(
                        args.len(),
                        arg_trivia,
                        trailing_trivia,
                        |p, i| p.print_expr(&args[i], 0),
                    );
                } else {
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.print_expr(a, 0);
                    }
                }
                self.push(")");
            }
            Expr::Unary { op, operand, .. } => {
                // A `-` printed flush against digits re-lexes as one signed
                // literal, which drops the sign of integer zero (`- 0` →
                // `-0` → `0` on the next pass) and rejects unsigned
                // suffixes (`- 5u8` → `-5u8` → parse error). Fold the
                // negation into signed/float literals; parenthesize the
                // numeric operands that can't absorb it (unsigned,
                // unit literals, `iN::MIN`).
                if matches!(op, UnaryOp::Neg)
                    && let Some(folded) = fold_neg(operand)
                {
                    self.print_expr(&folded, min_bp);
                } else {
                    self.push(match op {
                        UnaryOp::Neg => "-",
                        UnaryOp::Not => "!",
                    });
                    let mark = self.buf.len();
                    self.print_expr(operand, UNARY_BP);
                    // Any operand that rendered digit-first glues onto
                    // the `-` (`- 0 . u3` → `-0.u3`, whose zero re-lexes
                    // as a *signed* literal and drops the negation) —
                    // parenthesize it. Checking the rendered text covers
                    // every such shape: unsigned/unit literals, `iN::MIN`,
                    // member access or calls on a numeric literal, ….
                    if matches!(op, UnaryOp::Neg)
                        && self
                            .buf
                            .as_bytes()
                            .get(mark)
                            .is_some_and(|b| b.is_ascii_digit())
                    {
                        self.buf.insert(mark, '(');
                        self.buf.push(')');
                    }
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let (lbp, rbp) = op.binding_power();
                let need_parens = lbp < min_bp;
                if need_parens {
                    self.push("(");
                }
                self.print_expr(lhs, lbp);
                self.push_ch(' ');
                self.push(op.as_str());
                self.push_ch(' ');
                self.print_expr(rhs, rbp);
                if need_parens {
                    self.push(")");
                }
            }

            Expr::Block {
                lets,
                tail,
                trailing_trivia,
                ..
            } => self.print_block_expr(lets, tail, trailing_trivia),
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.push("if ");
                self.print_expr(cond, 0);
                self.push_ch(' ');
                self.print_expr(then_block, 0);
                self.push(" else ");
                self.print_expr(else_block, 0);
            }
            Expr::IfLet {
                pattern,
                scrut,
                then_block,
                else_block,
                ..
            } => {
                self.push("if let ");
                self.print_pattern(pattern);
                self.push(" = ");
                self.print_expr(scrut, 0);
                self.push_ch(' ');
                self.print_expr(then_block, 0);
                self.push(" else ");
                self.print_expr(else_block, 0);
            }
            Expr::Match {
                scrut,
                arms,
                trailing_trivia,
                ..
            } => self.print_match_expr(scrut, arms, trailing_trivia),
            Expr::Variant {
                type_path,
                variant,
                args,
                ..
            } => self.print_variant_expr(type_path, variant, args),
            Expr::Record {
                fields,
                trailing_trivia,
                ..
            } => {
                // Bare record literal — `field: value` pairs, mirroring
                // the variant-constructor record body so reparse is stable.
                self.print_record_fields(fields, trailing_trivia);
            }

            Expr::Function(f) => self.print_function_literal(f),
            Expr::Try {
                body,
                binder,
                handler,
                ..
            } => {
                // A try expression extends through its handler, so any
                // surrounding operator context needs parens to re-parse
                // with the same shape.
                let need_parens = min_bp > 0;
                if need_parens {
                    self.push("(");
                }
                self.push("try ");
                self.print_expr(body, 0);
                self.push(" catch ");
                self.push(binder);
                self.push(" => ");
                self.print_expr(handler, 0);
                if need_parens {
                    self.push(")");
                }
            }
        }
    }

    fn print_float(&mut self, v: f64) {
        // Infinity has no literal form — an overflowing literal
        // (`1.5E555`) saturates to it, and Debug's `inf` re-lexes as an
        // *identifier*. Emit an overflowing literal instead; it parses
        // back to the same value.
        if v.is_infinite() {
            self.push(if v < 0.0 { "-1.0e999" } else { "1.0e999" });
            return;
        }
        // Use Debug so finite floats round-trip; ensure a `.` is
        // always present so `2.0` doesn't get printed as `2` (which
        // would re-parse as an integer). Debug prints small/large
        // magnitudes in exponent form *without* a dot (`2e-6`), which
        // the lexer rejects — splice a `.0` back in before the
        // exponent so the literal re-parses (`2.0e-6`).
        let s = format!("{v:?}");
        if let Some(epos) = s.find(['e', 'E'])
            && !s[..epos].contains('.')
        {
            self.push(&s[..epos]);
            self.push(".0");
            self.push(&s[epos..]);
        } else {
            self.push(&s);
        }
    }

    fn print_string_lit(&mut self, body: &str, encoding: StringEncoding) {
        self.print_string_lit_in(body, encoding, false);
    }

    fn print_string_lit_in(&mut self, body: &str, encoding: StringEncoding, allow_heredoc: bool) {
        // Heredoc form only where it is *legal* (`allow_heredoc`: a
        // statement-level value followed by a bare newline), *value-
        // preserving* (`heredoc_round_trips`: re-parsing the emitted
        // heredoc reproduces the body exactly), and *worth it* (two or
        // more lines — `"\n"`-ish separator strings stay escaped
        // literals). Everything else prints as a quoted literal with
        // escapes, which always round-trips.
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        if allow_heredoc && heredoc_round_trips(body) && body.lines().count() >= 2 {
            self.print_heredoc(body, prefix, false);
        } else {
            self.push(prefix);
            self.push("\"");
            self.push(&EscapeString(body).to_string());
            self.push("\"");
        }
    }

    fn print_interpolated(
        &mut self,
        encoding: StringEncoding,
        parts: &[TemplatePart],
        allow_heredoc: bool,
    ) {
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        // A "skeleton" of the body — literal text with each `${…}` slot
        // standing in as a single placeholder character — drives the
        // style choice: the heredoc form is used only where it is legal
        // (`allow_heredoc`), the body round-trips through heredoc
        // line/indent handling, the *last* part ends with a newline (the
        // closing tag must start its own line — a body that was
        // authored with `\n` escapes and doesn't end on one cannot be a
        // heredoc), and the text is genuinely multi-line.
        let mut skeleton = String::new();
        for part in parts {
            match part {
                TemplatePart::Literal(s) => skeleton.push_str(s),
                TemplatePart::Expr(_) => skeleton.push('x'),
            }
        }
        let ends_on_newline = matches!(
            parts.last(),
            Some(TemplatePart::Literal(s)) if s.ends_with('\n')
        );
        self.push("$");
        self.push(prefix);
        if allow_heredoc
            && ends_on_newline
            && heredoc_round_trips(&skeleton)
            && skeleton.lines().count() >= 2
        {
            // Pick a tag no body line could close early on ("INTERP"
            // first to keep existing formatted files stable).
            let tag = pick_heredoc_tag_preferring(&skeleton, "INTERP");
            self.push("<<");
            self.push(&tag);
            self.push("\n");
            for part in parts {
                match part {
                    // The interpolated heredoc body is escape-
                    // interpreted: a literal backslash must double and a
                    // literal `${` must re-escape to `\${`, or the text
                    // re-parses as an escape / a slot.
                    TemplatePart::Literal(s) => {
                        self.push(&s.replace('\\', "\\\\").replace("${", "\\${"));
                    }
                    TemplatePart::Expr(e) => {
                        self.push("${");
                        self.print_expr(e, 0);
                        self.push("}");
                    }
                }
            }
            // The final literal ends with `\n` (checked above), so the
            // closing tag starts its own line. Don't add another — that
            // would creep one extra blank line in on every reformat.
            self.push(&tag);
        } else {
            self.push("\"");
            for part in parts {
                match part {
                    // `EscapeString` covers quotes / backslashes /
                    // control characters but not `${` — in an
                    // *interpolated* literal that sequence must
                    // re-escape to `\${` or it re-parses as a slot.
                    // (Safe after EscapeString: it never produces `${`
                    // from other characters.)
                    TemplatePart::Literal(s) => {
                        self.push(&EscapeString(s).to_string().replace("${", "\\${"));
                    }
                    TemplatePart::Expr(e) => {
                        self.push("${");
                        self.print_expr(e, 0);
                        self.push("}");
                    }
                }
            }
            self.push("\"");
        }
    }

    fn print_heredoc(&mut self, body: &str, prefix: &str, _interpolated: bool) {
        // Indent each body line at depth + 1 so parse-time indent
        // stripping recovers the original content. The trailing newline
        // is significant — the parser adds one per line, so the
        // round-trip value ends with `\n`.
        let body_indent = self.indent_str.repeat((self.depth + 1) as usize);
        let closer_indent = self.indent_str.repeat(self.depth as usize);
        // Pick a tag that doesn't collide with a (trimmed) body line,
        // so the closer can't trigger early.
        let tag = pick_heredoc_tag(body);

        // A plain `<<TAG` body is escape-interpreted on re-parse, so a
        // backslash would break the round-trip (`\f` → invalid escape).
        // For utf8 bodies emit a raw `<<'TAG'` heredoc — body taken
        // verbatim, which also keeps backslash-heavy text (LaTeX,
        // regexes) readable. The rarer typed-encoding heredocs fall back
        // to a plain heredoc with backslashes escaped so the value still
        // round-trips.
        if prefix.is_empty() && body.contains('\\') {
            self.push("<<'");
            self.push(&tag);
            self.push("'\n");
            for line in body.split_inclusive('\n') {
                self.push(&body_indent);
                self.push(line);
            }
            self.push(&closer_indent);
            self.push(&tag);
            return;
        }

        self.push(prefix);
        self.push("<<");
        self.push(&tag);
        self.push("\n");
        for line in body.split_inclusive('\n') {
            self.push(&body_indent);
            // Escape backslashes only; the literal `\n` stays a line break.
            self.push(&line.replace('\\', "\\\\"));
        }
        self.push(&closer_indent);
        self.push(&tag);
    }

    /// True when a comma-separated collection of bare-`Expr` elements
    /// must break onto multiple lines to carry its comments.
    fn elem_seq_multiline(elem_trivia: &[ElemTrivia], trailing_trivia: &[Trivia]) -> bool {
        elem_trivia.iter().any(ElemTrivia::has_comment) || trivia_has_comment(trailing_trivia)
    }

    /// Emit the multi-line body of a bracket/paren collection: a newline
    /// after the (already-pushed) opening delimiter, one element per line
    /// at the next indent level with its leading trivia and trailing
    /// comment, then the trailing trivia and the closing-delimiter indent.
    /// The caller pushes the opening and closing delimiters.
    fn print_elem_seq_multiline(
        &mut self,
        len: usize,
        elem_trivia: &[ElemTrivia],
        trailing_trivia: &[Trivia],
        mut print_elem: impl FnMut(&mut Self, usize),
    ) {
        self.newline();
        self.depth += 1;
        for i in 0..len {
            if let Some(t) = elem_trivia.get(i) {
                self.print_leading_trivia(&t.leading);
            }
            self.write_indent();
            print_elem(self, i);
            self.push(",");
            if let Some(c) = elem_trivia.get(i).and_then(|t| t.trailing.as_ref()) {
                self.push("  # ");
                self.push(c);
            }
            self.newline();
        }
        self.print_leading_trivia(trailing_trivia);
        self.depth -= 1;
        self.write_indent();
    }

    /// Print a `{ field: value, … }` record body, shared by bare record
    /// literals and variant record constructors. Breaks onto multiple
    /// lines (one field per line, with a trailing comma) when any field
    /// or the pre-`}` position carries a line comment; otherwise stays on
    /// one line in the canonical `{ a: 1, b: 2 }` form. The caller has
    /// already emitted any leading space before the `{`.
    fn print_record_fields(&mut self, fields: &[NamedArg], trailing_trivia: &[Trivia]) {
        if fields.is_empty() {
            self.push("{}");
            return;
        }
        let multiline = fields
            .iter()
            .any(|f| f.trailing_comment.is_some() || trivia_has_comment(&f.leading_trivia))
            || trivia_has_comment(trailing_trivia);
        if multiline {
            self.push("{");
            self.newline();
            self.depth += 1;
            for f in fields {
                self.print_leading_trivia(&f.leading_trivia);
                self.write_indent();
                self.push(&f.name);
                self.push(": ");
                self.print_expr(&f.value, 0);
                self.push(",");
                self.print_trailing_comment(&f.trailing_comment);
                self.newline();
            }
            self.print_leading_trivia(trailing_trivia);
            self.depth -= 1;
            self.write_indent();
            self.push("}");
        } else {
            self.push("{ ");
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&f.name);
                self.push(": ");
                self.print_expr(&f.value, 0);
            }
            self.push(" }");
        }
    }

    fn print_block_expr(&mut self, lets: &[LetBinding], tail: &Expr, trailing_trivia: &[Trivia]) {
        let has_comment = trivia_has_comment(trailing_trivia)
            || lets
                .iter()
                .any(|b| b.trailing_comment.is_some() || trivia_has_comment(&b.leading_trivia));
        if lets.is_empty() && !has_comment {
            // A bare `{ expr }` block — print on one line.
            self.push("{ ");
            self.print_expr(tail, 0);
            self.push(" }");
            return;
        }
        self.push("{");
        self.newline();
        self.depth += 1;
        for b in lets {
            self.print_leading_trivia(&b.leading_trivia);
            self.write_indent();
            self.push("let ");
            self.push(&b.name);
            self.push(" = ");
            self.print_expr(&b.value, 0);
            self.push(";");
            self.print_trailing_comment(&b.trailing_comment);
            self.newline();
        }
        // Comments that sat between the last binding and the tail (or
        // before the closing `}`) print above the tail expression.
        self.print_leading_trivia(trailing_trivia);
        self.write_indent();
        self.print_expr(tail, 0);
        self.newline();
        self.depth -= 1;
        self.write_indent();
        self.push("}");
    }

    fn print_match_expr(&mut self, scrut: &Expr, arms: &[MatchArm], trailing_trivia: &[Trivia]) {
        self.push("match ");
        self.print_expr(scrut, 0);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for arm in arms {
            self.print_leading_trivia(&arm.leading_trivia);
            self.write_indent();
            for (i, pat) in arm.patterns.iter().enumerate() {
                if i > 0 {
                    self.push(" | ");
                }
                self.print_pattern(pat);
            }
            if let Some(g) = &arm.guard {
                self.push(" if ");
                self.print_expr(g, 0);
            }
            self.push(" => ");
            self.print_expr(&arm.body, 0);
            // The parser accepts a trailing comma before the closing
            // brace; `FormatConfig::trailing_comma_in_match` flips
            // whether the printer emits one. Off-by-default keeps the
            // historical canonical form.
            if self.cfg.trailing_comma_in_match {
                self.push(",");
            }
            self.print_trailing_comment(&arm.trailing_comment);
            self.newline();
        }
        self.print_leading_trivia(trailing_trivia);
        self.depth -= 1;
        self.write_indent();
        self.push("}");
    }

    fn print_variant_expr(&mut self, type_path: &[String], variant: &str, args: &VariantArgs) {
        if !type_path.is_empty() {
            self.push(&join_path(type_path));
            self.push("::");
        }
        self.push(variant);
        match args {
            VariantArgs::Unit => {}
            VariantArgs::Positional(inner) => {
                self.push("(");
                self.print_expr(inner, 0);
                self.push(")");
            }
            VariantArgs::Record {
                fields,
                trailing_trivia,
            } => {
                // Variant *constructors* use `field: value` separated
                // by commas (not `=`). The record-pattern printer above
                // uses the same shape.
                self.push_ch(' ');
                self.print_record_fields(fields, trailing_trivia);
            }
        }
    }

    fn print_function_literal(&mut self, f: &FunctionLit) {
        self.push("fn");
        self.print_function_signature_and_body(f);
    }

    /// Print a function literal's `(params) -> T body` — everything after
    /// the `fn` keyword. Shared by expression literals and `fn name(…)`
    /// items (which splice the name between `fn` and the parameters).
    fn print_function_signature_and_body(&mut self, f: &FunctionLit) {
        let multiline = f
            .params
            .iter()
            .any(|p| p.trailing_comment.is_some() || trivia_has_comment(&p.leading_trivia))
            || trivia_has_comment(&f.trailing_trivia);
        self.push("(");
        if multiline {
            self.newline();
            self.depth += 1;
            for p in &f.params {
                self.print_leading_trivia(&p.leading_trivia);
                self.write_indent();
                self.print_parameter(p);
                self.push(",");
                self.print_trailing_comment(&p.trailing_comment);
                self.newline();
            }
            self.print_leading_trivia(&f.trailing_trivia);
            self.depth -= 1;
            self.write_indent();
        } else {
            for (i, p) in f.params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.print_parameter(p);
            }
        }
        self.push(") -> ");
        self.print_type_ref(&f.return_ty);
        self.push_ch(' ');
        self.print_expr(&f.body, 0);
    }

    fn print_parameter(&mut self, p: &Parameter) {
        self.push(&p.name);
        self.push(": ");
        self.print_type_ref(&p.ty);
    }

    // ---------- patterns ----------

    fn print_pattern(&mut self, p: &Pattern) {
        match p {
            Pattern::Wildcard(_) => self.push("_"),
            Pattern::Binding { name, .. } => self.push(name),
            Pattern::At { name, inner, .. } => {
                self.push(name);
                self.push(" @ ");
                self.print_pattern(inner);
            }
            Pattern::LiteralBool(b, _) => self.push(if *b { "true" } else { "false" }),
            Pattern::LiteralNumber { lit, .. } => {
                // NumberLit's Debug renders the value with its
                // typed-variant suffix, but that's not the source
                // form. Map to the same logic as Expr literals.
                let synthesized = number_lit_to_expr(lit);
                self.print_expr(&synthesized, 0);
            }
            Pattern::LiteralUtf8(s, _) => self.print_string_lit(s, StringEncoding::Utf8),
            Pattern::LiteralAscii(s, _) => self.print_string_lit(s, StringEncoding::Ascii),
            Pattern::LiteralSymbol(s, _) => {
                self.push(":");
                self.push(s);
            }
            Pattern::LiteralNone(_) => self.push("none"),
            Pattern::Variant {
                type_path,
                variant,
                args,
                ..
            } => {
                if !type_path.is_empty() {
                    self.push(&join_path(type_path));
                    self.push("::");
                }
                self.push(variant);
                match args {
                    VariantPatArgs::Unit => {}
                    VariantPatArgs::Positional(inner) => {
                        self.push("(");
                        self.print_pattern(inner);
                        self.push(")");
                    }
                    VariantPatArgs::Record { fields, rest } => {
                        self.push(" { ");
                        for (i, (name, pat)) in fields.iter().enumerate() {
                            if i > 0 {
                                self.push(", ");
                            }
                            self.push(name);
                            self.push(": ");
                            self.print_pattern(pat);
                        }
                        if *rest {
                            if !fields.is_empty() {
                                self.push(", ");
                            }
                            self.push("..");
                        }
                        self.push(" }");
                    }
                }
            }
        }
    }

    // ---------- type refs ----------

    fn print_type_ref(&mut self, t: &TypeRef) {
        match t {
            TypeRef::Builtin(b) => self.push(builtin_name(*b)),
            TypeRef::Named(path) => self.push(&path.join(".")),
            TypeRef::Reference(inner) => {
                self.push("&");
                self.print_type_ref(inner);
            }
            TypeRef::List(inner) => {
                self.push("list<");
                self.print_type_ref(inner);
                self.push(">");
            }
            TypeRef::Tensor { element, dims } => {
                self.push("tensor<");
                self.print_type_ref(element);
                self.push(", [");
                for (i, d) in dims.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    match d {
                        TensorDim::Fixed(n) => {
                            let _ = write!(self.buf, "{n}");
                        }
                        TensorDim::Symbolic(s) => self.push(s),
                    }
                }
                self.push("]>");
            }
            TypeRef::Function { params, return_ty } => {
                self.push("fn(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.print_type_ref(p);
                }
                self.push(") -> ");
                self.print_type_ref(return_ty);
            }
        }
    }
}

fn builtin_name(b: BuiltinType) -> &'static str {
    b.name()
}

/// True when a trivia run contains at least one line comment (blank
/// lines alone don't force a single-line collection to break).
fn trivia_has_comment(trivia: &[Trivia]) -> bool {
    trivia.iter().any(|t| matches!(t, Trivia::LineComment(_)))
}

fn join_path(parts: &[String]) -> String {
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
fn field_key(name: &str) -> String {
    if is_bare_ident(name) {
        name.to_string()
    } else {
        format!("\"{}\"", EscapeString(name))
    }
}

/// Whether `name` is a valid bare WCL identifier — mirrors the lexer's
/// `is_ident_start` / `is_ident_cont` (ASCII letter/underscore start, then
/// ASCII alphanumeric/underscore).
fn is_bare_ident(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Greedy-wrap single-line prose at `width` columns, breaking only at
/// spaces that sit outside inline-markup constructs, and return the
/// heredoc-shaped body (lines joined with `\n`, trailing `\n`).
///
/// The stdlib inline patterns (bold / italic / code / link / math)
/// deliberately don't match across `\n`, so a break inside `**…**` or
/// `[…](…)` would change rendering. The scanner below marks those spans
/// unbreakable; a false positive (e.g. two incidental underscores) only
/// costs a break opportunity, never correctness. A word longer than
/// `width` (a URL) is left on its own over-long line rather than split.
fn wrap_prose(text: &str, width: usize) -> String {
    let breaks = safe_break_points(text);
    let cols: Vec<usize> = {
        // Byte index → visual column (chars before it), for width checks.
        let mut v = vec![0; text.len() + 1];
        for (col, (i, c)) in text.char_indices().enumerate() {
            v[i] = col;
            for b in 1..c.len_utf8() {
                v[i + b] = col;
            }
        }
        v[text.len()] = text.chars().count();
        v
    };

    let mut out = String::with_capacity(text.len() + 8);
    let mut start = 0usize;
    while start < text.len() {
        let line_end_col = cols[start] + width;
        // The last safe break within the width, else the first one past it.
        let within = breaks
            .iter()
            .filter(|&&b| b > start && cols[b] <= line_end_col)
            .max()
            .copied();
        let past = breaks.iter().filter(|&&b| b > start).min().copied();
        let cut = match (cols[text.len()] <= line_end_col, within, past) {
            (true, ..) => None, // the rest fits
            (false, Some(b), _) | (false, None, Some(b)) => Some(b),
            (false, None, None) => None,
        };
        match cut {
            Some(b) => {
                out.push_str(text[start..b].trim_end());
                out.push('\n');
                start = b + 1; // consume the break space…
                while text.as_bytes().get(start) == Some(&b' ') {
                    start += 1; // …and any run of spaces after it
                }
            }
            None => {
                out.push_str(text[start..].trim_end());
                out.push('\n');
                break;
            }
        }
    }
    out
}

/// Byte offsets of the spaces in `text` where a line break is safe:
/// outside inline code spans, bold/italic runs, links, and math — and
/// not after a `>` (the blockquote pattern styles to end-of-line, so a
/// break would move where the quote ends).
fn safe_break_points(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut in_code = false; // `…`
    let mut in_bold = false; // **…**
    let mut in_italic = false; // _…_
    let mut in_math = false; // $…$ / $$…$$
    let mut link_depth = 0u32; // […](…) — [ to the closing )
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'`' => in_code = !in_code,
            _ if in_code => {} // a code span hides every other marker
            b'>' => break,     // nothing after a `>` is a safe break
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                in_bold = !in_bold;
                i += 1;
            }
            b'_' if text[i + 1..].contains('_') || in_italic => in_italic = !in_italic,
            b'$' => in_math = !in_math,
            b'[' => link_depth += 1,
            // `](` continues the link into its target; a bare `]` ends it.
            b']' if link_depth > 0 && bytes.get(i + 1) != Some(&b'(') => link_depth -= 1,
            b')' if link_depth > 0 => link_depth -= 1,
            b' ' if !in_bold && !in_italic && !in_math && link_depth == 0 => out.push(i),
            _ => {}
        }
        i += 1;
    }
    out
}

/// `true` when `body` would survive a heredoc round-trip exactly.
/// Heredoc parsing imposes three constraints a quoted literal doesn't:
///
/// - every body line contributes `content + '\n'`, so a value that
///   doesn't end with a newline is unrepresentable (the closing tag
///   would glue onto the last line and never close);
/// - the minimum leading whitespace across non-blank lines is stripped
///   from every line, so a body whose lines *all* start with whitespace
///   loses it on re-parse;
/// - whitespace-only lines are blanked entirely, so a line of spaces
///   loses them.
fn heredoc_round_trips(body: &str) -> bool {
    if !body.ends_with('\n') {
        return false;
    }
    let mut any_nonblank = false;
    let mut any_zero_indent = false;
    for line in body.lines() {
        if line.trim().is_empty() {
            if !line.is_empty() {
                // Whitespace-only line: blanked on re-parse.
                return false;
            }
        } else {
            any_nonblank = true;
            if !line.starts_with([' ', '\t']) {
                any_zero_indent = true;
            }
        }
    }
    !any_nonblank || any_zero_indent
}

/// [`pick_heredoc_tag`] with a preferred first candidate (the
/// interpolated form keeps its historical `INTERP` tag when possible).
fn pick_heredoc_tag_preferring(body: &str, preferred: &str) -> String {
    let lines: Vec<&str> = body.lines().map(str::trim).collect();
    if !lines.contains(&preferred) {
        return preferred.to_string();
    }
    pick_heredoc_tag(body)
}

/// Choose a heredoc tag that no (trimmed) body line equals, so the
/// closer line can't fire early. Falls back to a numbered tag in the
/// pathological case where every candidate appears in the body.
fn pick_heredoc_tag(body: &str) -> String {
    let lines: std::collections::HashSet<&str> = body.lines().map(str::trim).collect();
    for cand in ["DOC", "TEX", "RAW", "MATH", "BODY", "END", "HEREDOC"] {
        if !lines.contains(cand) {
            return cand.to_string();
        }
    }
    let mut i = 0;
    loop {
        let t = format!("DOC{i}");
        if !lines.contains(t.as_str()) {
            return t;
        }
        i += 1;
    }
}

/// Build a synthetic `Expr` from a `NumberLit` so a numeric *pattern*
/// reuses the same suffix-aware printing as a numeric *expression*.
fn number_lit_to_expr(n: &crate::lexer::NumberLit) -> Expr {
    use crate::lexer::NumberLit as N;
    match *n {
        N::I8(v) => Expr::I8(v),
        N::I16(v) => Expr::I16(v),
        N::I32(v) => Expr::I32(v),
        N::I64(v) => Expr::I64(v),
        N::I128(v) => Expr::I128(v),
        N::Isize(v) => Expr::Isize(v),
        N::U8(v) => Expr::U8(v),
        N::U16(v) => Expr::U16(v),
        N::U32(v) => Expr::U32(v),
        N::U64(v) => Expr::U64(v),
        N::U128(v) => Expr::U128(v),
        N::Usize(v) => Expr::Usize(v),
        N::F32(v) => Expr::F32(v),
        N::F64(v) => Expr::F64(v),
    }
}

/// Negation folded into a numeric literal, when the result is exactly
/// representable: signed ints via `checked_neg` (so `iN::MIN` bails),
/// floats always, unsigned only at zero (where `-` is a no-op). Double
/// negation over a foldable literal cancels. `None` means the caller
/// must print the `-` some other way.
fn fold_neg(e: &Expr) -> Option<Expr> {
    Some(match e {
        Expr::I8(v) => Expr::I8(v.checked_neg()?),
        Expr::I16(v) => Expr::I16(v.checked_neg()?),
        Expr::I32(v) => Expr::I32(v.checked_neg()?),
        Expr::I64(v) => Expr::I64(v.checked_neg()?),
        Expr::I128(v) => Expr::I128(v.checked_neg()?),
        Expr::Isize(v) => Expr::Isize(v.checked_neg()?),
        Expr::U8(0) => Expr::U8(0),
        Expr::U16(0) => Expr::U16(0),
        Expr::U32(0) => Expr::U32(0),
        Expr::U64(0) => Expr::U64(0),
        Expr::U128(0) => Expr::U128(0),
        Expr::Usize(0) => Expr::Usize(0),
        Expr::F32(v) => Expr::F32(-v),
        Expr::F64(v) => Expr::F64(-v),
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
            ..
        } if fold_neg(operand).is_some() => (**operand).clone(),
        _ => return None,
    })
}
