use miette::{NamedSource, SourceSpan};

use crate::ast::{Block, Document, Field, Item, Span, Value};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, Token, TokenKind};

pub struct Parser<'a> {
    src: &'a str,
    file: String,
    lexer: Lexer<'a>,
    peeked: Option<Token>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        Self {
            src,
            file: file.into(),
            lexer: Lexer::new(src),
            peeked: None,
        }
    }

    pub fn parse_document(&mut self) -> Result<Document, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Document { items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
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
            TokenKind::String(_) | TokenKind::LBrace => self.parse_block(name, span_start),
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

    fn parse_field(&mut self, name: String, start: usize) -> Result<Item, ParseError> {
        self.bump()?; // consume '='
        let val_tok = self.bump()?;
        let span = Span::new(start, val_tok.span.end);
        let value = match val_tok.kind {
            TokenKind::String(s) => Value::String(s),
            TokenKind::Int(n) => Value::Int(n),
            TokenKind::Float(f) => Value::Float(f),
            TokenKind::Bool(b) => Value::Bool(b),
            other => {
                return Err(self.err(
                    format!("expected value, found {}", describe(&other)),
                    val_tok.span,
                    "expected string, number, or boolean",
                ));
            }
        };
        Ok(Item::Field(Field { name, value, span }))
    }

    fn parse_block(&mut self, kind: String, start: usize) -> Result<Item, ParseError> {
        let mut labels = Vec::new();
        loop {
            let p = self.peek()?;
            match &p.kind {
                TokenKind::String(_) => {
                    if let TokenKind::String(s) = self.bump()?.kind {
                        labels.push(s);
                    }
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

    fn bump(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked.take() {
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
        TokenKind::String(_) => "string".to_string(),
        TokenKind::Int(_) => "integer".to_string(),
        TokenKind::Float(_) => "float".to_string(),
        TokenKind::Bool(_) => "boolean".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        Parser::new(src, "test").parse_document().expect("parse ok")
    }

    fn parse_err(src: &str) -> ParseError {
        Parser::new(src, "test")
            .parse_document()
            .expect_err("expected parse error")
    }

    #[test]
    fn parse_empty_document() {
        let doc = parse("");
        assert!(doc.items.is_empty());
    }

    #[test]
    fn parse_single_string_field() {
        let doc = parse(r#"name = "alpha""#);
        let f = doc.field("name").unwrap();
        assert_eq!(f.value, Value::String("alpha".into()));
    }

    #[test]
    fn parse_mixed_scalar_fields() {
        let doc = parse(
            r#"
            name = "alpha"
            count = 3
            ratio = 2.5
            enabled = true
            "#,
        );
        assert_eq!(
            doc.field("name").unwrap().value,
            Value::String("alpha".into())
        );
        assert_eq!(doc.field("count").unwrap().value, Value::Int(3));
        assert_eq!(doc.field("ratio").unwrap().value, Value::Float(2.5));
        assert_eq!(doc.field("enabled").unwrap().value, Value::Bool(true));
    }

    #[test]
    fn parse_block_with_label() {
        let doc = parse(
            r#"
            service "web" {
              port = 8080
              host = "0.0.0.0"
            }
            "#,
        );
        let block = doc.blocks().next().unwrap();
        assert_eq!(block.kind, "service");
        assert_eq!(block.labels, vec!["web".to_string()]);
        assert_eq!(block.field("port").unwrap().value, Value::Int(8080));
        assert_eq!(
            block.field("host").unwrap().value,
            Value::String("0.0.0.0".into())
        );
    }

    #[test]
    fn parse_block_without_label() {
        let doc = parse("metadata { region = \"us-east-1\" }");
        let block = doc.blocks().next().unwrap();
        assert_eq!(block.kind, "metadata");
        assert!(block.labels.is_empty());
    }

    #[test]
    fn parse_block_with_multiple_labels() {
        let doc = parse(r#"resource "aws_s3_bucket" "logs" { acl = "private" }"#);
        let block = doc.blocks().next().unwrap();
        assert_eq!(block.kind, "resource");
        assert_eq!(
            block.labels,
            vec!["aws_s3_bucket".to_string(), "logs".to_string()]
        );
    }

    #[test]
    fn parse_nested_blocks() {
        let doc = parse(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let outer = doc.blocks().next().unwrap();
        let inner = outer.blocks().next().unwrap();
        assert_eq!(inner.kind, "metadata");
        assert_eq!(
            inner.field("region").unwrap().value,
            Value::String("us-east-1".into())
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
        let doc = parse(src);
        let f = doc.field("name").unwrap();
        assert_eq!(&src[f.span.start..f.span.end], src);
    }
}
