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
    BinOp, Block, ConnectionDecl, ConnectionStmt, Decorator, Expr, Field, FunctionLit, ImportDecl,
    InterfaceDecl, Item, LetBinding, MatchArm, NamedArg, NamespaceDecl, Parameter, Pattern, Row,
    Source, SymbolEntry, SymbolSetDecl, TableItem, TemplatePart, Trivia, TypeDecl, TypeField,
    UnaryOp, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem, VariantArgs, VariantBody,
    VariantPatArgs,
};
use crate::lexer::StringEncoding;
use crate::value::{BuiltinType, TensorDim, TypeRef};

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
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            trailing_comma_in_match: true,
            blank_line_cap: 1,
        }
    }
}

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

struct Printer {
    buf: String,
    depth: u16,
    cfg: FormatConfig,
    indent_str: String,
}

impl Printer {
    fn new(cfg: FormatConfig) -> Self {
        let indent_str = " ".repeat(cfg.indent);
        Self {
            buf: String::new(),
            depth: 0,
            cfg,
            indent_str,
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

    // ---------- source / items ----------

    fn print_source(&mut self, s: &Source) {
        for item in &s.items {
            self.print_item(item);
        }
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
        self.push(&f.name);
        self.push(" = ");
        self.print_expr(&f.expr, 0);
        self.newline();
    }

    fn print_let_item(&mut self, l: &crate::ast::LetItem) {
        self.print_leading_trivia(&l.leading_trivia);
        self.write_indent();
        self.push("let ");
        self.push(&l.name);
        self.push(" = ");
        self.print_expr(&l.value, 0);
        self.newline();
    }

    fn print_block(&mut self, b: &Block) {
        self.print_leading_trivia(&b.leading_trivia);
        self.write_indent();
        self.print_decorators_inline(&b.decorators);
        self.push(&b.kind);
        for label in &b.labels {
            self.push_ch(' ');
            self.print_expr(label, 0);
        }
        // Empty-body shorthand: omit `{}` entirely when there are no items.
        // The parser accepts both `kind labels` (no braces) and
        // `kind labels {}` (explicit empty braces); the canonical form is
        // the shorter one.
        if b.items.is_empty() {
            self.newline();
            return;
        }
        self.push(" {");
        self.newline();
        self.depth += 1;
        for item in &b.items {
            self.print_item(item);
        }
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.newline();
    }

    fn print_type_decl(&mut self, t: &TypeDecl) {
        self.print_leading_trivia(&t.leading_trivia);
        self.print_decorators_block(&t.decorators);
        self.write_indent();
        self.push("type ");
        self.push(&join_path(&t.name));
        self.print_extends(&t.extends);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for f in &t.fields {
            self.print_type_field(f);
        }
        self.depth -= 1;
        self.write_indent();
        self.push("}");
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
        self.depth -= 1;
        self.write_indent();
        self.push("}");
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
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.newline();
    }

    fn print_union_variant(&mut self, v: &UnionVariant) {
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
            VariantBody::Record(fields) => {
                self.push(" {");
                self.newline();
                self.depth += 1;
                for f in fields {
                    self.print_type_field(f);
                }
                self.depth -= 1;
                self.write_indent();
                self.push("}");
            }
        }
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
        self.depth -= 1;
        self.write_indent();
        self.push("}");
        self.newline();
    }

    fn print_symbol_entry(&mut self, s: &SymbolEntry) {
        self.print_decorators_block(&s.decorators);
        self.write_indent();
        self.push(&s.name);
        self.newline();
    }

    fn print_namespace_decl(&mut self, n: &NamespaceDecl) {
        self.print_leading_trivia(&n.leading_trivia);
        self.write_indent();
        self.push("namespace ");
        self.push(&join_path(&n.path));
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
        self.newline();
    }

    fn print_table_item(&mut self, t: &TableItem) {
        self.print_leading_trivia(&t.leading_trivia);
        self.write_indent();
        self.push(&t.field_name);
        self.push(":");
        self.newline();
        self.depth += 1;
        for r in &t.rows {
            self.print_row(r);
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
        self.newline();
    }

    fn print_connection_decl(&mut self, c: &ConnectionDecl) {
        self.print_leading_trivia(&c.leading_trivia);
        self.write_indent();
        self.push("connection ");
        self.push(&join_path(&c.name));
        self.push(" : ");
        self.print_type_ref(&c.source);
        self.push(" -> ");
        self.print_type_ref(&c.destination);
        self.push(" : ");
        self.push(&join_path(&c.kind_set));
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

            // ----- strings -----
            Expr::Utf8(s) => self.print_string_lit(s, StringEncoding::Utf8),
            Expr::Ascii(s) => {
                let utf8 = s.clone();
                self.print_string_lit(&utf8, StringEncoding::Ascii);
            }
            Expr::Utf16(units) => {
                let s = String::from_utf16_lossy(units);
                self.print_string_lit(&s, StringEncoding::Utf16);
            }
            Expr::Utf32(chars) => {
                let s: String = chars.iter().collect();
                self.print_string_lit(&s, StringEncoding::Utf32);
            }
            Expr::InterpolatedString {
                encoding, parts, ..
            } => self.print_interpolated(*encoding, parts),

            // ----- composites -----
            Expr::Paren { inner, .. } => {
                self.push("(");
                self.print_expr(inner, 0);
                self.push(")");
            }
            Expr::ListLit { elements, .. } => {
                self.push("[");
                for (i, el) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.print_expr(el, 0);
                }
                self.push("]");
            }
            Expr::Member { recv, name, .. } => {
                self.print_expr(recv, MEMBER_BP);
                self.push(".");
                self.push(name);
            }
            Expr::Call { callee, args, .. } => {
                self.print_expr(callee, CALL_BP);
                self.push("(");
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.print_expr(a, 0);
                }
                self.push(")");
            }
            Expr::Unary { op, operand, .. } => {
                self.push(match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                });
                self.print_expr(operand, UNARY_BP);
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let (lbp, rbp) = bin_op_bp(*op);
                let need_parens = lbp < min_bp;
                if need_parens {
                    self.push("(");
                }
                self.print_expr(lhs, lbp);
                self.push_ch(' ');
                self.push(bin_op_str(*op));
                self.push_ch(' ');
                self.print_expr(rhs, rbp);
                if need_parens {
                    self.push(")");
                }
            }

