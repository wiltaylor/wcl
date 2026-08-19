use super::*;

fn tokens(src: &str) -> Vec<TokenKind> {
    let mut lex = Lexer::new(src);
    let mut out = Vec::new();
    loop {
        let t = lex.next_token().expect("lex error");
        let done = matches!(t.kind, TokenKind::Eof);
        out.push(t.kind);
        if done {
            break;
        }
    }
    out
}

fn one(src: &str) -> TokenKind {
    let mut lex = Lexer::new(src);
    let t = lex.next_token().expect("lex");
    t.kind
}

#[test]
fn default_int_is_i64() {
    assert_eq!(one("42"), TokenKind::Number(NumberLit::I64(42)));
}

#[test]
fn default_float_is_f64() {
    assert_eq!(one("1.25"), TokenKind::Number(NumberLit::F64(1.25)));
}

#[test]
fn typed_int_suffix() {
    assert_eq!(one("8080i32"), TokenKind::Number(NumberLit::I32(8080)));
    assert_eq!(one("200u8"), TokenKind::Number(NumberLit::U8(200)));
    assert_eq!(one("-128i8"), TokenKind::Number(NumberLit::I8(-128)));
}

#[test]
fn typed_float_suffix() {
    assert_eq!(one("1.5f32"), TokenKind::Number(NumberLit::F32(1.5)));
}

#[test]
fn literal_unit_suffix() {
    // An unrecognised suffix lexes as a unit-bearing number, magnitude
    // defaulting to i64 (int) / f64 (float).
    assert_eq!(
        one("5MiB"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::I64(5), "MiB".to_string())))
    );
    assert_eq!(
        one("512KiB"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::I64(512), "KiB".to_string())))
    );
    assert_eq!(
        one("1.5km"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::F64(1.5), "km".to_string())))
    );
    assert_eq!(
        one("-5MiB"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::I64(-5), "MiB".to_string())))
    );
}

#[test]
fn underscores_in_digits() {
    assert_eq!(
        one("1_000_000"),
        TokenKind::Number(NumberLit::I64(1_000_000))
    );
}

#[test]
fn hex_bin_oct_bases() {
    assert_eq!(one("0xFFu8"), TokenKind::Number(NumberLit::U8(255)));
    assert_eq!(
        one("0b1010_1100u8"),
        TokenKind::Number(NumberLit::U8(0b1010_1100))
    );
    assert_eq!(one("0o755u16"), TokenKind::Number(NumberLit::U16(0o755)));
    // unsuffixed hex defaults to i64
    assert_eq!(one("0x10"), TokenKind::Number(NumberLit::I64(16)));
}

#[test]
fn overflow_errors_with_literal_span() {
    let mut lex = Lexer::new("200i8");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("out of range"));
    assert_eq!(err.span, Span::new(0, 5));
}

#[test]
fn negative_unsigned_errors() {
    let mut lex = Lexer::new("-1u32");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("unsigned"));
}

#[test]
fn unknown_suffix_becomes_unit() {
    // An unrecognised suffix is no longer a lex error — it lexes as a
    // literal unit (resolved later against the declared type).
    assert_eq!(
        one("1zz"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::I64(1), "zz".to_string())))
    );
}

#[test]
fn invalid_digit_for_base() {
    let mut lex = Lexer::new("0b2");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("invalid digit"));
}

#[test]
fn trailing_underscore_rejected() {
    let mut lex = Lexer::new("1_000_");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("trailing"));
}

#[test]
fn float_exponent_requires_decimal_point() {
    // Per the lexer rules: float mode is triggered by '.' followed by
    // digit. `2e3` is therefore not a float; `e3` is taken as a suffix,
    // and since it isn't a numeric type suffix it lexes as a literal
    // unit named `e3` (which fails later unless such a unit is declared)
    // — write `2.0e3` for scientific notation. Keeps the grammar simple.
    assert_eq!(
        one("2e3"),
        TokenKind::NumberWithUnit(Box::new((NumberLit::I64(2), "e3".to_string())))
    );
}

#[test]
fn float_with_explicit_decimal_and_exponent() {
    assert_eq!(one("1.5e3"), TokenKind::Number(NumberLit::F64(1500.0)));
    assert_eq!(one("2.0e-3"), TokenKind::Number(NumberLit::F64(0.002)));
}

