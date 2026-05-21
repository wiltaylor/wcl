use miette::{NamedSource, SourceSpan};

use crate::ast::{Block, Expr, Field, Item, Source, Span, TypeDecl, TypeField};
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
        // Two-token lookahead for `type IDENT { ... }` declarations.
        if let TokenKind::Ident(first) = &self.peek()?.kind
            && first == "type"
            && matches!(self.peek2()?.kind, TokenKind::Ident(_))
        {
            return self.parse_type_decl();
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
        let ty_tok = self.bump()?;
        let ty_span = ty_tok.span;
        let TokenKind::Ident(ty_name) = ty_tok.kind else {
            return Err(self.err(
                format!("expected type name, found {}", describe(&ty_tok.kind)),
                ty_tok.span,
                "expected type",
            ));
        };
        let ty = match BuiltinType::from_name(&ty_name) {
            Some(b) => TypeRef::Builtin(b),
            None => TypeRef::Named(ty_name),
        };
        let mut optional = false;
        let mut end = ty_tok.span.end;
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

    fn parse_field(&mut self, name: String, start: usize) -> Result<Item, ParseError> {
        self.bump()?; // consume '='
        let val_tok = self.bump()?;
        let span = Span::new(start, val_tok.span.end);
        let expr = match val_tok.kind {
            TokenKind::Number(n) => number_to_expr(n),
            TokenKind::Str(s) => string_to_expr(s),
            TokenKind::Bool(b) => Expr::Bool(b),
            TokenKind::Ident(s) => Expr::Identifier(s),
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
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
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
    fn parse_type_with_identifier_builtin() {
        let s = parse("type Thing { id: identifier }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Identifier));
    }

    #[test]
    fn parse_bare_ident_as_identifier_value() {
        let s = parse("owner = wil_taylor");
        assert_eq!(
            field(&s.items, "owner").expr,
            Expr::Identifier("wil_taylor".into())
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

    #[test]
    fn type_field_without_colon_errors() {
        let err = parse_err("type Foo { x utf8 }");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("':'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }
}
