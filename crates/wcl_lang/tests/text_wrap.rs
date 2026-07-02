//! Prose wrapping of long `p`-block labels into heredocs, and the
//! value-preserving heredoc form for authored multi-line labels.

use wcl_lang::format::{FormatConfig, to_source, to_source_with};
use wcl_lang::parse_for_edit;

fn fmt(src: &str) -> String {
    let ast = parse_for_edit(src, "test").expect("parse ok");
    to_source(&ast)
}

const LONG: &str = "This paragraph is deliberately much longer than the default wrap width so the formatter converts it to a heredoc and wraps the prose across several readable lines.";

#[test]
fn long_p_label_wraps_into_a_heredoc() {
    let out = fmt(&format!("page t {{\n  p \"{LONG}\"\n}}\n"));
    assert!(out.contains("p <<"), "{out}");
    // Every emitted line stays within a sane width.
    for line in out.lines() {
        assert!(line.chars().count() <= 100, "over-long line: {line}");
    }
    // The wrapped output re-parses and is idempotent.
    let again = fmt(&out);
    assert_eq!(out, again, "wrap must be idempotent");
}

#[test]
fn short_p_label_stays_quoted() {
    let out = fmt("page t {\n  p \"Short and sweet.\"\n}\n");
    assert!(out.contains("p \"Short and sweet.\""), "{out}");
}

#[test]
fn wrap_never_splits_inline_markup() {
    let src = format!(
        "page t {{\n  p \"{LONG} And it has **a rather long bold span in the middle of it** plus `an unbreakable code span here` and [a long link label](https://example.com/deep/path) to protect.\"\n}}\n"
    );
    let out = fmt(&src);
    assert!(out.contains("p <<"), "{out}");
    // Each construct survives on a single line of the wrapped body.
    let joined: Vec<&str> = out.lines().collect();
    for needle in [
        "**a rather long bold span in the middle of it**",
        "`an unbreakable code span here`",
        "[a long link label](https://example.com/deep/path)",
    ] {
        assert!(
            joined.iter().any(|l| l.contains(needle)),
            "{needle} was split:\n{out}"
        );
    }
}

#[test]
fn authored_heredoc_label_keeps_its_form() {
    let src = "page t {\n  p <<'TXT'\nline one\nline two\nTXT\n}\n";
    let out = fmt(src);
    assert!(out.contains("p <<"), "heredoc label degraded: {out}");
    assert!(!out.contains("\\n"), "heredoc label degraded: {out}");
    // Value-preserving: re-parse both and compare the label text.
    let a = parse_for_edit(src, "a").expect("parse a");
    let b = parse_for_edit(&out, "b").expect("parse b");
    assert_eq!(
        wcl_lang::format::to_source(&a).replace(char::is_whitespace, ""),
        wcl_lang::format::to_source(&b).replace(char::is_whitespace, "")
    );
}

#[test]
fn p_with_a_body_stays_quoted() {
    let out = fmt(&format!(
        "page t {{\n  p \"{LONG}\" {{ class = [\"x\"] }}\n}}\n"
    ));
    assert!(!out.contains("p <<"), "{out}");
}

#[test]
fn non_prose_kinds_never_wrap() {
    let out = fmt(&format!("page t {{\n  h1 \"{LONG}\"\n}}\n"));
    assert!(!out.contains("h1 <<"), "{out}");
}

#[test]
fn wrap_width_is_configurable() {
    let cfg = FormatConfig {
        text_wrap_width: 40,
        ..Default::default()
    };
    let ast = parse_for_edit(
        "page t {\n  p \"Five short words repeated over and over again to cross forty columns.\"\n}\n",
        "test",
    )
    .expect("parse ok");
    let out = to_source_with(&ast, &cfg);
    assert!(out.contains("p <<"), "{out}");
}
