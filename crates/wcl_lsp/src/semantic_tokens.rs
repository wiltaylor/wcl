use async_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};
use ropey::Rope;
use std::collections::HashSet;
use wcl_lang::lang::ast;
use wcl_lang::lang::lexer::{Token, TokenKind};

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,   // 0
    SemanticTokenType::VARIABLE,  // 1
    SemanticTokenType::PROPERTY,  // 2
    SemanticTokenType::FUNCTION,  // 3
    SemanticTokenType::TYPE,      // 4
    SemanticTokenType::STRING,    // 5
    SemanticTokenType::NUMBER,    // 6
    SemanticTokenType::COMMENT,   // 7
    SemanticTokenType::OPERATOR,  // 8
    SemanticTokenType::DECORATOR, // 9
    SemanticTokenType::NAMESPACE, // 10
    SemanticTokenType::PARAMETER, // 11
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,   // 0
    SemanticTokenModifier::DOCUMENTATION, // 1
];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Collect AST-level context to refine token classification.
struct AstContext {
    /// Byte offsets of identifiers used as block kinds
    block_kinds: HashSet<usize>,
    /// Byte offsets of identifiers used as attribute names
    attr_names: HashSet<usize>,
    /// Byte offsets of identifiers used as let binding names (declarations)
    let_names: HashSet<usize>,
    /// Byte offsets of identifiers used as macro names (declarations)
    macro_names: HashSet<usize>,
    /// Byte offsets of identifiers used as macro call names
    macro_call_names: HashSet<usize>,
    /// Byte offsets of identifiers used as decorator names
    decorator_names: HashSet<usize>,
    /// Byte offsets of identifiers used as type expressions
    type_spans: HashSet<usize>,
    /// Byte offsets of identifiers used as function call names
    fn_call_names: HashSet<usize>,
    /// Byte offsets of identifiers used as macro parameter names
    param_names: HashSet<usize>,
    /// Byte offsets of identifiers used as namespace names
    namespace_names: HashSet<usize>,
}

impl AstContext {
    fn new() -> Self {
        AstContext {
            block_kinds: HashSet::new(),
            attr_names: HashSet::new(),
            let_names: HashSet::new(),
            macro_names: HashSet::new(),
            macro_call_names: HashSet::new(),
            decorator_names: HashSet::new(),
            type_spans: HashSet::new(),
            fn_call_names: HashSet::new(),
            param_names: HashSet::new(),
            namespace_names: HashSet::new(),
        }
    }

