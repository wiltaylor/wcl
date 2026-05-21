use miette::{NamedSource, SourceSpan};

use crate::ast::{
    Block, Expr, Field, Item, Source, Span, TypeDecl, TypeField, UnionDecl, UnionVariant,
    VariantBody,
};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, NumberLit, StringLit, Token, TokenKind};
use crate::value::{BuiltinType, TypeRef};

pub struct Parser<'a> {
    src: &'a str,
    file: String,
    lexer: Lexer<'a>,
    peeked: Option<Token>,
    peeked2: Option<Token>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        Self {
            src,
            file: file.into(),
            lexer: Lexer::new(src),
            peeked: None,
            peeked2: None,
        }
    }

    pub fn parse_source(&mut self) -> Result<Source, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Source { items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        // Two-token lookahead for `type IDENT { ... }` and `union IDENT { ... }`.
        let first_ident = match &self.peek()?.kind {
            TokenKind::Ident(s) => Some(s.clone()),
            _ => None,
        };
        if let Some(first) = first_ident
            && matches!(self.peek2()?.kind, TokenKind::Ident(_))
        {
            match first.as_str() {
                "type" => return self.parse_type_decl(),
                "union" => return self.parse_union_decl(),
                _ => {}
            }
        }

        let tok = self.bump()?;
        let span_start = tok.span.start;
        let name = match tok.kind {
            TokenKind::Ident(n) => n,
            other => {
                return Err(self.err(
                    format!("expected identifier, found {}", describe(&other)),
                    tok.span,
                    "expected identifier",
                ));
            }
        };
        let next = self.peek()?;
        match &next.kind {
            TokenKind::Eq => self.parse_field(name, span_start),
            TokenKind::Str(_) | TokenKind::LBrace => self.parse_block(name, span_start),
            other => {
                let msg = format!(
                    "expected '=', label, or '{{' after identifier '{}', found {}",
                    name,
                    describe(other)
                );
                let span = next.span;
                Err(self.err(msg, span, "unexpected token"))
            }
        }
    }

    fn parse_type_decl(&mut self) -> Result<Item, ParseError> {
        let type_kw = self.bump()?; // 'type'
        let start = type_kw.span.start;
        let name_tok = self.bump()?;
        let TokenKind::Ident(name) = name_tok.kind else {
            return Err(self.err(
                "expected type name after 'type'",
                name_tok.span,
                "expected identifier",
            ));
        };
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after type name '{name}', found {}",
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        let mut fields = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside type declaration",
                        span,
                        "expected '}'",
                    ));
                }
                _ => fields.push(self.parse_type_field()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::TypeDecl(TypeDecl {
            name,
            fields,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn parse_type_field(&mut self) -> Result<TypeField, ParseError> {
        let name_tok = self.bump()?;
        let field_start = name_tok.span.start;
        let TokenKind::Ident(field_name) = name_tok.kind else {
            return Err(self.err(
                format!("expected field name, found {}", describe(&name_tok.kind)),
                name_tok.span,
                "expected identifier",
            ));
        };
        let colon = self.bump()?;
        if !matches!(colon.kind, TokenKind::Colon) {
            return Err(self.err(
                format!(
                    "expected ':' after field name '{field_name}', found {}",
                    describe(&colon.kind)
                ),
                colon.span,
                "expected ':'",
            ));
        }
        let (ty, ty_span) = self.parse_type_ref()?;
        let mut optional = false;
        let mut end = ty_span.end;
        if matches!(self.peek()?.kind, TokenKind::Question) {
            let q = self.bump()?;
            optional = true;
            end = q.span.end;
        }
        Ok(TypeField {
            name: field_name,
            ty,
            ty_span,
            optional,
            span: Span::new(field_start, end),
        })
    }

    fn parse_union_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'union'
        let start = kw.span.start;
        let name_tok = self.bump()?;
        let TokenKind::Ident(name) = name_tok.kind else {
            return Err(self.err(
                "expected union name after 'union'",
                name_tok.span,
                "expected identifier",
            ));
        };
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after union name '{name}', found {}",
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        let mut variants = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside union declaration",
                        span,
                        "expected '}'",
                    ));
                }
                _ => variants.push(self.parse_variant_decl()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::UnionDecl(UnionDecl {
            name,
            variants,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn parse_variant_decl(&mut self) -> Result<UnionVariant, ParseError> {
        let name_tok = self.bump()?;
        let variant_start = name_tok.span.start;
        let TokenKind::Ident(variant_name) = name_tok.kind else {
            return Err(self.err(
                format!("expected variant name, found {}", describe(&name_tok.kind)),
                name_tok.span,
                "expected identifier",
            ));
        };
        let (body, body_end) = self.parse_variant_body()?;
        Ok(UnionVariant {
            name: variant_name,
            body,
            span: Span::new(variant_start, body_end),
        })
    }

    fn parse_variant_body(&mut self) -> Result<(VariantBody, usize), ParseError> {
        let head = self.peek()?;
        match head.kind {
            TokenKind::LBrace => {
                self.bump()?;
                let mut fields = Vec::new();
                loop {
                    let p = self.peek()?;
                    match p.kind {
                        TokenKind::RBrace => break,
                        TokenKind::Eof => {
                            let span = p.span;
                            return Err(self.err(
                                "unexpected end of file inside variant body",
                                span,
                                "expected '}'",
                            ));
                        }
                        _ => fields.push(self.parse_type_field()?),
                    }
                }
                let rbrace = self.bump()?;
                Ok((VariantBody::Record(fields), rbrace.span.end))
            }
            TokenKind::None => {
                let tok = self.bump()?;
                Ok((VariantBody::Unit, tok.span.end))
            }
            TokenKind::Amp | TokenKind::Ident(_) => {
                let (ty, ty_span) = self.parse_type_ref()?;
                // No optional `?` is permitted on a variant body type ref.
                if matches!(self.peek()?.kind, TokenKind::Question) {
                    let q = self.peek()?;
                    let span = q.span;
                    return Err(self.err(
                        "'?' is not allowed on a variant body",
                        span,
                        "remove '?'",
                    ));
                }
                Ok((VariantBody::TypeRef { ty, ty_span }, ty_span.end))
            }
            _ => {
                let span = head.span;
                let kind = describe(&head.kind);
                Err(self.err(
                    format!("expected variant body ('{{ ... }}', a type, or 'None'), found {kind}"),
                    span,
                    "expected variant body",
                ))
            }
        }
    }

    fn parse_type_ref(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let head = self.peek()?;
        if matches!(head.kind, TokenKind::Amp) {
            let amp = self.bump()?;
            let tok = self.bump()?;
            let TokenKind::Ident(name) = tok.kind else {
                return Err(self.err(
                    format!(
                        "expected type name after '&', found {}",
                        describe(&tok.kind)
                    ),
                    tok.span,
                    "expected identifier",
                ));
            };
            let inner = name_to_type_ref(&name);
            let span = Span::new(amp.span.start, tok.span.end);
            Ok((TypeRef::Reference(Box::new(inner)), span))
        } else if matches!(head.kind, TokenKind::Ident(_)) {
            let tok = self.bump()?;
            let TokenKind::Ident(name) = tok.kind else {
                unreachable!()
            };
            Ok((name_to_type_ref(&name), tok.span))
        } else {
            let span = head.span;
            let kind_desc = describe(&head.kind);
            Err(self.err(
                format!("expected type, found {kind_desc}"),
                span,
                "expected type",
            ))
        }
    }

    fn parse_field(&mut self, name: String, start: usize) -> Result<Item, ParseError> {
        self.bump()?; // consume '='
        let val_tok = self.bump()?;
        let span = Span::new(start, val_tok.span.end);
        let expr = match val_tok.kind {
            TokenKind::Number(n) => number_to_expr(n),
            TokenKind::Str(s) => string_to_expr(s),
            TokenKind::Bool(b) => Expr::Bool(b),
            TokenKind::Ident(s) => Expr::Reference(s),
            TokenKind::None => Expr::None,
            other => {
                return Err(self.err(
                    format!("expected value, found {}", describe(&other)),
                    val_tok.span,
                    "expected value",
                ));
            }
        };
        Ok(Item::Field(Field { name, expr, span }))
    }

    fn parse_block(&mut self, kind: String, start: usize) -> Result<Item, ParseError> {
        let mut labels = Vec::new();
        loop {
            let p = self.peek()?;
            match &p.kind {
                TokenKind::Str(StringLit::Utf8(_)) => {
                    let tok = self.bump()?;
                    if let TokenKind::Str(StringLit::Utf8(s)) = tok.kind {
                        labels.push(s);
                    }
                }
                TokenKind::Str(_) => {
                    let span = p.span;
                    return Err(self.err(
                        "block labels must be plain (utf8) strings",
                        span,
                        "expected an unprefixed string",
                    ));
                }
                TokenKind::LBrace => break,
                other => {
                    let msg = format!("expected label or '{{', found {}", describe(other));
                    let span = p.span;
                    return Err(self.err(msg, span, "expected label or '{'"));
                }
            }
        }
        self.bump()?; // consume '{'
        let mut items = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside block",
                        span,
                        "expected '}'",
                    ));
                }
                _ => items.push(self.parse_item()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::Block(Block {
            kind,
            labels,
            items,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn peek(&mut self) -> Result<&Token, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_lex()?);
        }
        Ok(self.peeked.as_ref().expect("just set"))
    }

    fn peek2(&mut self) -> Result<&Token, ParseError> {
        self.peek()?;
        if self.peeked2.is_none() {
            self.peeked2 = Some(self.next_lex()?);
        }
        Ok(self.peeked2.as_ref().expect("just set"))
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked.take() {
            self.peeked = self.peeked2.take();
            Ok(t)
        } else {
            self.next_lex()
        }
    }

    fn next_lex(&mut self) -> Result<Token, ParseError> {
        self.lexer.next_token().map_err(|e| self.lex_to_parse(e))
    }

    fn lex_to_parse(&self, e: LexError) -> ParseError {
        let label = e.message.clone();
        self.err(e.message, e.span, label)
    }

    fn err(&self, message: impl Into<String>, span: Span, label: impl Into<String>) -> ParseError {
        let len = span.len().max(1);
        ParseError::syntax(
            message.into(),
            NamedSource::new(self.file.clone(), self.src.to_string()),
            SourceSpan::new(span.start.into(), len),
            label.into(),
        )
    }
}