#[test]
fn default_utf8_string() {
    assert_eq!(
        one(r#""hello""#),
        TokenKind::Str(StringLit::Utf8("hello".into()))
    );
}

#[test]
fn explicit_utf8_prefix() {
    assert_eq!(
        one(r#"utf8"hi""#),
        TokenKind::Str(StringLit::Utf8("hi".into()))
    );
}

#[test]
fn ascii_prefix_validates() {
    assert_eq!(
        one(r#"ascii"alpha""#),
        TokenKind::Str(StringLit::Ascii("alpha".into()))
    );
    let mut lex = Lexer::new("ascii\"héllo\"");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("non-ASCII"));
}

#[test]
fn utf16_prefix_encodes() {
    let TokenKind::Str(StringLit::Utf16(v)) = one(r#"utf16"hi""#) else {
        panic!("expected utf16 string")
    };
    assert_eq!(v, vec![0x68, 0x69]);
}

#[test]
fn utf32_prefix_decodes() {
    let TokenKind::Str(StringLit::Utf32(v)) = one(r#"utf32"hi""#) else {
        panic!("expected utf32 string")
    };
    assert_eq!(v, vec!['h', 'i']);
}

#[test]
fn ident_named_utf16_followed_by_equals_is_ident() {
    // `utf16 = 1` should lex as Ident("utf16"), Eq, Number(I64(1)).
    assert_eq!(
        tokens("utf16 = 1"),
        vec![
            TokenKind::Ident("utf16".into()),
            TokenKind::Eq,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_block_punctuation_and_bool() {
    assert_eq!(
        tokens("service { on = true }"),
        vec![
            TokenKind::Ident("service".into()),
            TokenKind::LBrace,
            TokenKind::Ident("on".into()),
            TokenKind::Eq,
            TokenKind::Bool(true),
            TokenKind::RBrace,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_comments_are_skipped() {
    assert_eq!(
        tokens("# hash\n// slash\nx = 1"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn unterminated_string_errors() {
    let mut lex = Lexer::new(r#"x = "abc"#);
    assert!(matches!(
        lex.next_token().unwrap().kind,
        TokenKind::Ident(_)
    ));
    assert!(matches!(lex.next_token().unwrap().kind, TokenKind::Eq));
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("unterminated"));
}

#[test]
fn punctuation_colon_and_question() {
    assert_eq!(
        tokens("name: utf8?"),
        vec![
            TokenKind::Ident("name".into()),
            TokenKind::Colon,
            TokenKind::Ident("utf8".into()),
            TokenKind::Question,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_generic_and_bracket_tokens() {
    assert_eq!(
        tokens("tensor<f32, [3, 4]>"),
        vec![
            TokenKind::Ident("tensor".into()),
            TokenKind::Lt,
            TokenKind::Ident("f32".into()),
            TokenKind::Comma,
            TokenKind::LBracket,
            TokenKind::Number(NumberLit::I64(3)),
            TokenKind::Comma,
            TokenKind::Number(NumberLit::I64(4)),
            TokenKind::RBracket,
            TokenKind::Gt,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_at_token() {
    assert_eq!(
        tokens("@foo"),
        vec![
            TokenKind::At,
            TokenKind::Ident("foo".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_parens() {
    assert_eq!(
        tokens("(1, 2)"),
        vec![
            TokenKind::LParen,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Comma,
            TokenKind::Number(NumberLit::I64(2)),
            TokenKind::RParen,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_symbol_literal_tight() {
    assert_eq!(
        tokens(":red"),
        vec![TokenKind::Symbol("red".into()), TokenKind::Eof]
    );
}

#[test]
fn lex_symbol_with_underscore_and_digit() {
    assert_eq!(
        tokens(":foo_bar2"),
        vec![TokenKind::Symbol("foo_bar2".into()), TokenKind::Eof]
    );
}

#[test]
fn lex_colon_with_space() {
    assert_eq!(
        tokens("name: utf8"),
        vec![
            TokenKind::Ident("name".into()),
            TokenKind::Colon,
            TokenKind::Ident("utf8".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_colon_alone_at_eof() {
    assert_eq!(
        tokens("x :"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Colon,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn dot_separates_path_segments() {
    assert_eq!(
        tokens("foo.bar.baz"),
        vec![
            TokenKind::Ident("foo".into()),
            TokenKind::Dot,
            TokenKind::Ident("bar".into()),
            TokenKind::Dot,
            TokenKind::Ident("baz".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn comma_lexes_as_comma() {
    assert_eq!(
        tokens("a , b"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Comma,
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn amp_lexes_as_amp() {
    assert_eq!(
        tokens("&User"),
        vec![
            TokenKind::Amp,
            TokenKind::Ident("User".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn none_keyword_lexes_distinctly_from_ident() {
    assert_eq!(one("none"), TokenKind::None);
    assert_eq!(one("nonexistent"), TokenKind::Ident("nonexistent".into()));
}

#[test]
fn ascii_byte_below_0x80_validates() {
    // Confirm 0x7F (DEL) is accepted as ASCII.
    let mut lex = Lexer::new("ascii\"\x7F\"");
    let t = lex.next_token().unwrap();
    assert!(matches!(t.kind, TokenKind::Str(StringLit::Ascii(_))));
}

#[test]
fn lex_arithmetic_operators() {
    assert_eq!(
        tokens("+ - * / %"),
        vec![
            TokenKind::Plus,
            TokenKind::Dash,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_compound_eq_and_inequality() {
    assert_eq!(
        tokens("= == != !"),
        vec![
            TokenKind::Eq,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::Bang,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_compound_lt_gt() {
    assert_eq!(
        tokens("< <= > >="),
        vec![
            TokenKind::Lt,
            TokenKind::LtEq,
            TokenKind::Gt,
            TokenKind::GtEq,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_logical_ops() {
    assert_eq!(
        tokens("&& || & |"),
        vec![
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::Amp,
            TokenKind::Pipe,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_arrow_vs_dash() {
    assert_eq!(
        tokens("-> -"),
        vec![TokenKind::Arrow, TokenKind::Dash, TokenKind::Eof]
    );
}

#[test]
fn lex_semi_token() {
    assert_eq!(
        tokens("a ; b"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Semi,
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_signed_number_after_ident_is_subtraction() {
    // `a-1` and `a - 1` should both lex with `-` as Dash so the parser
    // can treat them as subtraction once expressions exist.
    assert_eq!(
        tokens("a-1"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Dash,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        tokens("a - 1"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Dash,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_kebab_and_path_label_token_streams() {
    // The label-stitching parser relies on `-`/`/` staying standalone
    // Dash/Slash tokens, with a trailing digit staying a Number.
    assert_eq!(
        tokens("dgm-box"),
        vec![
            TokenKind::Ident("dgm".into()),
            TokenKind::Dash,
            TokenKind::Ident("box".into()),
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        tokens("data-series-1"),
        vec![
            TokenKind::Ident("data".into()),
            TokenKind::Dash,
            TokenKind::Ident("series".into()),
            TokenKind::Dash,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        tokens("api/v1/users"),
        vec![
            TokenKind::Ident("api".into()),
            TokenKind::Slash,
            TokenKind::Ident("v1".into()),
            TokenKind::Slash,
            TokenKind::Ident("users".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_signed_number_at_start_or_after_separator() {
    // After whitespace at start-of-file: signed literal.
    assert_eq!(
        tokens("-5"),
        vec![TokenKind::Number(NumberLit::I64(-5)), TokenKind::Eof]
    );
    // After `=` (no value to its left): signed literal.
    assert_eq!(
        tokens("x=-5"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::Number(NumberLit::I64(-5)),
            TokenKind::Eof,
        ]
    );
    // After `(`: signed literal.
    assert_eq!(
        tokens("(-5)"),
        vec![
            TokenKind::LParen,
            TokenKind::Number(NumberLit::I64(-5)),
            TokenKind::RParen,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lex_signed_number_after_value_terminator_is_subtraction() {
    // After `)`: subtraction. After `]`: subtraction. After `}`: subtraction.
    assert_eq!(
        tokens(")-1"),
        vec![
            TokenKind::RParen,
            TokenKind::Dash,
            TokenKind::Number(NumberLit::I64(1)),
            TokenKind::Eof,
        ]
    );
}

// ---- heredocs --------------------------------------------------

fn lex_str(src: &str) -> StringLit {
    match one(src) {
        TokenKind::Str(s) => s,
        other => panic!("expected Str, got {other:?}"),
    }
}

#[test]
fn heredoc_basic_two_line_body() {
    let s = "<<END\nfirst\nsecond\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf8("first\nsecond\n".into()));
}

#[test]
fn heredoc_strips_common_indent() {
    // 4-space common indent across non-blank lines. Blank line in
    // the middle stays blank in the output.
    let s = "<<END\n    foo\n\n    bar\n    END\n";
    assert_eq!(lex_str(s), StringLit::Utf8("foo\n\nbar\n".into()));
}

#[test]
fn heredoc_indent_ignores_blank_lines() {
    // The leading-whitespace-only line should not contribute to the
    // minimum-indent calculation.
    let s = "<<END\n  foo\n  \n  bar\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf8("foo\n\nbar\n".into()));
}

#[test]
fn heredoc_interprets_escapes() {
    let s = "<<END\nhi\\tthere\\nline\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf8("hi\tthere\nline\n".into()));
}

#[test]
fn raw_heredoc_takes_body_verbatim() {
    // `<<'TAG'` — backslashes are literal, no escape interpretation,
    // so LaTeX (`\frac`, `\theta`) survives unmangled. A plain
    // heredoc would reject `\f` / turn `\t` into a tab.
    let s = "<<'TEX'\n\\frac{a}{b} \\theta\nTEX\n";
    assert_eq!(lex_str(s), StringLit::Utf8("\\frac{a}{b} \\theta\n".into()));
}

#[test]
fn raw_heredoc_ignores_interpolation_slots() {
    // `${…}` is literal in a raw heredoc, not an interpolation slot.
    let s = "<<'RAW'\na ${b} c\nRAW\n";
    assert_eq!(lex_str(s), StringLit::Utf8("a ${b} c\n".into()));
}

#[test]
fn raw_heredoc_strips_common_indent() {
    let s = "<<'RAW'\n    \\foo\n\n    \\bar\n    RAW\n";
    assert_eq!(lex_str(s), StringLit::Utf8("\\foo\n\n\\bar\n".into()));
}

#[test]
fn raw_heredoc_unclosed_tag_quote_errors() {
    let mut lex = Lexer::new("<<'TEX\n\\frac\nTEX\n");
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("single quote"), "got: {}", err.message);
}

#[test]
fn heredoc_ascii_prefix_validates() {
    let s = "ascii<<END\nplain ascii\nEND\n";
    assert_eq!(lex_str(s), StringLit::Ascii("plain ascii\n".into()));
}

#[test]
fn heredoc_ascii_prefix_rejects_non_ascii() {
    let s = "ascii<<END\nplain\u{2713}\nEND\n";
    let mut lex = Lexer::new(s);
    let err = lex.next_token().unwrap_err();
    assert!(err.message.contains("non-ASCII"), "got: {}", err.message);
}

#[test]
fn heredoc_utf16_prefix_encodes_body() {
    let s = "utf16<<END\nhi\nEND\n";
    // body = "hi\n" → UTF-16: [0x68, 0x69, 0x0a]
    assert_eq!(lex_str(s), StringLit::Utf16(vec![0x68, 0x69, 0x0a]));
}

#[test]
fn heredoc_utf32_prefix_encodes_body() {
    let s = "utf32<<END\nhi\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf32(vec!['h', 'i', '\n']));
}

#[test]
fn heredoc_unterminated_errors() {
    let mut lex = Lexer::new("<<END\nfoo\nbar\n");
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("unterminated heredoc"),
        "got: {}",
        err.message
    );
}

#[test]
fn heredoc_junk_after_tag_errors() {
    let mut lex = Lexer::new("<<END oops\nfoo\nEND\n");
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("unexpected text after heredoc tag"),
        "got: {}",
        err.message
    );
}

#[test]
fn heredoc_comment_after_tag_is_fine() {
    // A trailing `# comment` on the opener line is trivia.
    let s = "<<END  # a comment\nfoo\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf8("foo\n".into()));
}

#[test]
fn heredoc_empty_body() {
    let s = "<<END\nEND\n";
    assert_eq!(lex_str(s), StringLit::Utf8(String::new()));
}

#[test]
fn heredoc_closer_with_leading_whitespace() {
    // Closer may be indented; common-indent strip still applies to
    // the body using the minimum across non-blank body lines.
    let s = "<<END\n    foo\n    bar\n    END\n";
    assert_eq!(lex_str(s), StringLit::Utf8("foo\nbar\n".into()));
}

#[test]
fn heredoc_pair_in_one_source() {
    let src = "<<A\none\nA\n<<B\ntwo\nB\n";
    let toks = tokens(src);
    assert_eq!(toks.len(), 3); // two strings + Eof
    assert_eq!(toks[0], TokenKind::Str(StringLit::Utf8("one\n".into())));
    assert_eq!(toks[1], TokenKind::Str(StringLit::Utf8("two\n".into())));
}

#[test]
fn double_lt_with_non_ident_still_lt_lt() {
    // `<<` followed by non-ident-start should not be misread as a
    // heredoc — keep the existing two `Lt` token sequence.
    let toks = tokens("<<=");
    assert_eq!(toks[0], TokenKind::Lt);
    assert_eq!(toks[1], TokenKind::LtEq);
}

// ---- interpolation ---------------------------------------------

fn interp_parts(src: &str) -> Vec<StringPart> {
    match one(src) {
        TokenKind::Str(StringLit::Interpolated { parts, .. }) => parts,
        other => panic!("expected Interpolated, got {other:?}"),
    }
}

#[test]
fn interp_string_basic_slot() {
    let parts = interp_parts(r#"$"hi ${name}!""#);
    assert_eq!(parts.len(), 3);
    match &parts[0] {
        StringPart::Literal(s) => assert_eq!(s, "hi "),
        other => panic!("[0]: {other:?}"),
    }
    match &parts[1] {
        StringPart::Expr { text, .. } => assert_eq!(text, "name"),
        other => panic!("[1]: {other:?}"),
    }
    match &parts[2] {
        StringPart::Literal(s) => assert_eq!(s, "!"),
        other => panic!("[2]: {other:?}"),
    }
}

#[test]
fn interp_string_multiple_slots() {
    let parts = interp_parts(r#"$"${a}-${b}""#);
    assert_eq!(parts.len(), 3);
    match &parts[0] {
        StringPart::Expr { text, .. } => assert_eq!(text, "a"),
        other => panic!("[0]: {other:?}"),
    }
    match &parts[1] {
        StringPart::Literal(s) => assert_eq!(s, "-"),
        other => panic!("[1]: {other:?}"),
    }
    match &parts[2] {
        StringPart::Expr { text, .. } => assert_eq!(text, "b"),
        other => panic!("[2]: {other:?}"),
    }
}

#[test]
fn interp_slot_with_braces_and_strings() {
    // Brace balance + string-skip inside the slot.
    let parts = interp_parts(r#"$"x=${foo({y = "hi"})}""#);
    assert_eq!(parts.len(), 2);
    match &parts[1] {
        StringPart::Expr { text, .. } => {
            assert_eq!(text, r#"foo({y = "hi"})"#);
        }
        other => panic!("[1]: {other:?}"),
    }
}

#[test]
fn interp_dollar_escape_emits_literal_dollar() {
    let parts = interp_parts(r#"$"price=\$5""#);
    assert_eq!(parts.len(), 1);
    match &parts[0] {
        StringPart::Literal(s) => assert_eq!(s, "price=$5"),
        other => panic!("[0]: {other:?}"),
    }
}

#[test]
fn plain_string_treats_dollar_as_literal() {
    // No `$` prefix → `$` and `${...}` are plain bytes.
    let s = one(r#""price=$5 ${x}""#);
    assert_eq!(s, TokenKind::Str(StringLit::Utf8("price=$5 ${x}".into())));
}

#[test]
fn interp_typed_ascii_carries_encoding() {
    let toks = tokens(r#"$ascii"id=${k}""#);
    match &toks[0] {
        TokenKind::Str(StringLit::Interpolated { encoding, .. }) => {
            assert_eq!(*encoding, StringEncoding::Ascii);
        }
        other => panic!("expected ascii interpolated, got {other:?}"),
    }
}

#[test]
fn interp_heredoc_collects_slots_per_line() {
    let src = "$<<END\n  port=${cfg.port}\n  END\n";
    let parts = interp_parts(src);
    // ["port=", Expr("cfg.port"), "\n"]
    assert!(matches!(parts[0], StringPart::Literal(ref s) if s == "port="));
    assert!(matches!(parts[1], StringPart::Expr { ref text, .. } if text == "cfg.port"));
    // The trailing literal carries the joining newline.
    match parts.last().unwrap() {
        StringPart::Literal(s) => assert_eq!(s, "\n"),
        other => panic!("last: {other:?}"),
    }
}

#[test]
fn interp_slot_unterminated_errors() {
    let mut lex = Lexer::new(r#"$"hi ${name"#);
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("unterminated") || err.message.contains("slot"),
        "got: {}",
        err.message
    );
}

#[test]
fn interp_slot_rejects_newline() {
    let mut lex = Lexer::new("$\"hi ${name\n}\"");
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("multiple lines") || err.message.contains("newline"),
        "got: {}",
        err.message
    );
}

#[test]
fn interp_slot_rejects_nested_heredoc() {
    let mut lex = Lexer::new("$\"${<<X\nhi\nX\n}\"");
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("heredoc literals are not allowed"),
        "got: {}",
        err.message
    );
}

#[test]
fn dollar_alone_errors() {
    let mut lex = Lexer::new("$ hello");
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("expected '\"' or '<<'"),
        "got: {}",
        err.message
    );
}

#[test]
fn dollar_with_unknown_encoding_errors() {
    let mut lex = Lexer::new(r#"$bogus"x""#);
    let err = lex.next_token().unwrap_err();
    assert!(
        err.message.contains("unknown string encoding"),
        "got: {}",
        err.message
    );
}
