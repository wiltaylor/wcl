//! Formatter round-trip regressions (BUG-fmt-heredoc-roundtrip):
//! `parse → format → parse` must always succeed, and a second format
//! pass must reproduce the first (idempotence) — `wcl fmt` quietly
//! breaking a parsing file is the worst failure mode a formatter has.

use wcl_lang::format::to_source;
use wcl_lang::parse_for_edit;

/// Format `src`, assert the output re-parses, and assert a second
/// format pass is a fixpoint. Returns the formatted text.
fn round_trip(src: &str) -> String {
    let ast1 = parse_for_edit(src, "test").expect("input parses");
    let out1 = to_source(&ast1);
    let ast2 = parse_for_edit(&out1, "test")
        .unwrap_or_else(|e| panic!("formatter output fails to re-parse:\n{out1}\n--\n{e:?}"));
    let out2 = to_source(&ast2);
    assert_eq!(out1, out2, "formatter is not idempotent");
    out1
}

#[test]
fn newline_escape_string_stays_quoted() {
    // Variant 2 of the report: a `"\n"` separator inside a call used to
    // become a whitespace heredoc whose closer glued onto the `)` —
    // unparseable. Heredocs are never legal inside call arguments.
    let out = round_trip("@schemaless x = join([\"a\", \"b\"], \"\\n\")\n");
    assert!(out.contains("\"\\n\""), "separator stays escaped:\n{out}");
    assert!(!out.contains("<<"), "no heredoc in a call argument:\n{out}");
}

#[test]
fn interpolated_string_without_trailing_newline_stays_quoted() {
    // Variant 1: a multi-line `$"…\n…"` value not ending in a newline is
    // unrepresentable as a heredoc (the closing tag would glue onto the
    // last content line). It must stay an escaped quoted literal.
    let out =
        round_trip("@schemaless body = $\"first ${1 + 1}\\n\\n**Goals**\\nlast ${2 + 2}.\"\n");
    assert!(!out.contains("<<"), "stays quoted:\n{out}");
    assert!(out.contains("\\n"), "escapes preserved:\n{out}");
}

#[test]
fn heredoc_authored_strings_keep_heredoc_form() {
    let out = round_trip("x = <<DOC\n  line one\n  line two\nDOC\n");
    assert!(out.contains("<<DOC"), "plain heredoc preserved:\n{out}");
    let out = round_trip("@schemaless y = $<<INTERP\nfirst ${1 + 1}\nsecond\nINTERP\n");
    assert!(
        out.contains("<<INTERP"),
        "interpolated heredoc preserved:\n{out}"
    );
}

#[test]
fn safe_multiline_string_converts_to_heredoc() {
    // The conversion still happens where it pays off and is safe: a
    // statement-level value with >= 2 lines ending in a newline.
    let out = round_trip("@schemaless x = \"line one\\nline two\\nline three\\n\"\n");
    assert!(out.contains("<<DOC"), "converted to heredoc:\n{out}");
}

#[test]
fn heredoc_unsafe_bodies_stay_quoted() {
    // Uniform leading whitespace would be stripped by heredoc indent
    // handling; a whitespace-only line would be blanked. Both must stay
    // escaped literals to preserve the value.
    let out = round_trip("@schemaless a = \"  one\\n  two\\n\"\n");
    assert!(!out.contains("<<"), "indented body stays quoted:\n{out}");
    let out = round_trip("@schemaless b = \"one\\n  \\ntwo\\n\"\n");
    assert!(!out.contains("<<"), "ws-only line stays quoted:\n{out}");
}

#[test]
fn heredoc_not_emitted_before_trailing_comment() {
    // A trailing comment would share the closer's line (`DOC  # x`),
    // which the lexer rejects — the value stays quoted.
    let out = round_trip("@schemaless x = \"one\\ntwo\\n\"  # comment\n");
    assert!(!out.contains("<<"), "stays quoted before comment:\n{out}");
    assert!(out.contains("# comment"), "comment preserved:\n{out}");
}

#[test]
fn escaped_interpolation_slot_re_escapes() {
    // A literal `${…}` (authored as `\${…}`) must re-escape on output —
    // in both the quoted and heredoc interpolated forms — or it
    // re-parses as a real slot.
    let out = round_trip("@schemaless a = $\"literal \\${not_a_slot} and ${1 + 1}\"\n");
    assert!(out.contains("\\${not_a_slot}"), "re-escaped:\n{out}");
    let out =
        round_trip("@schemaless b = $<<INTERP\nkeep \\${this} text ${1 + 1}\nsecond\nINTERP\n");
    assert!(out.contains("\\${this}"), "re-escaped in heredoc:\n{out}");
}

#[test]
fn exponent_floats_round_trip() {
    // Debug prints `2e-6` (no dot), which the lexer rejects — the
    // formatter splices the dot back in.
    let out = round_trip("@schemaless x = 2.0e-6\n@schemaless y = 1.5e10\n");
    assert!(out.contains("2.0e-6") || out.contains("2.0E-6"), "{out}");
}

#[test]
fn blank_only_files_format_to_empty() {
    let out = round_trip("\n\n");
    assert_eq!(out, "", "no leading blank lines survive:\n{out:?}");
}

#[test]
fn deeply_nested_input_errors_instead_of_overflowing_the_stack() {
    // Regression (BUG-parser-recursion-overflow): kilobytes of `(((((`
    // used to abort the process with a stack overflow. The recursive
    // parse paths are depth-capped with a spanned diagnostic.
    let src = format!("x = {}1{}\n", "(".repeat(4000), ")".repeat(4000));
    let err = parse_for_edit(&src, "deep").expect_err("depth cap fires");
    assert!(
        format!("{err}").contains("nesting too deep"),
        "expected nesting diagnostic, got: {err}"
    );
    // Deep but legal nesting still parses.
    let ok = format!("x = {}1{}\n", "(".repeat(100), ")".repeat(100));
    parse_for_edit(&ok, "ok").expect("100 levels parse fine");
}