fn describe(t: &TokenKind) -> String {
    match t {
        TokenKind::Ident(s) => format!("identifier '{s}'"),
        TokenKind::Str(_) => "string".to_string(),
        TokenKind::Number(_) => "number".to_string(),
        TokenKind::Bool(_) => "boolean".to_string(),
        TokenKind::None => "'none'".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Question => "'?'".to_string(),
        TokenKind::Amp => "'&'".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
}

fn name_to_type_ref(name: &str) -> TypeRef {
    match BuiltinType::from_name(name) {
        Some(b) => TypeRef::Builtin(b),
        None => TypeRef::Named(name.to_string()),
    }
}

fn number_to_expr(n: NumberLit) -> Expr {
    match n {
        NumberLit::I8(v) => Expr::I8(v),
        NumberLit::I16(v) => Expr::I16(v),
        NumberLit::I32(v) => Expr::I32(v),
        NumberLit::I64(v) => Expr::I64(v),
        NumberLit::I128(v) => Expr::I128(v),
        NumberLit::Isize(v) => Expr::Isize(v),
        NumberLit::U8(v) => Expr::U8(v),
        NumberLit::U16(v) => Expr::U16(v),
        NumberLit::U32(v) => Expr::U32(v),
        NumberLit::U64(v) => Expr::U64(v),
        NumberLit::U128(v) => Expr::U128(v),
        NumberLit::Usize(v) => Expr::Usize(v),
        NumberLit::F32(v) => Expr::F32(v),
        NumberLit::F64(v) => Expr::F64(v),
    }
}

fn string_to_expr(s: StringLit) -> Expr {
    match s {
        StringLit::Utf8(s) => Expr::Utf8(s),
        StringLit::Ascii(s) => Expr::Ascii(s),
        StringLit::Utf16(v) => Expr::Utf16(v),
        StringLit::Utf32(v) => Expr::Utf32(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Source {
        Parser::new(src, "test").parse_source().expect("parse ok")
    }

    fn parse_err(src: &str) -> ParseError {
        Parser::new(src, "test")
            .parse_source()
            .expect_err("expected parse error")
    }

    fn field<'a>(items: &'a [Item], name: &str) -> &'a Field {
        items
            .iter()
            .find_map(|i| match i {
                Item::Field(f) if f.name == name => Some(f),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no field '{name}'"))
    }

    fn blocks(items: &[Item]) -> Vec<&Block> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_empty_document() {
        let s = parse("");
        assert!(s.items.is_empty());
    }

    #[test]
    fn parse_single_string_field() {
        let s = parse(r#"name = "alpha""#);
        assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
    }

    #[test]
    fn parse_mixed_scalar_fields() {
        let s = parse(
            r#"
            name = "alpha"
            count = 3
            ratio = 2.5
            enabled = true
            "#,
        );
        assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
        assert_eq!(field(&s.items, "count").expr, Expr::I64(3));
        assert_eq!(field(&s.items, "ratio").expr, Expr::F64(2.5));
        assert_eq!(field(&s.items, "enabled").expr, Expr::Bool(true));
    }

    #[test]
    fn parse_block_with_label() {
        let s = parse(
            r#"
            service "web" {
              port = 8080
              host = "0.0.0.0"
            }
            "#,
        );
        let blks = blocks(&s.items);
        let block = blks[0];
        assert_eq!(block.kind, "service");
        assert_eq!(block.labels, vec!["web".to_string()]);
        assert_eq!(field(&block.items, "port").expr, Expr::I64(8080));
        assert_eq!(
            field(&block.items, "host").expr,
            Expr::Utf8("0.0.0.0".into())
        );
    }

    #[test]
    fn parse_block_without_label() {
        let s = parse("metadata { region = \"us-east-1\" }");
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "metadata");
        assert!(block.labels.is_empty());
    }

    #[test]
    fn parse_block_with_multiple_labels() {
        let s = parse(r#"resource "aws_s3_bucket" "logs" { acl = "private" }"#);
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "resource");
        assert_eq!(
            block.labels,
            vec!["aws_s3_bucket".to_string(), "logs".to_string()]
        );
    }

    #[test]
    fn parse_nested_blocks() {
        let s = parse(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let outer = blocks(&s.items)[0];
        let inner = blocks(&outer.items)[0];
        assert_eq!(inner.kind, "metadata");
        assert_eq!(
            field(&inner.items, "region").expr,
            Expr::Utf8("us-east-1".into())
        );
    }

    #[test]
    fn error_on_missing_value() {
        let err = parse_err("name =");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("expected value")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn error_on_unclosed_block() {
        let err = parse_err("service \"web\" { port = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("end of file")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn error_when_first_token_is_not_ident() {
        let err = parse_err("= 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("expected identifier")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn spans_cover_full_field() {
        let src = r#"name = "alpha""#;
        let s = parse(src);
        let f = field(&s.items, "name");
        assert_eq!(&src[f.span.start..f.span.end], src);
    }

    fn type_decls(items: &[Item]) -> Vec<&TypeDecl> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::TypeDecl(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_simple_type_declaration() {
        let s = parse("type User { name: utf8 }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.name, "User");
        assert_eq!(t.fields.len(), 1);
        assert_eq!(t.fields[0].name, "name");
        assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Utf8));
        assert!(!t.fields[0].optional);
    }

    #[test]
    fn parse_type_with_optional_field() {
        let s = parse("type User { bio: utf8? age: u32? }");
        let t = type_decls(&s.items)[0];
        assert!(t.fields[0].optional);
        assert!(t.fields[1].optional);
        assert_eq!(t.fields[1].ty, TypeRef::Builtin(BuiltinType::U32));
    }

    #[test]
    fn parse_empty_type_body() {
        let s = parse("type Empty {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.name, "Empty");
        assert!(t.fields.is_empty());
    }

    #[test]
    fn parse_type_with_named_ref() {
        let s = parse("type Tree { parent: Tree? }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.fields[0].ty, TypeRef::Named("Tree".into()));
        assert!(t.fields[0].optional);
    }

    #[test]
    fn parse_reference_type_to_named() {
        let s = parse("type Post { author: &User? }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::Reference(Box::new(TypeRef::Named("User".into())))
        );
        assert!(t.fields[0].optional);
    }

    #[test]
    fn parse_reference_type_to_builtin() {
        let s = parse("type Score { value: &i32 }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::Reference(Box::new(TypeRef::Builtin(BuiltinType::I32)))
        );
        assert!(!t.fields[0].optional);
    }

    #[test]
    fn nested_reference_rejected() {
        let err = parse_err("type X { y: &&User }");
        match err {
            ParseError::Syntax(e) => {
                assert!(
                    e.message.contains("expected type name after '&'"),
                    "{}",
                    e.message
                )
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_bare_ident_as_reference_value() {
        let s = parse("owner = wil_taylor");
        assert_eq!(
            field(&s.items, "owner").expr,
            Expr::Reference("wil_taylor".into())
        );
    }

    #[test]
    fn parse_none_as_value() {
        let s = parse("maybe = none");
        assert_eq!(field(&s.items, "maybe").expr, Expr::None);
    }

    #[test]
    fn contextual_keyword_field_named_type_still_works() {
        // `type` followed by `=` is just a field named "type".
        let s = parse("type = 1");
        assert_eq!(field(&s.items, "type").expr, Expr::I64(1));
    }

    #[test]
    fn contextual_keyword_block_with_kind_type_still_works() {
        let s = parse(r#"type "label" { x = 1 }"#);
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "type");
        assert_eq!(block.labels, vec!["label".to_string()]);
    }

    #[test]
    fn type_decl_without_brace_errors() {
        let err = parse_err("type Foo = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'{'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    fn union_decls(items: &[Item]) -> Vec<&UnionDecl> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::UnionDecl(u) => Some(u),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_union_with_all_three_body_forms() {
        let s = parse(
            r#"
            type Point { x: f64 y: f64 }
            union Shape {
              Circle { center: Point radius: f64 }
              Polygon Point
              Empty none
            }
            "#,
        );
        let u = union_decls(&s.items)[0];
        assert_eq!(u.name, "Shape");
        assert_eq!(u.variants.len(), 3);
        assert_eq!(u.variants[0].name, "Circle");
        assert!(matches!(u.variants[0].body, VariantBody::Record(_)));
        assert_eq!(u.variants[1].name, "Polygon");
        match &u.variants[1].body {
            VariantBody::TypeRef { ty, .. } => {
                assert_eq!(*ty, TypeRef::Named("Point".into()))
            }
            _ => panic!("expected TypeRef body"),
        }
        assert_eq!(u.variants[2].name, "Empty");
        assert!(matches!(u.variants[2].body, VariantBody::Unit));
    }

    #[test]
    fn parse_empty_union() {
        let s = parse("union Nothing {}");
        let u = union_decls(&s.items)[0];
        assert_eq!(u.name, "Nothing");
        assert!(u.variants.is_empty());
    }

    #[test]
    fn parse_reference_variant_body() {
        let s = parse("type Item {} union Wrap { Boxed &Item }");
        let u = union_decls(&s.items)[0];
        match &u.variants[0].body {
            VariantBody::TypeRef { ty, .. } => assert_eq!(
                *ty,
                TypeRef::Reference(Box::new(TypeRef::Named("Item".into())))
            ),
            _ => panic!("expected TypeRef body"),
        }
    }

    #[test]
    fn variant_body_question_mark_rejected() {
        let err = parse_err("type T {} union X { V T? }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("'?' is not allowed on a variant body"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn union_decl_without_brace_errors() {
        let err = parse_err("union Foo = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'{'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn type_field_without_colon_errors() {
        let err = parse_err("type Foo { x utf8 }");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("':'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }
}
