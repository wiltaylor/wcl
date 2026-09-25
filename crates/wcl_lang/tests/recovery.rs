//! Parser error recovery: one parse reports every syntax error in a
//! file, keeps the items around them, and a single mistake yields a
//! single error rather than a cascade.

use std::path::{Path, PathBuf};

use proptest::prelude::*;
use wcl_lang::ast::{Item, Source};
use wcl_lang::{
    Document, MAX_SYNTAX_ERRORS, ParseError, PartialParse, SyntaxError, format, parse_for_edit,
    parse_for_edit_recovering,
};

fn recover(src: &str) -> PartialParse {
    parse_for_edit_recovering(src, "test.wcl")
}

/// `(line, column)` of an error's span start, both 1-based.
fn position(src: &str, err: &SyntaxError) -> (usize, usize) {
    let offset = err.span.offset();
    let before = &src[..offset];
    let line = before.matches('\n').count() + 1;
    let column = offset - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, column)
}

/// Each error as `(line, column, message)`.
fn errors(src: &str, parsed: &PartialParse) -> Vec<(usize, usize, String)> {
    parsed
        .errors
        .iter()
        .map(|e| {
            let (line, column) = position(src, e);
            (line, column, e.message.clone())
        })
        .collect()
}

/// Names of the fields and kinds of the blocks in `items`, nested
/// bodies bracketed, e.g. `["a", "svc{port, host}"]`.
fn outline(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            Item::Field(f) => f.name.clone(),
            Item::Block(b) => format!("{}{{{}}}", b.kind, outline(&b.items).join(", ")),
            other => format!("{other:?}").chars().take(20).collect(),
        })
        .collect()
}

#[test]
fn errors_in_separate_items_are_all_reported() {
    let src = "a = = 1\nb = 2\nc = ]\nd = 4\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![
            (1, 5, "expected value, found '='".to_string()),
            (3, 5, "expected value, found ']'".to_string()),
        ]
    );
    assert_eq!(outline(&parsed.source.items), vec!["b", "d"]);
}

#[test]
fn errors_in_nested_bodies_keep_their_siblings() {
    let src = "\
outer {
  inner {
    x = = 1
    y = 2
  }
  z = )
  w = 3
}
after = 1
";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![
            (3, 9, "expected value, found '='".to_string()),
            (6, 7, "expected value, found ')'".to_string()),
        ]
    );
    assert_eq!(
        outline(&parsed.source.items),
        vec!["outer{inner{y}, w}", "after"]
    );
}

#[test]
fn unterminated_string_costs_only_its_line() {
    let src = "a = \"abc\nb = 2\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(1, 5, "newline in string literal".to_string())]
    );
    assert_eq!(outline(&parsed.source.items), vec!["b"]);
}

#[test]
fn unterminated_heredoc_is_one_error() {
    let src = "a = 1\nb = <<EOF\nline one\nc = 2\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(
            2,
            5,
            "unterminated heredoc starting with '<<EOF'".to_string()
        )]
    );
    assert_eq!(outline(&parsed.source.items), vec!["a"]);
}

#[test]
fn missing_closing_brace_at_eof_keeps_the_block() {
    let src = "a {\n  x = 1\n  b {\n    y = 2\n  }\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(6, 1, "unexpected end of file inside block".to_string())]
    );
    assert_eq!(outline(&parsed.source.items), vec!["a{x, b{y}}"]);
}

#[test]
fn several_unclosed_blocks_report_end_of_file_once() {
    let src = "a {\n  b {\n    c {\n      x = 1\n";
    let parsed = recover(src);
    assert_eq!(parsed.errors.len(), 1, "{:?}", errors(src, &parsed));
    assert_eq!(outline(&parsed.source.items), vec!["a{b{c{x}}}"]);
}

#[test]
fn one_mistake_in_a_multiline_list_is_one_error() {
    let src = "\
x = [
  foo
  bar baz
  qux
]
y = 1
";
    let parsed = recover(src);
    assert_eq!(parsed.errors.len(), 1, "{:?}", errors(src, &parsed));
    assert_eq!(outline(&parsed.source.items), vec!["y"]);
}