            Expr::Block { lets, tail, .. } => self.print_block_expr(lets, tail),
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
            Expr::Match { scrut, arms, .. } => self.print_match_expr(scrut, arms),
            Expr::Variant {
                type_path,
                variant,
                args,
                ..
            } => self.print_variant_expr(type_path, variant, args),
            Expr::Record { fields, .. } => {
                // Bare record literal — `field: value` pairs, mirroring
                // the variant-constructor record body so reparse is
                // stable. An empty field list prints `{}` (it can't be
                // produced by the parser, but stays round-trippable).
                if fields.is_empty() {
                    self.push("{}");
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

            Expr::Function(f) => self.print_function_literal(f),
        }
    }

    fn print_float(&mut self, v: f64) {
        // Use Debug so finite floats round-trip; ensure a `.` is
        // always present so `2.0` doesn't get printed as `2` (which
        // would re-parse as an integer).
        let s = format!("{v:?}");
        self.push(&s);
    }

    fn print_string_lit(&mut self, body: &str, encoding: StringEncoding) {
        // If the value contains a newline, emit a heredoc; otherwise a
        // quoted form. Either way, prefix with the encoding tag for
        // anything other than the default utf8.
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        if body.contains('\n') {
            self.print_heredoc(body, prefix, false);
        } else {
            self.push(prefix);
            self.push("\"");
            self.push(&escape_inline_string(body));
            self.push("\"");
        }
    }

    fn print_interpolated(&mut self, encoding: StringEncoding, parts: &[TemplatePart]) {
        let prefix = match encoding {
            StringEncoding::Utf8 => "",
            StringEncoding::Ascii => "ascii",
            StringEncoding::Utf16 => "utf16",
            StringEncoding::Utf32 => "utf32",
        };
        // Compose the literal-only body once so we know whether to
        // pick quoted or heredoc style. Slot text comes from a fresh
        // `to_source` over the slot expr.
        let has_newline = parts.iter().any(|p| match p {
            TemplatePart::Literal(s) => s.contains('\n'),
            TemplatePart::Expr(_) => false,
        });
        self.push("$");
        self.push(prefix);
        if has_newline {
            self.push("<<INTERP\n");
            for part in parts {
                match part {
                    TemplatePart::Literal(s) => self.push(s),
                    TemplatePart::Expr(e) => {
                        self.push("${");
                        self.print_expr(e, 0);
                        self.push("}");
                    }
                }
            }
            // The heredoc parser always emits a trailing `\n` on the
            // final body line, so the literal we just printed ends on
            // a newline. Don't add another — that would creep one
            // extra blank line in on every reformat.
            self.push("INTERP");
        } else {
            self.push("\"");
            for part in parts {
                match part {
                    TemplatePart::Literal(s) => self.push(&escape_inline_string(s)),
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

    fn print_block_expr(&mut self, lets: &[LetBinding], tail: &Expr) {
        if lets.is_empty() {
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
            self.write_indent();
            self.push("let ");
            self.push(&b.name);
            self.push(" = ");
            self.print_expr(&b.value, 0);
            self.push(";");
            self.newline();
        }
        self.write_indent();
        self.print_expr(tail, 0);
        self.newline();
        self.depth -= 1;
        self.write_indent();
        self.push("}");
    }

    fn print_match_expr(&mut self, scrut: &Expr, arms: &[MatchArm]) {
        self.push("match ");
        self.print_expr(scrut, 0);
        self.push(" {");
        self.newline();
        self.depth += 1;
        for arm in arms {
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
            self.newline();
        }
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
            VariantArgs::Record(fields) => {
                // Variant *constructors* use `field: value` separated
                // by commas (not `=`). The record-pattern printer above
                // uses the same shape.
                self.push(" { ");
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
    }

    fn print_function_literal(&mut self, f: &FunctionLit) {
        self.push("fn(");
        for (i, p) in f.params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.print_parameter(p);
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

fn join_path(parts: &[String]) -> String {
    parts.join(".")
}

fn bin_op_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// `(left_bp, right_bp)` for binary operators. Mirrors `bin_op_info`
/// in `parser/mod.rs` so a parse → print round-trip preserves
/// associativity / precedence.
fn bin_op_bp(op: BinOp) -> (u8, u8) {
    match op {
        BinOp::Or => (1, 2),
        BinOp::And => (3, 4),
        BinOp::Eq | BinOp::Ne => (5, 6),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => (7, 8),
        BinOp::Add | BinOp::Sub => (9, 10),
        BinOp::Mul | BinOp::Div | BinOp::Mod => (11, 12),
    }
}

/// Binding power that has to bind tighter than any binary op for a
/// receiver expression to not need parens. Matches the parser's
/// `MEMBER_BP` / `CALL_BP` / `UNARY_BP` constants.
const UNARY_BP: u8 = 13;
const CALL_BP: u8 = 14;
const MEMBER_BP: u8 = 15;

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

fn escape_inline_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
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
