//! Printing declarations and block-level items.
//!
//! The counterpart of [`parser::decls`](crate::parser) — what that
//! module parses, this one prints.

use crate::ast::*;
use crate::lexer::StringEncoding;
use crate::value::EscapeString;

use super::literals::{heredoc_round_trips, wrap_prose};
use super::{ITEM_STARTERS, Printer, TEXT_WRAP_KINDS, field_key, join_path};

impl Printer {
    /// Print a `name = expr` field.
    pub(super) fn print_field(&mut self, f: &Field) {
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

    /// Print a `let` item, or the `fn` sugar when it was written that way.
    pub(super) fn print_let_item(&mut self, l: &crate::ast::LetItem) {
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

    /// Print a block instance: kind, labels, then its body.
    pub(super) fn print_block(&mut self, b: &Block) {
        self.print_leading_trivia(&b.leading_trivia);
        self.write_indent();
        self.print_decorators_inline(&b.decorators);
        if let Some(slot) = &b.slot_decl {
            self.push("slot ");
            if let Some(name) = b.labels.first() {
                match name {
                    Expr::Identifier(name, _) => self.push(name),
                    other => self.print_expr(other, 0),
                }
            }
            self.push(": ");
            self.print_type_ref(&slot.ty);
            if slot.optional {
                self.push("?");
            } else if slot.repeated {
                self.push("*");
            }
            if let Some(Item::Field(default)) = b
                .items
                .iter()
                .find(|item| matches!(item, Item::Field(field) if field.name == "default"))
            {
                self.push(" = ");
                self.print_expr(&default.expr, 0);
            }
            self.print_trailing_comment(&b.trailing_comment);
            self.newline();
            return;
        }
        if !b.kind_ns.is_empty() {
            self.push(&b.kind_ns.join("."));
            self.push("::");
        }
        self.push(&b.kind);
        if b.conditional {
            self.push("?");
        }
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
    pub(super) fn heredoc_label(&self, b: &Block) -> Option<String> {
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

    /// Print a `type` declaration, or its alias form.
    pub(super) fn print_type_decl(&mut self, t: &TypeDecl) {
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

    /// Print an `interface` declaration.
    pub(super) fn print_interface_decl(&mut self, t: &InterfaceDecl) {
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

    /// Print a `union` declaration.
    pub(super) fn print_union_decl(&mut self, u: &UnionDecl) {
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

    /// Print one union variant and its payload.
    pub(super) fn print_union_variant(&mut self, v: &UnionVariant) {
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

    /// Print a `symbol_set` declaration.
    pub(super) fn print_symbol_set_decl(&mut self, s: &SymbolSetDecl) {
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

    /// Print one symbol of a symbol set.
    pub(super) fn print_symbol_entry(&mut self, s: &SymbolEntry) {
        self.print_leading_trivia(&s.leading_trivia);
        self.print_decorators_block(&s.decorators);
        self.write_indent();
        self.push(&s.name);
        self.print_trailing_comment(&s.trailing_comment);
        self.newline();
    }

    /// Print a `namespace` declaration.
    pub(super) fn print_namespace_decl(&mut self, n: &NamespaceDecl) {
        self.print_leading_trivia(&n.leading_trivia);
        self.write_indent();
        self.push("namespace ");
        self.push(&join_path(&n.path));
        self.print_trailing_comment(&n.trailing_comment);
        self.newline();
    }

    /// Print a `use` declaration, in whichever form it was written.
    pub(super) fn print_use_decl(&mut self, u: &UseDecl) {
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

    /// Print one name of a brace-list `use`.
    pub(super) fn print_use_item(&mut self, u: &UseItem) {
        self.push(&u.name);
        if let Some(alias) = &u.alias {
            self.push(" as ");
            self.push(alias);
        }
    }

    /// Print an `import`, quoted or angle-bracketed as declared.
    pub(super) fn print_import_decl(&mut self, i: &ImportDecl) {
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

    /// Print a table: its field name, then its rows.
    pub(super) fn print_table_item(&mut self, t: &TableItem) {
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

    /// Print one pipe-delimited table row.
    pub(super) fn print_row(&mut self, r: &Row) {
        self.write_indent();
        for v in &r.values {
            self.push("| ");
            self.print_expr(v, 0);
            self.push(" ");
        }
        self.push("|");
    }

    /// Print a `connection` declaration.
    pub(super) fn print_connection_decl(&mut self, c: &ConnectionDecl) {
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

    /// Print a connection statement.
    pub(super) fn print_connection_stmt(&mut self, c: &ConnectionStmt) {
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

    /// Print an `extends` clause, omitting it when the list is empty.
    pub(super) fn print_extends(&mut self, extends: &[Vec<String>]) {
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

    /// Print one field of a type, interface or record variant.
    pub(super) fn print_type_field(&mut self, f: &TypeField) {
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

    /// Decorators preceding an item that sits on its own line (type,
    /// interface, union, block, …): each decorator is emitted on its
    /// own preceding line. Indentation matches the item.
    pub(super) fn print_decorators_block(&mut self, decs: &[Decorator]) {
        for d in decs {
            self.write_indent();
            self.print_decorator(d);
            self.newline();
        }
    }

    /// Decorators preceding a single-line construct (field, type
    /// field, symbol entry, …): emitted inline, space-separated, in
    /// front of the construct. Caller writes the leading indent.
    pub(super) fn print_decorators_inline(&mut self, decs: &[Decorator]) {
        for d in decs {
            self.print_decorator(d);
            self.push_ch(' ');
        }
    }

    /// Print a decorator with its positional and named arguments.
    pub(super) fn print_decorator(&mut self, d: &Decorator) {
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

    /// Print a `name = value` pair.
    pub(super) fn print_named_arg(&mut self, n: &NamedArg) {
        self.push(&n.name);
        self.push(" = ");
        self.print_expr(&n.value, 0);
    }
}
