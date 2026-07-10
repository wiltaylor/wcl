//! `wcl fmt` comment preservation (GitHub issue #12). Comments are
//! pulled into the AST as leading/trailing trivia on each node, so the
//! source printer re-emits them in place across a round-trip. Each test
//! also asserts idempotency: `fmt(fmt(src)) == fmt(src)`.

use wcl_lang::format::to_source;
use wcl_lang::parse_for_edit;

fn fmt(src: &str) -> String {
    let ast = parse_for_edit(src, "test").expect("parse ok");
    to_source(&ast)
}

/// Format once, then format the result again; assert both that the
/// output contains `needle` and that the printer is idempotent.
fn fmt_stable(src: &str, needle: &str) -> String {
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice, "printer not idempotent\n--- once ---\n{once}");
    assert!(once.contains(needle), "missing {needle:?} in:\n{once}");
    once
}

#[test]
fn eof_trailing_comment_survives() {
    // No trailing newline, comment is the last thing in the file.
    let out = fmt_stable("a = 1  # tail", "# tail");
    assert!(out.contains("a = 1  # tail"), "{out}");
}

#[test]
fn comment_before_closing_brace_survives() {
    let src = "type T {\n  a: i64\n  # tail note\n}\n";
    let out = fmt_stable(src, "# tail note");
    // The comment stays inside the body, before `}`.
    let note = out.find("# tail note").unwrap();
    let brace = out.rfind('}').unwrap();
    assert!(note < brace, "comment should precede closing brace:\n{out}");
}

#[test]
fn inline_field_comment_stays_inline() {
    let out = fmt_stable("name = \"app\"  # the service\n", "# the service");
    assert!(out.contains("name = \"app\"  # the service"), "{out}");
}

#[test]
fn inline_type_field_comment_stays_on_field() {
    let src = "type T {\n  host: string  # bind host\n  port: i32\n}\n";
    let out = fmt_stable(src, "# bind host");
    assert!(out.contains("host: string  # bind host"), "{out}");
}

#[test]
fn leading_comments_inside_bodies_survive() {
    let src = concat!(
        "type T {\n  # a field\n  a: i64\n}\n",
        "union U {\n  # a variant\n  V none\n}\n",
        "symbol_set S {\n  # a symbol\n  one\n}\n",
    );
    let out = fmt_stable(src, "# a field");
    assert!(out.contains("# a variant"), "{out}");
    assert!(out.contains("# a symbol"), "{out}");
}

#[test]
fn open_brace_line_comment_becomes_first_member_leading() {
    // A comment on the open-brace line has no previous sibling, so it
    // falls through to the first member's leading trivia.
    let src = "union U {  # heads up\n  A none\n  B none\n}\n";
    let out = fmt_stable(src, "# heads up");
    let comment = out.find("# heads up").unwrap();
    let first = out.find("A none").unwrap();
    assert!(
        comment < first,
        "comment should lead the first variant:\n{out}"
    );
}

#[test]
fn block_expression_comments_survive() {
    let src = "x = {\n  # bind a\n  let a = 1;  # the one\n  a\n}\n";
    let out = fmt_stable(src, "# bind a");
    assert!(out.contains("let a = 1;  # the one"), "{out}");
}

#[test]
fn match_arm_comments_survive() {
    let src = concat!(
        "x = match 1 {\n",
        "  # the one case\n",
        "  1 => true,  # matched one\n",
        "  _ => false,\n",
        "}\n",
    );
    let out = fmt_stable(src, "# the one case");
    assert!(out.contains("# matched one"), "{out}");
}

#[test]
fn list_element_comments_force_multiline() {
    // A list that fits on one line is broken out so comments have a home.
    let src = "values = [1, 2, 3]\n";
    assert_eq!(
        fmt(src),
        "values = [1, 2, 3]\n",
        "uncommented list stays inline"
    );

    let commented = "values = [\n  1,  # one\n  2,  # two\n]\n";
    let out = fmt_stable(commented, "# one");
    assert!(out.contains("1,  # one"), "{out}");
    assert!(out.contains("2,  # two"), "{out}");
}

#[test]
fn record_field_comments_force_multiline() {
    let src = "p = { x: 1, y: 2 }\n";
    assert_eq!(
        fmt(src),
        "p = { x: 1, y: 2 }\n",
        "uncommented record stays inline"
    );

    let commented = "p = {\n  x: 1,  # across\n  y: 2,\n}\n";
    let out = fmt_stable(commented, "# across");
    assert!(out.contains("x: 1,  # across"), "{out}");
}

#[test]
fn fn_parameter_comments_survive() {
    let src = "f = fn(\n  a: i32,  # first\n  b: i32,\n) -> i32 a\n";
    let out = fmt_stable(src, "# first");
    assert!(out.contains("a: i32,  # first"), "{out}");
}

#[test]
fn call_argument_comments_survive() {
    let src = "y = add(\n  1,  # lhs\n  2,  # rhs\n)\n";
    let out = fmt_stable(src, "# lhs");
    assert!(out.contains("1,  # lhs"), "{out}");
    assert!(out.contains("2,  # rhs"), "{out}");
}

#[test]
fn file_start_comment_stays_leading_not_trailing() {
    // A comment at the very top has no previous token: it must stay a
    // leading comment, never become a trailing comment of nothing.
    let out = fmt_stable("# header\na = 1\n", "# header");
    assert!(out.starts_with("# header\n"), "{out}");
}

#[test]
fn table_trailing_comment_lands_after_last_row() {
    // A table is multi-line, so an inline comment that follows it
    // attaches after the *last row* (where it round-trips) rather than
    // on the `field:` header line (where it would reflow on reparse).
    let src = "rows:\n  | 1 |\n  | 2 |  # last\n";
    let out = fmt_stable(src, "# last");
    let comment = out.find("# last").unwrap();
    let header = out.find("rows:").unwrap();
    assert!(
        comment > header,
        "comment must not sit on the header:\n{out}"
    );
}

#[test]
fn comment_before_top_level_block_survives() {
    // Regression: the block construction sites drained the shared
    // item-trivia slot *after* parsing the body, so every nested
    // `parse_item` had already clobbered the block's own leading trivia.
    let out = fmt_stable(
        "# the demo lab\nlab \"demo\" {\n  a = 1\n}\n",
        "# the demo lab",
    );
    let comment = out.find("# the demo lab").unwrap();
    let block = out.find("lab \"demo\"").unwrap();
    assert!(comment < block, "comment must precede the block:\n{out}");
}

#[test]
fn comment_before_nested_block_survives() {
    let src = "lab \"demo\" {\n  # the domain controller\n  vm \"dc01\" {\n    cpus = 4\n  }\n}\n";
    let out = fmt_stable(src, "# the domain controller");
    let comment = out.find("# the domain controller").unwrap();
    let vm = out.find("vm \"dc01\"").unwrap();
    assert!(
        comment < vm,
        "comment must precede the nested block:\n{out}"
    );
}

#[test]
fn blank_line_before_block_survives() {
    let src = "a = 1\n\nlab \"demo\" {\n  b = 2\n}\n";
    let out = fmt_stable(src, "lab \"demo\"");
    assert!(
        out.contains("a = 1\n\nlab"),
        "blank line should survive:\n{out}"
    );
}
