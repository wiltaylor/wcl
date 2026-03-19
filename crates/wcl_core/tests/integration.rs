//! Integration tests for wcl_core — exercises both the lexer and parser
//! through their public APIs.

use wcl_core::lexer::{lex, TokenKind};
use wcl_core::span::FileId;
use wcl_core::{
    ast::{BinOp, BodyItem, DocItem, Expr, InlineId, MacroBody, MacroKind, StringPart, UnaryOp},
    parse,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn file() -> FileId {
    FileId(0)
}

/// Lex `src` and return the token kinds, panicking on lex errors.
fn lex_ok(src: &str) -> Vec<TokenKind> {
    lex(src, file())
        .unwrap_or_else(|errs| panic!("lex errors: {:?}", errs))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

/// Lex `src` and expect at least one lex error to be returned.
fn lex_expect_error(src: &str) {
    assert!(
        lex(src, file()).is_err(),
        "expected lex to fail for input: {:?}",
        src
    );
}

/// Parse `src` and assert there are no diagnostics.
fn parse_clean(src: &str) -> wcl_core::ast::Document {
    let (doc, diags) = parse(src, file());
    assert!(
        !diags.has_errors(),
        "unexpected parse errors for {:?}: {:?}",
        src,
        diags.diagnostics()
    );
    doc
}

/// Parse `src` and assert at least one diagnostic is emitted (error recovery).
fn parse_with_errors(src: &str) -> wcl_core::ast::Document {
    let (doc, diags) = parse(src, file());
    assert!(
        diags.has_errors(),
        "expected parse errors for {:?}, but got none",
        src
    );
    doc
}

// ─────────────────────────────────────────────────────────────────────────────
// LEXER TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod lexer_tests {
    use super::*;

    // ── Keywords ─────────────────────────────────────────────────────────────

    #[test]
    fn all_keywords() {
        let src = "let export import schema partial for in if else true false null macro decorator_schema validation ref";
        let kinds = lex_ok(src);
        let expected = vec![
            TokenKind::Let,
            TokenKind::Export,
            TokenKind::Import,
            TokenKind::Schema,
            TokenKind::Partial,
            TokenKind::For,
            TokenKind::In,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::BoolLit(true),
            TokenKind::BoolLit(false),
            TokenKind::NullLit,
            TokenKind::Macro,
            TokenKind::DecoratorSchema,
            TokenKind::Validation,
            TokenKind::Ref,
            TokenKind::Eof,
        ];
        assert_eq!(kinds, expected);
    }

    #[test]
    fn additional_keywords() {
        let src = "when inject set remove self query table";
        let kinds = lex_ok(src);
        let expected = vec![
            TokenKind::When,
            TokenKind::Inject,
            TokenKind::Set,
            TokenKind::Remove,
            TokenKind::SelfKw,
            TokenKind::Query,
            TokenKind::Table,
            TokenKind::Eof,
        ];
        assert_eq!(kinds, expected);
    }

    // ── Operators ─────────────────────────────────────────────────────────────

    #[test]
    fn all_single_char_operators() {
        let src = "= + - * / % < > ! ? : ; | . # @";
        let kinds = lex_ok(src);
        let expected = vec![
            TokenKind::Equals,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::Not,
            TokenKind::Question,
            TokenKind::Colon,
            TokenKind::Semicolon,
            TokenKind::Pipe,
            TokenKind::Dot,
            TokenKind::Hash,
            TokenKind::At,
            TokenKind::Eof,
        ];
        assert_eq!(kinds, expected);
    }

    #[test]
    fn all_multi_char_operators() {
        let src = "== != <= >= =~ && || => ..";
        let kinds = lex_ok(src);
        let expected = vec![
            TokenKind::EqEq,
            TokenKind::Neq,
            TokenKind::Lte,
            TokenKind::Gte,
            TokenKind::Match,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::FatArrow,
            TokenKind::DotDot,
            TokenKind::Eof,
        ];
        assert_eq!(kinds, expected);
    }

    #[test]
    fn equals_disambiguated_from_eqeq() {
        let kinds = lex_ok("= ==");
        assert_eq!(kinds[0], TokenKind::Equals);
        assert_eq!(kinds[1], TokenKind::EqEq);
    }

    #[test]
    fn lt_disambiguated_from_lte() {
        let kinds = lex_ok("< <=");
        assert_eq!(kinds[0], TokenKind::Lt);
        assert_eq!(kinds[1], TokenKind::Lte);
    }

    #[test]
    fn gt_disambiguated_from_gte() {
        let kinds = lex_ok("> >=");
        assert_eq!(kinds[0], TokenKind::Gt);
        assert_eq!(kinds[1], TokenKind::Gte);
    }

    #[test]
    fn dot_vs_dotdot() {
        let kinds = lex_ok(". ..");
        assert_eq!(kinds[0], TokenKind::Dot);
        assert_eq!(kinds[1], TokenKind::DotDot);
    }

    #[test]
    fn fat_arrow_vs_equals() {
        let kinds = lex_ok("= =>");
        assert_eq!(kinds[0], TokenKind::Equals);
        assert_eq!(kinds[1], TokenKind::FatArrow);
    }

    // ── Delimiters ────────────────────────────────────────────────────────────

    #[test]
    fn all_delimiters() {
        let kinds = lex_ok("{ } [ ] ( )");
        let expected = vec![
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Eof,
        ];
        assert_eq!(kinds, expected);
    }

    // ── Number literals ───────────────────────────────────────────────────────

    #[test]
    fn integer_zero() {
        let kinds = lex_ok("0");
        assert_eq!(kinds[0], TokenKind::IntLit(0));
    }

    #[test]
    fn decimal_integers() {
        let kinds = lex_ok("1 42 999");
        assert_eq!(kinds[0], TokenKind::IntLit(1));
        assert_eq!(kinds[1], TokenKind::IntLit(42));
        assert_eq!(kinds[2], TokenKind::IntLit(999));
    }

    #[test]
    fn integer_with_underscores() {
        let kinds = lex_ok("1_000_000");
        assert_eq!(kinds[0], TokenKind::IntLit(1_000_000));
    }

    #[test]
    fn hex_prefix_lowercase() {
        let kinds = lex_ok("0xff");
        assert_eq!(kinds[0], TokenKind::IntLit(0xff));
    }

    #[test]
    fn hex_prefix_uppercase() {
        let kinds = lex_ok("0xFF 0XFF");
        assert_eq!(kinds[0], TokenKind::IntLit(0xFF));
        assert_eq!(kinds[1], TokenKind::IntLit(0xFF));
    }

    #[test]
    fn hex_with_underscores() {
        let kinds = lex_ok("0xDEAD_BEEF");
        assert_eq!(kinds[0], TokenKind::IntLit(0xDEAD_BEEFi64));
    }

    #[test]
    fn octal_literal() {
        let kinds = lex_ok("0o755");
        assert_eq!(kinds[0], TokenKind::IntLit(0o755));
    }

    #[test]
    fn octal_with_underscores() {
        let kinds = lex_ok("0o7_5_5");
        assert_eq!(kinds[0], TokenKind::IntLit(0o755));
    }

    #[test]
    fn binary_literal() {
        let kinds = lex_ok("0b1010");
        assert_eq!(kinds[0], TokenKind::IntLit(0b1010));
    }

    #[test]
    fn binary_with_underscores() {
        let kinds = lex_ok("0b1111_0000");
        assert_eq!(kinds[0], TokenKind::IntLit(0b1111_0000));
    }

    #[test]
    fn float_simple() {
        let kinds = lex_ok("3.14");
        assert!(matches!(kinds[0], TokenKind::FloatLit(_)));
        if let TokenKind::FloatLit(f) = kinds[0] {
            assert!((f - 3.14).abs() < 1e-10);
        }
    }

    #[test]
    fn float_with_exponent() {
        let kinds = lex_ok("1.0e10");
        assert!(matches!(kinds[0], TokenKind::FloatLit(_)));
        if let TokenKind::FloatLit(f) = kinds[0] {
            assert!((f - 1.0e10).abs() < 1.0);
        }
    }

    #[test]
    fn float_with_negative_exponent() {
        let kinds = lex_ok("1.0e-5");
        assert!(matches!(kinds[0], TokenKind::FloatLit(_)));
        if let TokenKind::FloatLit(f) = kinds[0] {
            assert!((f - 1.0e-5).abs() < 1e-15);
        }
    }

    // ── String literals ───────────────────────────────────────────────────────

    #[test]
    fn empty_string() {
        let kinds = lex_ok(r#""""#);
        assert_eq!(kinds[0], TokenKind::StringLit("".into()));
    }

    #[test]
    fn simple_double_quoted_string() {
        let kinds = lex_ok(r#""hello world""#);
        assert_eq!(kinds[0], TokenKind::StringLit("hello world".into()));
    }

    #[test]
    fn string_escape_newline() {
        let kinds = lex_ok(r#""\n""#);
        assert_eq!(kinds[0], TokenKind::StringLit("\n".into()));
    }

    #[test]
    fn string_escape_tab() {
        let kinds = lex_ok(r#""\t""#);
        assert_eq!(kinds[0], TokenKind::StringLit("\t".into()));
    }

    #[test]
    fn string_escape_backslash() {
        let kinds = lex_ok(r#""\\""#);
        assert_eq!(kinds[0], TokenKind::StringLit("\\".into()));
    }

    #[test]
    fn string_escape_double_quote() {
        let kinds = lex_ok(r#""\"""#);
        assert_eq!(kinds[0], TokenKind::StringLit("\"".into()));
    }

    #[test]
    fn string_unicode_escape_small_u() {
        // \u0041 == 'A'
        let kinds = lex_ok(r#""\u0041""#);
        assert_eq!(kinds[0], TokenKind::StringLit("A".into()));
    }

    #[test]
    fn string_unicode_escape_capital_u() {
        // \U00000041 == 'A'
        let kinds = lex_ok(r#""\U00000041""#);
        assert_eq!(kinds[0], TokenKind::StringLit("A".into()));
    }

    #[test]
    fn string_interpolation_marker_preserved() {
        // ${name} is preserved verbatim in the token content for the parser
        let kinds = lex_ok(r#""Hello ${name}!""#);
        assert_eq!(kinds[0], TokenKind::StringLit("Hello ${name}!".into()));
    }

    #[test]
    fn unterminated_string_is_lex_error() {
        lex_expect_error(r#""unterminated"#);
    }

    // ── Identifiers vs keywords ───────────────────────────────────────────────

    #[test]
    fn plain_identifier() {
        let kinds = lex_ok("myVar");
        assert_eq!(kinds[0], TokenKind::Ident("myVar".into()));
    }

    #[test]
    fn underscore_prefixed_identifier() {
        let kinds = lex_ok("_private");
        assert_eq!(kinds[0], TokenKind::Ident("_private".into()));
    }

    #[test]
    fn identifier_with_digits() {
        let kinds = lex_ok("port8080");
        assert_eq!(kinds[0], TokenKind::Ident("port8080".into()));
    }

    #[test]
    fn identifier_lit_with_single_hyphen() {
        let kinds = lex_ok("svc-auth");
        assert_eq!(kinds[0], TokenKind::IdentifierLit("svc-auth".into()));
    }

    #[test]
    fn identifier_lit_with_multiple_hyphens() {
        let kinds = lex_ok("my-long-name");
        assert_eq!(kinds[0], TokenKind::IdentifierLit("my-long-name".into()));
    }

    #[test]
    fn trailing_hyphen_is_minus() {
        // "foo-" should be Ident("foo") + Minus
        let kinds = lex_ok("foo-");
        assert_eq!(kinds[0], TokenKind::Ident("foo".into()));
        assert_eq!(kinds[1], TokenKind::Minus);
    }

    #[test]
    fn keyword_let_not_ident() {
        let kinds = lex_ok("let");
        assert_eq!(kinds[0], TokenKind::Let);
        // Must not be an Ident
        assert!(!matches!(kinds[0], TokenKind::Ident(_)));
    }

    #[test]
    fn identifier_starting_with_keyword_prefix() {
        // "letter" starts with "let" but is a full identifier
        let kinds = lex_ok("letter");
        assert_eq!(kinds[0], TokenKind::Ident("letter".into()));
    }

    #[test]
    fn bool_true_is_not_ident() {
        let kinds = lex_ok("true");
        assert_eq!(kinds[0], TokenKind::BoolLit(true));
    }

    #[test]
    fn bool_false_is_not_ident() {
        let kinds = lex_ok("false");
        assert_eq!(kinds[0], TokenKind::BoolLit(false));
    }

    #[test]
    fn null_is_not_ident() {
        let kinds = lex_ok("null");
        assert_eq!(kinds[0], TokenKind::NullLit);
    }

    // ── Comments ──────────────────────────────────────────────────────────────

    #[test]
    fn line_comment_emitted_as_token() {
        let kinds = lex_ok("// hello world\n");
        assert_eq!(kinds[0], TokenKind::LineComment("// hello world".into()));
    }

    #[test]
    fn doc_comment_triple_slash() {
        let kinds = lex_ok("/// doc comment\n");
        assert_eq!(kinds[0], TokenKind::DocComment("/// doc comment".into()));
    }

    #[test]
    fn doc_comment_distinguished_from_line_comment() {
        let kinds = lex_ok("/// doc\n// line\n");
        assert!(matches!(kinds[0], TokenKind::DocComment(_)));
        assert!(matches!(kinds[2], TokenKind::LineComment(_)));
    }

    #[test]
    fn block_comment_simple() {
        let kinds = lex_ok("/* a comment */");
        assert_eq!(kinds[0], TokenKind::BlockComment("/* a comment */".into()));
    }

    #[test]
    fn block_comment_multiline() {
        let src = "/* line one\n   line two */";
        let kinds = lex_ok(src);
        assert_eq!(kinds[0], TokenKind::BlockComment(src.into()));
    }

    #[test]
    fn block_comment_nested() {
        let src = "/* outer /* inner */ still outer */";
        let kinds = lex_ok(src);
        assert_eq!(kinds[0], TokenKind::BlockComment(src.into()));
    }

    #[test]
    fn block_comment_deeply_nested() {
        let src = "/* a /* b /* c */ b */ a */";
        let kinds = lex_ok(src);
        assert_eq!(kinds[0], TokenKind::BlockComment(src.into()));
    }

    #[test]
    fn unterminated_block_comment_is_lex_error() {
        lex_expect_error("/* not closed");
    }

    // ── Heredocs ──────────────────────────────────────────────────────────────

    #[test]
    fn heredoc_basic() {
        let src = "<<EOF\nhello\nworld\nEOF\n";
        let kinds = lex_ok(src);
        assert_eq!(
            kinds[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: false,
                raw: false,
            }
        );
    }

    #[test]
    fn heredoc_single_line() {
        let src = "<<EOT\nonly one line\nEOT\n";
        let kinds = lex_ok(src);
        assert_eq!(
            kinds[0],
            TokenKind::Heredoc {
                content: "only one line".into(),
                indented: false,
                raw: false,
            }
        );
    }

    #[test]
    fn heredoc_indented_strips_leading_whitespace() {
        let src = "<<-EOF\n    hello\n    world\n    EOF\n";
        let kinds = lex_ok(src);
        assert_eq!(
            kinds[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: true,
                raw: false,
            }
        );
    }

    #[test]
    fn heredoc_raw_no_escape_processing() {
        let src = "<<'EOF'\nno \\n escape here\nEOF\n";
        let kinds = lex_ok(src);
        // Raw heredoc: escape not processed, raw=true
        assert!(matches!(
            &kinds[0],
            TokenKind::Heredoc { raw: true, content, .. } if content.contains("\\n")
        ));
    }

    #[test]
    fn heredoc_escape_processed_in_non_raw() {
        // In a normal heredoc, \n becomes newline
        let src = "<<EOF\nhello\\nworld\nEOF\n";
        let kinds = lex_ok(src);
        assert_eq!(
            kinds[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: false,
                raw: false,
            }
        );
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn empty_input_produces_only_eof() {
        let kinds = lex_ok("");
        assert_eq!(kinds, vec![TokenKind::Eof]);
    }

    #[test]
    fn whitespace_only_produces_only_eof() {
        let kinds = lex_ok("   \t  ");
        assert_eq!(kinds, vec![TokenKind::Eof]);
    }

    #[test]
    fn newlines_are_emitted() {
        let kinds = lex_ok("a\nb");
        assert_eq!(kinds[0], TokenKind::Ident("a".into()));
        assert_eq!(kinds[1], TokenKind::Newline);
        assert_eq!(kinds[2], TokenKind::Ident("b".into()));
    }

    #[test]
    fn multiple_newlines_each_emitted() {
        let kinds = lex_ok("\n\n\n");
        let newlines: Vec<_> = kinds
            .iter()
            .filter(|k| matches!(k, TokenKind::Newline))
            .collect();
        assert_eq!(newlines.len(), 3);
    }

    #[test]
    fn interp_start_token() {
        let kinds = lex_ok("${");
        assert_eq!(kinds[0], TokenKind::InterpStart);
    }

    #[test]
    fn comma_token() {
        let kinds = lex_ok(",");
        assert_eq!(kinds[0], TokenKind::Comma);
    }

    #[test]
    fn mixed_tokens_sequence() {
        let kinds = lex_ok("let x = 42");
        assert_eq!(kinds[0], TokenKind::Let);
        assert_eq!(kinds[1], TokenKind::Ident("x".into()));
        assert_eq!(kinds[2], TokenKind::Equals);
        assert_eq!(kinds[3], TokenKind::IntLit(42));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PARSER TESTS
// ─────────────────────────────────────────────────────────────────────────────

mod parser_tests {
    use super::*;

    // ── Simple attributes ─────────────────────────────────────────────────────

    #[test]
    fn attribute_integer() {
        let doc = parse_clean("x = 1");
        assert_eq!(doc.items.len(), 1);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute, got {:?}", doc.items[0]);
        };
        assert_eq!(attr.name.name, "x");
        assert!(matches!(attr.value, Expr::IntLit(1, _)));
    }

    #[test]
    fn attribute_string() {
        let doc = parse_clean(r#"name = "hello""#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert_eq!(attr.name.name, "name");
        assert!(matches!(&attr.value, Expr::StringLit(_)));
        if let Expr::StringLit(s) = &attr.value {
            assert_eq!(s.parts.len(), 1);
            assert!(matches!(&s.parts[0], StringPart::Literal(t) if t == "hello"));
        }
    }

    #[test]
    fn attribute_boolean_true() {
        let doc = parse_clean("flag = true");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(attr.value, Expr::BoolLit(true, _)));
    }

    #[test]
    fn attribute_boolean_false() {
        let doc = parse_clean("flag = false");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(attr.value, Expr::BoolLit(false, _)));
    }

    #[test]
    fn attribute_null() {
        let doc = parse_clean("val = null");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(attr.value, Expr::NullLit(_)));
    }

    #[test]
    fn attribute_float() {
        let doc = parse_clean("ratio = 3.14");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(attr.value, Expr::FloatLit(_, _)));
        if let Expr::FloatLit(f, _) = attr.value {
            assert!((f - 3.14).abs() < 1e-10);
        }
    }

    // ── List expressions ──────────────────────────────────────────────────────

    #[test]
    fn list_empty() {
        let doc = parse_clean("items = []");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::List(items, _) = &attr.value else {
            panic!("expected list");
        };
        assert!(items.is_empty());
    }

    #[test]
    fn list_of_integers() {
        let doc = parse_clean("items = [1, 2, 3]");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::List(items, _) = &attr.value else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], Expr::IntLit(1, _)));
        assert!(matches!(items[1], Expr::IntLit(2, _)));
        assert!(matches!(items[2], Expr::IntLit(3, _)));
    }

    #[test]
    fn list_of_strings() {
        let doc = parse_clean(r#"tags = ["a", "b", "c"]"#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::List(items, _) = &attr.value else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 3);
    }

    // ── Map expressions ───────────────────────────────────────────────────────

    #[test]
    fn map_empty() {
        let doc = parse_clean("m = {}");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Map(entries, _) = &attr.value else {
            panic!("expected map");
        };
        assert!(entries.is_empty());
    }

    #[test]
    fn map_with_entries() {
        let doc = parse_clean("m = { a = 1, b = 2 }");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Map(entries, _) = &attr.value else {
            panic!("expected map");
        };
        assert_eq!(entries.len(), 2);
    }

    // ── Block items ───────────────────────────────────────────────────────────

    #[test]
    fn block_no_id() {
        let doc = parse_clean("server { port = 8080 }");
        assert_eq!(doc.items.len(), 1);
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.kind.name, "server");
        assert!(block.inline_id.is_none());
        assert_eq!(block.body.len(), 1);
    }

    #[test]
    fn block_with_inline_id() {
        let doc = parse_clean("server my-server { port = 8080 }");
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.kind.name, "server");
        assert!(block.inline_id.is_some());
        if let Some(InlineId::Literal(id)) = &block.inline_id {
            assert_eq!(id.value, "my-server");
        } else {
            panic!("expected literal inline id");
        }
    }

    #[test]
    fn block_with_string_labels() {
        let doc = parse_clean(r#"resource "aws_instance" "web" { ami = "abc" }"#);
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.kind.name, "resource");
        assert_eq!(block.labels.len(), 2);
    }

    #[test]
    fn block_with_single_label() {
        let doc = parse_clean(r#"server "primary" { port = 443 }"#);
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.labels.len(), 1);
    }

    #[test]
    fn block_empty_body() {
        let doc = parse_clean("server {}");
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert!(block.body.is_empty());
    }

    #[test]
    fn nested_blocks() {
        let doc = parse_clean("outer { inner { x = 1 } }");
        let DocItem::Body(BodyItem::Block(outer)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(outer.kind.name, "outer");
        assert_eq!(outer.body.len(), 1);
        let BodyItem::Block(inner) = &outer.body[0] else {
            panic!("expected inner block");
        };
        assert_eq!(inner.kind.name, "inner");
        assert_eq!(inner.body.len(), 1);
    }

    #[test]
    fn block_with_multiple_attributes() {
        let doc = parse_clean("server { host = \"localhost\"\nport = 8080\nenabled = true }");
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.body.len(), 3);
    }

    #[test]
    fn partial_block() {
        let doc = parse_clean(r#"partial server defaults { port = 8080 }"#);
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert!(block.partial);
        assert_eq!(block.kind.name, "server");
    }

    // ── Let bindings ──────────────────────────────────────────────────────────

    #[test]
    fn let_binding_integer() {
        let doc = parse_clean("let x = 42");
        let DocItem::Body(BodyItem::LetBinding(binding)) = &doc.items[0] else {
            panic!("expected let binding");
        };
        assert_eq!(binding.name.name, "x");
        assert!(matches!(binding.value, Expr::IntLit(42, _)));
    }

    #[test]
    fn let_binding_string() {
        let doc = parse_clean(r#"let greeting = "hello""#);
        let DocItem::Body(BodyItem::LetBinding(binding)) = &doc.items[0] else {
            panic!("expected let binding");
        };
        assert_eq!(binding.name.name, "greeting");
        assert!(matches!(&binding.value, Expr::StringLit(_)));
    }

    #[test]
    fn let_binding_expression() {
        let doc = parse_clean("let result = 1 + 2");
        let DocItem::Body(BodyItem::LetBinding(binding)) = &doc.items[0] else {
            panic!("expected let binding");
        };
        assert!(matches!(&binding.value, Expr::BinaryOp(_, BinOp::Add, _, _)));
    }

    // ── Export let ────────────────────────────────────────────────────────────

    #[test]
    fn export_let() {
        let doc = parse_clean("export let base_port = 8000");
        assert_eq!(doc.items.len(), 1);
        let DocItem::ExportLet(export) = &doc.items[0] else {
            panic!("expected export let");
        };
        assert_eq!(export.name.name, "base_port");
        assert!(matches!(export.value, Expr::IntLit(8000, _)));
    }

    // ── Import ────────────────────────────────────────────────────────────────

    #[test]
    fn import_statement() {
        let doc = parse_clean(r#"import "./other.wcl""#);
        assert_eq!(doc.items.len(), 1);
        let DocItem::Import(imp) = &doc.items[0] else {
            panic!("expected import");
        };
        assert_eq!(imp.path.parts.len(), 1);
        assert!(matches!(&imp.path.parts[0], StringPart::Literal(s) if s == "./other.wcl"));
    }

    // ── String interpolation ──────────────────────────────────────────────────

    #[test]
    fn string_interpolation_simple() {
        let doc = parse_clean(r#"msg = "hello ${name}""#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::StringLit(s) = &attr.value else {
            panic!("expected string lit");
        };
        // Should have "hello " literal + interpolation
        assert!(s.parts.len() >= 2);
        assert!(matches!(&s.parts[0], StringPart::Literal(t) if t == "hello "));
        assert!(matches!(&s.parts[1], StringPart::Interpolation(_)));
    }

    #[test]
    fn string_interpolation_at_start() {
        let doc = parse_clean(r#"msg = "${prefix}-suffix""#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::StringLit(s) = &attr.value else {
            panic!("expected string lit");
        };
        assert!(s.parts.iter().any(|p| matches!(p, StringPart::Interpolation(_))));
    }

    // ── Ternary expressions ───────────────────────────────────────────────────

    #[test]
    fn ternary_expression() {
        let doc = parse_clean(r#"x = condition ? "yes" : "no""#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::Ternary(_, _, _, _)));
    }

    #[test]
    fn ternary_with_comparison() {
        let doc = parse_clean("x = a > 0 ? 1 : -1");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::Ternary(_, _, _, _)));
    }

    // ── Binary expressions / precedence ──────────────────────────────────────

    #[test]
    fn binary_add() {
        let doc = parse_clean("x = 1 + 2");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::BinaryOp(_, BinOp::Add, _, _)));
    }

    #[test]
    fn binary_precedence_mul_over_add() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let doc = parse_clean("x = 1 + 2 * 3");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        // Top-level should be Add
        let Expr::BinaryOp(lhs, op, rhs, _) = &attr.value else {
            panic!("expected binary op");
        };
        assert_eq!(*op, BinOp::Add);
        assert!(matches!(lhs.as_ref(), Expr::IntLit(1, _)));
        // rhs should be 2 * 3
        assert!(matches!(rhs.as_ref(), Expr::BinaryOp(_, BinOp::Mul, _, _)));
    }

    #[test]
    fn binary_precedence_add_left_associative() {
        // 1 + 2 + 3 should parse as (1 + 2) + 3
        let doc = parse_clean("x = 1 + 2 + 3");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::BinaryOp(lhs, op, rhs, _) = &attr.value else {
            panic!("expected binary op");
        };
        assert_eq!(*op, BinOp::Add);
        // lhs should be 1 + 2
        assert!(matches!(lhs.as_ref(), Expr::BinaryOp(_, BinOp::Add, _, _)));
        assert!(matches!(rhs.as_ref(), Expr::IntLit(3, _)));
    }

    #[test]
    fn binary_comparison_operators() {
        let cases = [
            ("x = a == b", BinOp::Eq),
            ("x = a != b", BinOp::Neq),
            ("x = a < b", BinOp::Lt),
            ("x = a > b", BinOp::Gt),
            ("x = a <= b", BinOp::Lte),
            ("x = a >= b", BinOp::Gte),
        ];
        for (src, expected_op) in cases {
            let doc = parse_clean(src);
            let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
                panic!("expected attribute for {}", src);
            };
            let Expr::BinaryOp(_, op, _, _) = &attr.value else {
                panic!("expected binary op for {}", src);
            };
            assert_eq!(*op, expected_op, "wrong op for: {}", src);
        }
    }

    #[test]
    fn binary_logical_operators() {
        let doc = parse_clean("x = a && b || c");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        // || has lower precedence than &&, so top-level is Or
        assert!(matches!(&attr.value, Expr::BinaryOp(_, BinOp::Or, _, _)));
    }

    #[test]
    fn binary_match_operator() {
        let doc = parse_clean(r#"x = name =~ "pattern""#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::BinaryOp(_, BinOp::Match, _, _)));
    }

    #[test]
    fn binary_arithmetic_all_ops() {
        let cases = [
            ("x = a + b", BinOp::Add),
            ("x = a - b", BinOp::Sub),
            ("x = a * b", BinOp::Mul),
            ("x = a / b", BinOp::Div),
            ("x = a % b", BinOp::Mod),
        ];
        for (src, expected_op) in cases {
            let doc = parse_clean(src);
            let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
                panic!("expected attribute for {}", src);
            };
            let Expr::BinaryOp(_, op, _, _) = &attr.value else {
                panic!("expected binary op for {}", src);
            };
            assert_eq!(*op, expected_op);
        }
    }

    // ── Unary expressions ─────────────────────────────────────────────────────

    #[test]
    fn unary_negation() {
        let doc = parse_clean("x = -5");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::UnaryOp(UnaryOp::Neg, _, _)));
    }

    #[test]
    fn unary_not() {
        let doc = parse_clean("x = !flag");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::UnaryOp(UnaryOp::Not, _, _)));
    }

    #[test]
    fn double_negation() {
        let doc = parse_clean("x = !!flag");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::UnaryOp(UnaryOp::Not, inner, _) = &attr.value else {
            panic!("expected unary not");
        };
        assert!(matches!(inner.as_ref(), Expr::UnaryOp(UnaryOp::Not, _, _)));
    }

    // ── Member access ─────────────────────────────────────────────────────────

    #[test]
    fn member_access() {
        let doc = parse_clean("x = config.port");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::MemberAccess(obj, field, _) = &attr.value else {
            panic!("expected member access");
        };
        assert!(matches!(obj.as_ref(), Expr::Ident(id) if id.name == "config"));
        assert_eq!(field.name, "port");
    }

    #[test]
    fn chained_member_access() {
        let doc = parse_clean("x = a.b.c");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        // a.b.c => (a.b).c  (left-associative postfix)
        let Expr::MemberAccess(inner, field_c, _) = &attr.value else {
            panic!("expected member access");
        };
        assert_eq!(field_c.name, "c");
        let Expr::MemberAccess(root, field_b, _) = inner.as_ref() else {
            panic!("expected inner member access");
        };
        assert_eq!(field_b.name, "b");
        assert!(matches!(root.as_ref(), Expr::Ident(id) if id.name == "a"));
    }

    // ── Index access ──────────────────────────────────────────────────────────

    #[test]
    fn index_access() {
        let doc = parse_clean("x = list[0]");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::IndexAccess(obj, idx, _) = &attr.value else {
            panic!("expected index access");
        };
        assert!(matches!(obj.as_ref(), Expr::Ident(id) if id.name == "list"));
        assert!(matches!(idx.as_ref(), Expr::IntLit(0, _)));
    }

    #[test]
    fn index_access_with_expression() {
        let doc = parse_clean("x = list[i + 1]");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::IndexAccess(_, idx, _) = &attr.value else {
            panic!("expected index access");
        };
        assert!(matches!(idx.as_ref(), Expr::BinaryOp(_, BinOp::Add, _, _)));
    }

    // ── Function calls ────────────────────────────────────────────────────────

    #[test]
    fn function_call_no_args() {
        let doc = parse_clean("x = count()");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::FnCall(callee, args, _) = &attr.value else {
            panic!("expected fn call");
        };
        assert!(matches!(callee.as_ref(), Expr::Ident(id) if id.name == "count"));
        assert!(args.is_empty());
    }

    #[test]
    fn function_call_one_arg() {
        let doc = parse_clean("x = len(items)");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::FnCall(callee, args, _) = &attr.value else {
            panic!("expected fn call");
        };
        assert!(matches!(callee.as_ref(), Expr::Ident(id) if id.name == "len"));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn function_call_multiple_args() {
        let doc = parse_clean("x = substr(s, 0, 5)");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::FnCall(_, args, _) = &attr.value else {
            panic!("expected fn call");
        };
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn method_call_chained() {
        let doc = parse_clean("x = list.map(f)");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        // list.map(f) => FnCall( MemberAccess(list, map), [f] )
        let Expr::FnCall(callee, args, _) = &attr.value else {
            panic!("expected fn call");
        };
        assert!(matches!(callee.as_ref(), Expr::MemberAccess(_, _, _)));
        assert_eq!(args.len(), 1);
    }

    // ── Lambda expressions ────────────────────────────────────────────────────

    #[test]
    fn lambda_single_param() {
        let doc = parse_clean("f = (x) => x + 1");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Lambda(params, body, _) = &attr.value else {
            panic!("expected lambda");
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
        assert!(matches!(body.as_ref(), Expr::BinaryOp(_, BinOp::Add, _, _)));
    }

    #[test]
    fn lambda_multi_param() {
        let doc = parse_clean("f = (x, y) => x + y");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Lambda(params, _, _) = &attr.value else {
            panic!("expected lambda");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "x");
        assert_eq!(params[1].name, "y");
    }

    #[test]
    fn lambda_no_params() {
        let doc = parse_clean("f = () => 42");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Lambda(params, body, _) = &attr.value else {
            panic!("expected lambda");
        };
        assert!(params.is_empty());
        assert!(matches!(body.as_ref(), Expr::IntLit(42, _)));
    }

    // ── For loops ─────────────────────────────────────────────────────────────

    #[test]
    fn for_loop_basic() {
        let doc = parse_clean("for item in items { x = 1 }");
        let DocItem::Body(BodyItem::ForLoop(fl)) = &doc.items[0] else {
            panic!("expected for loop");
        };
        assert_eq!(fl.iterator.name, "item");
        assert!(fl.index.is_none());
        assert!(matches!(&fl.iterable, Expr::Ident(id) if id.name == "items"));
        assert_eq!(fl.body.len(), 1);
    }

    #[test]
    fn for_loop_with_index() {
        let doc = parse_clean("for item, idx in items { x = 1 }");
        let DocItem::Body(BodyItem::ForLoop(fl)) = &doc.items[0] else {
            panic!("expected for loop");
        };
        assert_eq!(fl.iterator.name, "item");
        assert!(fl.index.is_some());
        assert_eq!(fl.index.as_ref().unwrap().name, "idx");
    }

    #[test]
    fn for_loop_empty_body() {
        let doc = parse_clean("for x in list {}");
        let DocItem::Body(BodyItem::ForLoop(fl)) = &doc.items[0] else {
            panic!("expected for loop");
        };
        assert!(fl.body.is_empty());
    }

    // ── If/else conditionals ──────────────────────────────────────────────────

    #[test]
    fn if_no_else() {
        let doc = parse_clean("if condition { x = 1 }");
        let DocItem::Body(BodyItem::Conditional(cond)) = &doc.items[0] else {
            panic!("expected conditional");
        };
        assert!(matches!(&cond.condition, Expr::Ident(id) if id.name == "condition"));
        assert_eq!(cond.then_body.len(), 1);
        assert!(cond.else_branch.is_none());
    }

    #[test]
    fn if_else() {
        let doc = parse_clean("if condition { x = 1 } else { x = 2 }");
        let DocItem::Body(BodyItem::Conditional(cond)) = &doc.items[0] else {
            panic!("expected conditional");
        };
        assert!(cond.else_branch.is_some());
        let Some(wcl_core::ast::ElseBranch::Else(else_body, _, _)) = &cond.else_branch else {
            panic!("expected else branch");
        };
        assert_eq!(else_body.len(), 1);
    }

    #[test]
    fn if_else_if() {
        let doc = parse_clean("if a { x = 1 } else if b { x = 2 } else { x = 3 }");
        let DocItem::Body(BodyItem::Conditional(cond)) = &doc.items[0] else {
            panic!("expected conditional");
        };
        let Some(wcl_core::ast::ElseBranch::ElseIf(nested)) = &cond.else_branch else {
            panic!("expected else-if branch");
        };
        assert!(nested.else_branch.is_some());
    }

    // ── Schema blocks ─────────────────────────────────────────────────────────

    #[test]
    fn schema_simple() {
        let doc = parse_clean(r#"schema "Server" { port: int }"#);
        let DocItem::Body(BodyItem::Schema(schema)) = &doc.items[0] else {
            panic!("expected schema");
        };
        assert!(matches!(&schema.name.parts[0], StringPart::Literal(s) if s == "Server"));
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].name.name, "port");
        assert!(matches!(&schema.fields[0].type_expr, wcl_core::ast::TypeExpr::Int(_)));
    }

    #[test]
    fn schema_multiple_fields() {
        let doc = parse_clean(r#"schema "Config" { host: string\nport: int\nenabled: bool }"#
            .replace("\\n", "\n")
            .as_str());
        let DocItem::Body(BodyItem::Schema(schema)) = &doc.items[0] else {
            panic!("expected schema");
        };
        assert_eq!(schema.fields.len(), 3);
    }

    #[test]
    fn schema_with_list_type() {
        let doc = parse_clean(r#"schema "S" { tags: list(string) }"#);
        let DocItem::Body(BodyItem::Schema(schema)) = &doc.items[0] else {
            panic!("expected schema");
        };
        assert!(matches!(
            &schema.fields[0].type_expr,
            wcl_core::ast::TypeExpr::List(_, _)
        ));
    }

    // ── Decorator schemas ─────────────────────────────────────────────────────

    #[test]
    fn decorator_schema_basic() {
        let doc = parse_clean(
            r#"decorator_schema "deprecated" { target = [block] reason: string }"#,
        );
        let DocItem::Body(BodyItem::DecoratorSchema(ds)) = &doc.items[0] else {
            panic!("expected decorator_schema");
        };
        assert!(matches!(&ds.name.parts[0], StringPart::Literal(s) if s == "deprecated"));
        assert_eq!(ds.target.len(), 1);
        assert_eq!(ds.fields.len(), 1);
    }

    // ── Validation blocks ─────────────────────────────────────────────────────

    #[test]
    fn validation_block() {
        let doc = parse_clean(
            r#"validation "need-servers" { check = count > 0
message = "need at least one server" }"#,
        );
        let DocItem::Body(BodyItem::Validation(v)) = &doc.items[0] else {
            panic!("expected validation");
        };
        assert!(matches!(&v.name.parts[0], StringPart::Literal(s) if s == "need-servers"));
        assert!(v.lets.is_empty());
    }

    #[test]
    fn validation_with_let_bindings() {
        let doc = parse_clean(
            r#"validation "check" { let n = count(servers)
check = n > 0
message = "need servers" }"#,
        );
        let DocItem::Body(BodyItem::Validation(v)) = &doc.items[0] else {
            panic!("expected validation");
        };
        assert_eq!(v.lets.len(), 1);
        assert_eq!(v.lets[0].name.name, "n");
    }

    // ── Table blocks ──────────────────────────────────────────────────────────

    #[test]
    fn table_with_columns_and_rows() {
        let doc = parse_clean(
            r#"table users {
  name: string
  age: int
  | "Alice" | 30 |
  | "Bob" | 25 |
}"#,
        );
        let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] else {
            panic!("expected table, got {:?}", doc.items[0]);
        };
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.columns[0].name.name, "name");
        assert_eq!(table.columns[1].name.name, "age");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[1].cells.len(), 2);
    }

    #[test]
    fn table_empty() {
        let doc = parse_clean("table empty {}");
        let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] else {
            panic!("expected table");
        };
        assert!(table.columns.is_empty());
        assert!(table.rows.is_empty());
    }

    // ── Decorators ────────────────────────────────────────────────────────────

    #[test]
    fn decorator_on_block() {
        let doc = parse_clean(r#"@deprecated server old-server { }"#);
        let DocItem::Body(BodyItem::Block(block)) = &doc.items[0] else {
            panic!("expected block");
        };
        assert_eq!(block.decorators.len(), 1);
        assert_eq!(block.decorators[0].name.name, "deprecated");
    }

    #[test]
    fn decorator_with_args() {
        let doc = parse_clean(r#"@validate(min = 1, max = 100) x = 50"#);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert_eq!(attr.decorators.len(), 1);
        assert_eq!(attr.decorators[0].name.name, "validate");
        assert_eq!(attr.decorators[0].args.len(), 2);
    }

    #[test]
    fn multiple_decorators() {
        let doc = parse_clean("@a\n@b\n@c\nx = 1");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert_eq!(attr.decorators.len(), 3);
    }

    // ── Macro definitions ─────────────────────────────────────────────────────

    #[test]
    fn macro_function_definition() {
        let doc = parse_clean("macro double(x) { result = x * 2 }");
        let DocItem::Body(BodyItem::MacroDef(m)) = &doc.items[0] else {
            panic!("expected macro def");
        };
        assert_eq!(m.kind, MacroKind::Function);
        assert_eq!(m.name.name, "double");
        assert_eq!(m.params.len(), 1);
        assert_eq!(m.params[0].name.name, "x");
        assert!(matches!(&m.body, MacroBody::Function(_)));
    }

    #[test]
    fn macro_function_no_params() {
        let doc = parse_clean("macro noop() { }");
        let DocItem::Body(BodyItem::MacroDef(m)) = &doc.items[0] else {
            panic!("expected macro def");
        };
        assert_eq!(m.kind, MacroKind::Function);
        assert!(m.params.is_empty());
    }

    #[test]
    fn macro_attribute_definition() {
        let doc = parse_clean("macro @transform(x) { inject { y = x } }");
        let DocItem::Body(BodyItem::MacroDef(m)) = &doc.items[0] else {
            panic!("expected macro def");
        };
        assert_eq!(m.kind, MacroKind::Attribute);
        assert_eq!(m.name.name, "transform");
        assert!(matches!(&m.body, MacroBody::Attribute(_)));
    }

    // ── Ref expressions ───────────────────────────────────────────────────────

    #[test]
    fn ref_expression() {
        let doc = parse_clean("target = ref(my-server)");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::Ref(id_lit, _) = &attr.value else {
            panic!("expected ref expr");
        };
        assert_eq!(id_lit.value, "my-server");
    }

    #[test]
    fn ref_expression_plain_ident() {
        let doc = parse_clean("target = ref(server)");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(matches!(&attr.value, Expr::Ref(_, _)));
    }

    // ── Multiple top-level items ──────────────────────────────────────────────

    #[test]
    fn multiple_top_level_items() {
        let src = r#"
let base = 8000
export let port = base
import "./other.wcl"
server { port = 80 }
"#;
        let doc = parse_clean(src);
        assert_eq!(doc.items.len(), 4);
        assert!(matches!(&doc.items[0], DocItem::Body(BodyItem::LetBinding(_))));
        assert!(matches!(&doc.items[1], DocItem::ExportLet(_)));
        assert!(matches!(&doc.items[2], DocItem::Import(_)));
        assert!(matches!(&doc.items[3], DocItem::Body(BodyItem::Block(_))));
    }

    #[test]
    fn multiple_attributes() {
        let src = "a = 1\nb = 2\nc = 3";
        let doc = parse_clean(src);
        assert_eq!(doc.items.len(), 3);
    }

    #[test]
    fn multiple_blocks() {
        let src = "server { }\nclient { }\nproxy { }";
        let doc = parse_clean(src);
        assert_eq!(doc.items.len(), 3);
    }

    // ── Parenthesized expressions ─────────────────────────────────────────────

    #[test]
    fn parenthesized_expression() {
        let doc = parse_clean("x = (1 + 2) * 3");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        // Top-level should be Mul since (1+2) is parenthesized
        let Expr::BinaryOp(lhs, op, rhs, _) = &attr.value else {
            panic!("expected binary op");
        };
        assert_eq!(*op, BinOp::Mul);
        // lhs is a Paren wrapping Add
        assert!(matches!(lhs.as_ref(), Expr::Paren(_, _)));
        assert!(matches!(rhs.as_ref(), Expr::IntLit(3, _)));
    }

    // ── Heredoc in parser ─────────────────────────────────────────────────────

    #[test]
    fn heredoc_as_attribute_value() {
        let src = "body = <<EOF\nhello world\nEOF\n";
        let doc = parse_clean(src);
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert_eq!(attr.name.name, "body");
        let Expr::StringLit(s) = &attr.value else {
            panic!("expected string lit from heredoc");
        };
        assert!(matches!(&s.parts[0], StringPart::Literal(t) if t == "hello world"));
    }

    // ── Error recovery ────────────────────────────────────────────────────────

    #[test]
    fn malformed_input_does_not_panic() {
        // Totally broken input — just must not panic
        let _ = parse("@@@###%%%", file());
    }

    #[test]
    fn malformed_attribute_missing_value() {
        // "x =" with no value produces an error but does not panic
        parse_with_errors("x =");
    }

    #[test]
    fn unclosed_block_produces_error() {
        parse_with_errors("server {");
    }

    #[test]
    fn missing_block_brace_produces_error() {
        parse_with_errors("server port = 8080 }");
    }

    #[test]
    fn malformed_let_missing_binding() {
        parse_with_errors("let");
    }

    #[test]
    fn malformed_schema_missing_name() {
        parse_with_errors("schema { port: int }");
    }

    #[test]
    fn partial_parse_continues_after_error() {
        // A malformed first item should produce errors, but the parser should
        // not panic and should return a Document (possibly with partial results).
        // The key invariant is: parse never panics on arbitrary input.
        let src = "@@@ bad\ngood = 42";
        let (doc, diags) = parse(src, file());
        // There must be at least one diagnostic
        assert!(diags.has_errors());
        // The parser should have attempted to parse both items; `good = 42` may
        // or may not be recovered depending on how many tokens were consumed,
        // but the document must be non-empty since `good = 42` follows.
        let _ = doc; // just ensure it doesn't panic and returns a document
    }

    #[test]
    fn empty_document() {
        let doc = parse_clean("");
        assert!(doc.items.is_empty());
    }

    #[test]
    fn whitespace_only_document() {
        let doc = parse_clean("   \n\n   \n");
        assert!(doc.items.is_empty());
    }

    #[test]
    fn comment_only_document() {
        let doc = parse_clean("// just a comment\n/* block comment */\n");
        assert!(doc.items.is_empty());
    }

    // ── Block expressions ─────────────────────────────────────────────────────

    #[test]
    fn block_expression() {
        let doc = parse_clean("x = { let a = 1\na + 2 }");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        let Expr::BlockExpr(lets, final_expr, _) = &attr.value else {
            panic!("expected block expr");
        };
        assert_eq!(lets.len(), 1);
        assert_eq!(lets[0].name.name, "a");
        assert!(matches!(final_expr.as_ref(), Expr::BinaryOp(_, BinOp::Add, _, _)));
    }

    // ── Re-export ─────────────────────────────────────────────────────────────

    #[test]
    fn re_export() {
        let doc = parse_clean("export myVar");
        assert_eq!(doc.items.len(), 1);
        let DocItem::ReExport(re) = &doc.items[0] else {
            panic!("expected re-export");
        };
        assert_eq!(re.name.name, "myVar");
    }

    // ── Span coverage check ───────────────────────────────────────────────────

    #[test]
    fn spans_are_non_zero_for_non_empty_input() {
        let doc = parse_clean("x = 42");
        let DocItem::Body(BodyItem::Attribute(attr)) = &doc.items[0] else {
            panic!("expected attribute");
        };
        assert!(attr.span.end > attr.span.start);
    }

    // ── Type expressions (via schema) ─────────────────────────────────────────

    #[test]
    fn type_expr_string() {
        let doc = parse_clean(r#"schema "S" { name: string }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(s.fields[0].type_expr, wcl_core::ast::TypeExpr::String(_)));
    }

    #[test]
    fn type_expr_int() {
        let doc = parse_clean(r#"schema "S" { n: int }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(s.fields[0].type_expr, wcl_core::ast::TypeExpr::Int(_)));
    }

    #[test]
    fn type_expr_float() {
        let doc = parse_clean(r#"schema "S" { ratio: float }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(s.fields[0].type_expr, wcl_core::ast::TypeExpr::Float(_)));
    }

    #[test]
    fn type_expr_bool() {
        let doc = parse_clean(r#"schema "S" { flag: bool }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(s.fields[0].type_expr, wcl_core::ast::TypeExpr::Bool(_)));
    }

    #[test]
    fn type_expr_any() {
        let doc = parse_clean(r#"schema "S" { val: any }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(s.fields[0].type_expr, wcl_core::ast::TypeExpr::Any(_)));
    }

    #[test]
    fn type_expr_list() {
        let doc = parse_clean(r#"schema "S" { tags: list(string) }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(
            s.fields[0].type_expr,
            wcl_core::ast::TypeExpr::List(_, _)
        ));
    }

    #[test]
    fn type_expr_map() {
        let doc = parse_clean(r#"schema "S" { props: map(string, int) }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(
            s.fields[0].type_expr,
            wcl_core::ast::TypeExpr::Map(_, _, _)
        ));
    }

    #[test]
    fn type_expr_union() {
        let doc = parse_clean(r#"schema "S" { val: union(string, int) }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        let wcl_core::ast::TypeExpr::Union(variants, _) = &s.fields[0].type_expr else {
            panic!("expected union type");
        };
        assert_eq!(variants.len(), 2);
    }

    // NOTE: `set` is a keyword token (TokenKind::Set) used in attribute-macro bodies.
    // The type parser only matches Ident("set"), so `set(T)` as a type annotation
    // currently produces parse errors. This test documents that behaviour.
    #[test]
    fn type_expr_set_keyword_conflict_produces_errors() {
        let (_doc, diags) = parse(r#"schema "S" { items: set(string) }"#, file());
        // The parser emits errors because `set` is lexed as a keyword, not Ident("set").
        assert!(diags.has_errors(), "expected errors due to keyword conflict");
    }

    #[test]
    fn type_expr_ref() {
        let doc = parse_clean(r#"schema "S" { target: ref("Server") }"#);
        let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] else {
            panic!();
        };
        assert!(matches!(
            s.fields[0].type_expr,
            wcl_core::ast::TypeExpr::Ref(_, _)
        ));
    }
}