#[test]
fn one_mistake_in_multiline_call_arguments_is_one_error() {
    let src = "\
@schemaless
svc {
  x = f(1,
    2 3,
    a,
    4)
  y = 1
}
";
    let parsed = recover(src);
    assert_eq!(parsed.errors.len(), 1, "{:?}", errors(src, &parsed));
    assert_eq!(outline(&parsed.source.items), vec!["svc{y}"]);
}

#[test]
fn an_unclosed_paren_does_not_swallow_the_file() {
    let src = "a = f(1,\nb = 2\nc = 3\n";
    let parsed = recover(src);
    assert_eq!(parsed.errors.len(), 1, "{:?}", errors(src, &parsed));
    assert_eq!(outline(&parsed.source.items), vec!["c"]);
}

#[test]
fn an_error_that_eats_the_closing_brace_ends_the_body() {
    let src = "svc {\n  a = 1\n  b = }\nc = 2\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(3, 7, "expected value, found '}'".to_string())]
    );
    assert_eq!(outline(&parsed.source.items), vec!["svc{a}", "c"]);
}

#[test]
fn a_stray_closing_brace_is_one_error() {
    let src = "a = 1\n}\nb = 2\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(2, 1, "expected identifier, found '}'".to_string())]
    );
    assert_eq!(outline(&parsed.source.items), vec!["a", "b"]);
}

#[test]
fn a_bad_character_is_one_error() {
    let src = "a = 1 ~ 2\nb = 2\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![(1, 7, "unexpected character '~'".to_string())]
    );
    assert_eq!(outline(&parsed.source.items), vec!["b"]);
}

#[test]
fn duplicate_declarations_are_reported_alongside_syntax_errors() {
    let src = "a = 1\na = 2\nb = = 3\n";
    let parsed = recover(src);
    assert_eq!(
        errors(src, &parsed),
        vec![
            (2, 1, "duplicate declaration 'a'".to_string()),
            (3, 5, "expected value, found '='".to_string()),
        ]
    );
}

#[test]
fn errors_are_capped() {
    let src = "a = =\n".repeat(MAX_SYNTAX_ERRORS * 3);
    let parsed = recover(&src);
    assert_eq!(parsed.errors.len(), MAX_SYNTAX_ERRORS);
}

/// Found by fuzzing: the cap is reached inside a block body, and the
/// item loops unwinding out of it met further errors on the way.
#[test]
fn errors_stay_capped_when_the_cap_is_reached_inside_a_body() {
    for tail in ["~\n", "}\n~ ~\n", "\"open\n"] {
        let src = format!("a {{\n{}{tail}", "  x = =\n".repeat(MAX_SYNTAX_ERRORS));
        let parsed = recover(&src);
        assert_eq!(parsed.errors.len(), MAX_SYNTAX_ERRORS, "tail {tail:?}");
    }
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recovery_cap.wcl"),
    )
    .expect("fixture");
    assert_eq!(recover(&src).errors.len(), MAX_SYNTAX_ERRORS);
}

#[test]
fn the_partial_tree_carries_symbols() {
    let src = "type T { name: utf8 }\nbroken = = 1\nname = \"x\"\n";
    let parsed = recover(src);
    assert_eq!(parsed.errors.len(), 1);
    let mut names: Vec<&str> = parsed.symbols.iter().map(|r| r.fqn.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["T", "T.name", "name"]);
}

#[test]
fn strict_parse_reports_every_error() {
    let src = "a = = 1\nb = 2\nc = ]\n";
    let err = parse_for_edit(src, "test.wcl").expect_err("two syntax errors");
    let ParseError::Syntax(first) = &err else {
        panic!("not a syntax error: {err:?}")
    };
    assert_eq!(first.message, "expected value, found '='");
    assert_eq!(first.others.len(), 1);
    let all: Vec<&str> = err.syntax_errors().map(|e| e.message.as_str()).collect();
    assert_eq!(
        all,
        vec!["expected value, found '='", "expected value, found ']'"]
    );
    // Opening a document reports the same set.
    let err = Document::open(src, "test.wcl").expect_err("does not open");
    assert_eq!(err.syntax_errors().count(), 2);
}

#[test]
fn miette_renders_every_error() {
    use miette::Diagnostic;
    let err = parse_for_edit("a = = 1\nb = ]\n", "test.wcl").expect_err("fails");
    let related = err.related().expect("related diagnostics").count();
    assert_eq!(related, 1);
}

