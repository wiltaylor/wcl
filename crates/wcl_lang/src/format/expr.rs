//! Printing expressions.
//!
//! The counterpart of [`parser::expr`](crate::parser). Parenthesisation
//! is driven by the same binding powers the parser uses
//! ([`BinOp::binding_power`]), so a parse → print round trip preserves
//! precedence by construction rather than by agreement.

use std::fmt::Write as _;

use crate::ast::*;
use crate::lexer::StringEncoding;

use super::{Printer, join_path, trivia_has_comment};

impl Printer {
    /// Pratt-style precedence printing: caller passes the minimum
    /// binding power required by the parent context. `Binary` wraps
    /// itself in parens when its left-bp falls below `min_bp`, so
    /// `(a + b) * c` survives round-trip.
    pub(super) fn print_expr(&mut self, e: &Expr, min_bp: u8) {
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
            Expr::Symbol(name, _) => {
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
                if !self.in_slot() && Self::elem_seq_multiline(elem_trivia, trailing_trivia) {
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
                if !self.in_slot() && Self::elem_seq_multiline(arg_trivia, trailing_trivia) {
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
                // An else-less `if` prints without one — the formatter
                // never spells out the implicit `none`.
                if let Some(else_block) = else_block {
                    self.push(" else ");
                    self.print_expr(else_block, 0);
                }
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

    /// Print a float so it re-parses as one — an integral value
    /// still gets a `.0`.
    pub(super) fn print_float(&mut self, v: f64) {
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

    /// True when a comma-separated collection of bare-`Expr` elements
    /// must break onto multiple lines to carry its comments.
    pub(super) fn elem_seq_multiline(
        elem_trivia: &[ElemTrivia],
        trailing_trivia: &[Trivia],
    ) -> bool {
        elem_trivia.iter().any(ElemTrivia::has_comment) || trivia_has_comment(trailing_trivia)
    }

    /// Emit the multi-line body of a bracket/paren collection: a newline
    /// after the (already-pushed) opening delimiter, one element per line
    /// at the next indent level with its leading trivia and trailing
    /// comment, then the trailing trivia and the closing-delimiter indent.
    /// The caller pushes the opening and closing delimiters.
    pub(super) fn print_elem_seq_multiline(
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
    pub(super) fn print_record_fields(&mut self, fields: &[NamedArg], trailing_trivia: &[Trivia]) {
        if fields.is_empty() {
            self.push("{}");
            return;
        }
        let multiline = !self.in_slot()
            && (fields
                .iter()
                .any(|f| f.trailing_comment.is_some() || trivia_has_comment(&f.leading_trivia))
                || trivia_has_comment(trailing_trivia));
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

    /// Print a block expression: its bindings, then its tail.
    pub(super) fn print_block_expr(
        &mut self,
        lets: &[LetBinding],
        tail: &Expr,
        trailing_trivia: &[Trivia],
    ) {
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
        // Inside an interpolation slot the bindings join on one line
        // (`{ let a = 1; tail }`) — the multi-line form can't re-parse.
        if self.in_slot() {
            self.push("{ ");
            for b in lets {
                self.push("let ");
                self.push(&b.name);
                self.push(" = ");
                self.print_expr(&b.value, 0);
                self.push("; ");
            }
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

    /// Print a `match` expression and its arms.
    pub(super) fn print_match_expr(
        &mut self,
        scrut: &Expr,
        arms: &[MatchArm],
        trailing_trivia: &[Trivia],
    ) {
        // Inside an interpolation slot the arms print comma-separated on
        // one line (trivia dropped — single-line source can't carry line
        // comments anyway).
        if self.in_slot() {
            self.push("match ");
            self.print_expr(scrut, 0);
            self.push(" { ");
            for (i, arm) in arms.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                for (j, pat) in arm.patterns.iter().enumerate() {
                    if j > 0 {
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
            }
            self.push(" }");
            return;
        }
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

    /// Print a variant constructor and its payload.
    pub(super) fn print_variant_expr(
        &mut self,
        type_path: &[String],
        variant: &str,
        args: &VariantArgs,
    ) {
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

    /// Print a function literal.
    pub(super) fn print_function_literal(&mut self, f: &FunctionLit) {
        self.push("fn");
        self.print_function_signature_and_body(f);
    }

    /// Print a function literal's `(params) -> T body` — everything after
    /// the `fn` keyword. Shared by expression literals and `fn name(…)`
    /// items (which splice the name between `fn` and the parameters).
    pub(super) fn print_function_signature_and_body(&mut self, f: &FunctionLit) {
        let multiline =
            !self.in_slot()
                && (f.params.iter().any(|p| {
                    p.trailing_comment.is_some() || trivia_has_comment(&p.leading_trivia)
                }) || trivia_has_comment(&f.trailing_trivia));
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

    /// Print one declared parameter.
    pub(super) fn print_parameter(&mut self, p: &Parameter) {
        self.push(&p.name);
        self.push(": ");
        self.print_type_ref(&p.ty);
    }
}

/// Build a synthetic `Expr` from a `NumberLit` so a numeric *pattern*
/// reuses the same suffix-aware printing as a numeric *expression*.
pub(super) fn number_lit_to_expr(n: &crate::lexer::NumberLit) -> Expr {
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
pub(super) fn fold_neg(e: &Expr) -> Option<Expr> {
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