    fn collect_from_doc(&mut self, doc: &ast::Document) {
        for item in &doc.items {
            match item {
                ast::DocItem::Body(body_item) => self.collect_body(body_item),
                ast::DocItem::ExportLet(el) => {
                    self.let_names.insert(el.name.span.start);
                    self.collect_expr(&el.value);
                }
                ast::DocItem::Namespace(ns) => {
                    for seg in &ns.path {
                        self.namespace_names.insert(seg.span.start);
                    }
                    for inner in &ns.items {
                        match inner {
                            ast::DocItem::Body(body_item) => self.collect_body(body_item),
                            ast::DocItem::Namespace(_) | ast::DocItem::Use(_) => {
                                // Nested namespaces/use — handled recursively by collect_from_doc
                            }
                            _ => {}
                        }
                    }
                }
                ast::DocItem::Use(use_decl) => {
                    for seg in &use_decl.namespace_path {
                        self.namespace_names.insert(seg.span.start);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_body(&mut self, item: &ast::BodyItem) {
        match item {
            ast::BodyItem::Attribute(attr) => {
                self.attr_names.insert(attr.name.span.start);
                self.collect_decorators(&attr.decorators);
                self.collect_expr(&attr.value);
            }
            ast::BodyItem::Block(block) => {
                self.block_kinds.insert(block.kind.span.start);
                self.collect_decorators(&block.decorators);
                for child in &block.body {
                    self.collect_body(child);
                }
            }
            ast::BodyItem::LetBinding(lb) => {
                self.let_names.insert(lb.name.span.start);
                self.collect_decorators(&lb.decorators);
                self.collect_expr(&lb.value);
            }
            ast::BodyItem::MacroDef(md) => {
                self.macro_names.insert(md.name.span.start);
                for param in &md.params {
                    self.param_names.insert(param.name.span.start);
                    if let Some(te) = &param.type_constraint {
                        self.collect_type_expr(te);
                    }
                }
            }
            ast::BodyItem::MacroCall(mc) => {
                self.macro_call_names.insert(mc.name.span.start);
            }
            ast::BodyItem::Schema(schema) => {
                for field in &schema.fields {
                    self.attr_names.insert(field.name.span.start);
                    self.collect_type_expr(&field.type_expr);
                    self.collect_decorators(&field.decorators_before);
                    self.collect_decorators(&field.decorators_after);
                }
                for variant in &schema.variants {
                    self.collect_decorators(&variant.decorators);
                    for field in &variant.fields {
                        self.attr_names.insert(field.name.span.start);
                        self.collect_type_expr(&field.type_expr);
                        self.collect_decorators(&field.decorators_before);
                        self.collect_decorators(&field.decorators_after);
                    }
                }
            }
            ast::BodyItem::Table(table) => {
                for col in &table.columns {
                    self.attr_names.insert(col.name.span.start);
                    self.collect_type_expr(&col.type_expr);
                }
            }
            ast::BodyItem::ForLoop(fl) => {
                self.let_names.insert(fl.iterator.span.start);
                self.collect_expr(&fl.iterable);
                for child in &fl.body {
                    self.collect_body(child);
                }
            }
            ast::BodyItem::Conditional(cond) => {
                self.collect_conditional(cond);
            }
            ast::BodyItem::Validation(val) => {
                self.collect_expr(&val.check);
                self.collect_expr(&val.message);
            }
            ast::BodyItem::DecoratorSchema(ds) => {
                for field in &ds.fields {
                    self.attr_names.insert(field.name.span.start);
                    self.collect_type_expr(&field.type_expr);
                }
            }
            ast::BodyItem::SymbolSetDecl(_) => {}
            ast::BodyItem::StructDef(_) => {}
        }
    }

    fn collect_conditional(&mut self, cond: &ast::Conditional) {
        self.collect_expr(&cond.condition);
        for child in &cond.then_body {
            self.collect_body(child);
        }
        if let Some(eb) = &cond.else_branch {
            match eb {
                ast::ElseBranch::ElseIf(inner) => self.collect_conditional(inner),
                ast::ElseBranch::Else(body, _, _) => {
                    for child in body {
                        self.collect_body(child);
                    }
                }
            }
        }
    }

    fn collect_decorators(&mut self, decorators: &[ast::Decorator]) {
        for dec in decorators {
            self.decorator_names.insert(dec.name.span.start);
        }
    }

    fn collect_type_expr(&mut self, te: &ast::TypeExpr) {
        self.type_spans.insert(te.span().start);
        match te {
            ast::TypeExpr::List(inner, _) | ast::TypeExpr::Set(inner, _) => {
                self.collect_type_expr(inner);
            }
            ast::TypeExpr::Map(k, v, _) => {
                self.collect_type_expr(k);
                self.collect_type_expr(v);
            }
            ast::TypeExpr::Union(types, _) => {
                for t in types {
                    self.collect_type_expr(t);
                }
            }
            _ => {}
        }
    }

    fn collect_expr(&mut self, expr: &ast::Expr) {
        match expr {
            ast::Expr::FnCall(callee, args, _) => {
                if let ast::Expr::Ident(ident) = callee.as_ref() {
                    self.fn_call_names.insert(ident.span.start);
                }
                self.collect_expr(callee);
                for arg in args {
                    match arg {
                        ast::CallArg::Positional(e) | ast::CallArg::Named(_, e) => {
                            self.collect_expr(e);
                        }
                    }
                }
            }
            ast::Expr::BinaryOp(l, _, r, _) => {
                self.collect_expr(l);
                self.collect_expr(r);
            }
            ast::Expr::UnaryOp(_, e, _) | ast::Expr::Paren(e, _) => self.collect_expr(e),
            ast::Expr::Ternary(a, b, c, _) => {
                self.collect_expr(a);
                self.collect_expr(b);
                self.collect_expr(c);
            }
            ast::Expr::MemberAccess(obj, _, _) => self.collect_expr(obj),
            ast::Expr::IndexAccess(obj, idx, _) => {
                self.collect_expr(obj);
                self.collect_expr(idx);
            }
            ast::Expr::List(items, _) => {
                for item in items {
                    self.collect_expr(item);
                }
            }
            ast::Expr::Map(entries, _) => {
                for (_, val) in entries {
                    self.collect_expr(val);
                }
            }
            ast::Expr::Lambda(_, body, _) => self.collect_expr(body),
            ast::Expr::BlockExpr(_, final_expr, _) => self.collect_expr(final_expr),
            _ => {}
        }
    }
}

/// Classify tokens into semantic token types, using AST context for refinement.
pub fn compute_semantic_tokens(
    tokens: &[Token],
    rope: &Rope,
    doc: Option<&ast::Document>,
) -> Vec<SemanticToken> {
    // Phase 1: Collect AST context
    let ctx = if let Some(doc) = doc {
        let mut ctx = AstContext::new();
        ctx.collect_from_doc(doc);
        Some(ctx)
    } else {
        None
    };

    // Phase 2: Classify tokens with context
    let mut result = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in tokens {
        let (token_type, modifiers) = match &token.kind {
            // Keywords
            TokenKind::Let
            | TokenKind::Partial
            | TokenKind::Macro
            | TokenKind::Schema
            | TokenKind::Table
            | TokenKind::Import
            | TokenKind::Export
            | TokenKind::Query
            | TokenKind::Ref
            | TokenKind::For
            | TokenKind::In
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::When
            | TokenKind::Inject
            | TokenKind::Set
            | TokenKind::Remove
            | TokenKind::SelfKw
            | TokenKind::Validation
            | TokenKind::DecoratorSchema
            | TokenKind::SymbolSet
            | TokenKind::Namespace
            | TokenKind::Use => (0, 0), // keyword

            // Literals
            TokenKind::StringLit(_) | TokenKind::StringFragment(_) | TokenKind::Heredoc { .. } => {
                (5, 0)
            } // string
            TokenKind::IntLit(_) | TokenKind::FloatLit(_) => (6, 0), // number
            TokenKind::BoolLit(_) | TokenKind::NullLit => (0, 0),    // keyword-ish
            TokenKind::SymbolLit(_) => (8, 0),                       // enumMember

            // Comments
            TokenKind::LineComment(_) | TokenKind::BlockComment(_) => (7, 0),
            TokenKind::DocComment(_) => (7, 0b10), // comment + documentation

            // Operators
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::EqEq
            | TokenKind::Neq
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::Lte
            | TokenKind::Gte
            | TokenKind::Match
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Not
            | TokenKind::FatArrow => (8, 0), // operator

            // Decorator sigil
            TokenKind::At => (9, 0), // decorator

            // Identifiers — use AST context for refinement
            TokenKind::Ident(_) | TokenKind::IdentifierLit(_) => {
                let start = token.span.start;
                if let Some(ctx) = &ctx {
                    if ctx.namespace_names.contains(&start) {
                        (10, 0) // namespace
                    } else if ctx.decorator_names.contains(&start) {
                        (9, 0) // decorator
                    } else if ctx.block_kinds.contains(&start) {
                        (4, 0) // type
                    } else if ctx.macro_names.contains(&start) {
                        (3, 0b01) // function + declaration
                    } else if ctx.macro_call_names.contains(&start)
                        || ctx.fn_call_names.contains(&start)
                    {
                        (3, 0) // function
                    } else if ctx.type_spans.contains(&start) {
                        (4, 0) // type
                    } else if ctx.param_names.contains(&start) {
                        (11, 0b01) // parameter + declaration
                    } else if ctx.let_names.contains(&start) {
                        (1, 0b01) // variable + declaration
                    } else if ctx.attr_names.contains(&start) {
                        (2, 0) // property
                    } else {
                        (1, 0) // variable (default)
                    }
                } else {
                    (1, 0) // variable (no AST context)
                }
            }

            // Skip delimiters, punctuation, whitespace
            _ => continue,
        };

        let span = token.span;
        let start_byte = span.start.min(rope.len_bytes());
        let line = rope.byte_to_line(start_byte) as u32;
        let line_start = rope.line_to_byte(line as usize);
        let line_slice = rope.line(line as usize);
        let byte_diff = start_byte - line_start;
        let mut utf16_col = 0u32;
        let mut bytes = 0usize;
        for ch in line_slice.chars() {
            if bytes >= byte_diff {
                break;
            }
            utf16_col += ch.len_utf16() as u32;
            bytes += ch.len_utf8();
        }

        let length = (span.end - span.start) as u32;

        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            utf16_col - prev_start
        } else {
            utf16_col
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: token_type as u32,
            token_modifiers_bitset: modifiers,
        });

        prev_line = line;
        prev_start = utf16_col;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::lang::span::FileId;

    fn classify(source: &str) -> Vec<(u32, u32)> {
        let file_id = FileId(0);
        let tokens = wcl_lang::lang::lexer::lex(source, file_id).unwrap();
        let (doc, _) = wcl_lang::lang::parse(source, file_id);
        let rope = Rope::from_str(source);
        let sem = compute_semantic_tokens(&tokens, &rope, Some(&doc));
        sem.iter()
            .map(|t| (t.token_type, t.token_modifiers_bitset))
            .collect()
    }

    #[test]
    fn test_keyword_classified() {
        let tokens = classify("let x = 42");
        // let=keyword, x=variable+decl, =skipped, 42=number
        assert!(tokens.iter().any(|(ty, _)| *ty == 0)); // keyword
        assert!(tokens.iter().any(|(ty, _)| *ty == 6)); // number
    }

    #[test]
    fn test_block_kind_is_type() {
        let tokens = classify("config { port = 8080 }");
        // config should be type(4), port should be property(2)
        assert_eq!(tokens[0].0, 4); // config = type
    }

    #[test]
    fn test_attribute_is_property() {
        let tokens = classify("config { port = 8080 }");
        // Find the property token
        assert!(tokens.iter().any(|(ty, _)| *ty == 2)); // property
    }

    #[test]
    fn test_let_name_has_declaration_modifier() {
        let tokens = classify("let x = 42");
        // x should be variable(1) + declaration(0b01)
        let var_decl = tokens.iter().find(|(ty, m)| *ty == 1 && *m == 0b01);
        assert!(var_decl.is_some());
    }

    #[test]
    fn test_string_classified() {
        let tokens = classify("name = \"hello\"");
        assert!(tokens.iter().any(|(ty, _)| *ty == 5)); // string
    }

    #[test]
    fn test_comment_classified() {
        let tokens = classify("// a comment\nname = 1");
        assert!(tokens.iter().any(|(ty, _)| *ty == 7)); // comment
    }
}