fn example_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("examples dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            example_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "wcl") {
            out.push(path);
        }
    }
}

/// On every example file the recovering parse agrees with the strict
/// one: the same tree when it parses, the same first error when not.
#[test]
fn recovering_parse_agrees_with_strict_parse_on_examples() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut files = Vec::new();
    example_files(&root, &mut files);
    assert!(files.len() > 20, "found {} examples", files.len());
    for file in files {
        let src = std::fs::read_to_string(&file).expect("read example");
        let name = file.display().to_string();
        let recovered = parse_for_edit_recovering(&src, &name);
        match parse_for_edit(&src, &name) {
            Ok(tree) => {
                assert!(
                    recovered.errors.is_empty(),
                    "{name}: {:?}",
                    recovered.errors
                );
                assert_eq!(recovered.source, tree, "{name}");
            }
            Err(err) => {
                let first = err.syntax_errors().next().expect("syntax error");
                assert_eq!(recovered.errors[0].message, first.message, "{name}");
                assert_eq!(recovered.errors[0].span, first.span, "{name}");
            }
        }
    }
}

/// Items that parse on their own, one per line or block.
const VALID: &[&str] = &[
    "a = 1",
    "b = \"text\"",
    "c = [1, 2, 3]",
    "d = { x: 1, y: 2 }",
    "e = f(1, 2)",
    "svc web {\n  port = 8080\n  host = \"h\"\n}",
    "outer {\n  inner {\n    v = true\n  }\n}",
    "type T {\n  name: utf8\n}",
    "let k = 3",
    "tags = [\n  \"x\",\n  \"y\",\n]",
];

/// Lines that each hold exactly one syntax error.
const BROKEN: &[&str] = &[
    "bad = = 1",
    "bad = [1, 2 3]",
    "bad = f(1,\n  2 3,\n  4)",
    "bad = \"unterminated",
    "bad = 1 ~ 2",
    "type Bad {\n  name utf8\n  other: i64\n}",
    "@deco(\n  a = 1\n  b = = 2\n)",
    "bad = [\n  one\n  two three\n]",
    "}",
    "= 3",
];

/// Render items back to source, for comparing trees whose spans differ.
fn printed(items: Vec<Item>) -> String {
    format::to_source(&Source {
        items,
        trailing_trivia: Vec::new(),
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// A file of valid items parses the same both ways.
    #[test]
    fn valid_files_parse_the_same_both_ways(
        picks in proptest::collection::vec(0..VALID.len(), 0..8),
    ) {
        let src = unique(&picks).join("\n");
        let strict = parse_for_edit(&src, "p.wcl").expect("valid items parse");
        let recovered = parse_for_edit_recovering(&src, "p.wcl");
        prop_assert!(recovered.errors.is_empty());
        prop_assert_eq!(recovered.source, strict);
    }

    /// One broken item among valid ones costs one error and nothing
    /// else: every valid item survives.
    #[test]
    fn one_broken_item_is_one_error(
        picks in proptest::collection::vec(0..VALID.len(), 1..8),
        broken in 0..BROKEN.len(),
        at in 0usize..8,
    ) {
        let valid = unique(&picks);
        let at = at.min(valid.len());
        let mut lines = valid.clone();
        lines.insert(at, BROKEN[broken].to_string());
        let src = lines.join("\n");
        let recovered = parse_for_edit_recovering(&src, "p.wcl");
        prop_assert_eq!(recovered.errors.len(), 1, "{}\n{:?}", src, recovered.errors);
        let expected = parse_for_edit(&valid.join("\n"), "p.wcl").expect("valid items parse");
        prop_assert_eq!(printed(recovered.source.items), printed(expected.items), "{}", src);
    }
}

/// The picked valid items, renamed so no name is declared twice.
fn unique(picks: &[usize]) -> Vec<String> {
    picks
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let item = VALID[p];
            let (head, rest) = item.split_at(item.find(' ').unwrap_or(0));
            match head {
                "svc" | "outer" => item.to_string(),
                "type" => item.replacen(" T ", &format!(" T{i} "), 1),
                "let" => item.replacen("let k", &format!("let k{i}"), 1),
                _ => format!("{head}{i}{rest}"),
            }
        })
        .collect()
}
