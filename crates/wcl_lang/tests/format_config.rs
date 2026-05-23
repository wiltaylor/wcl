//! Exercise `FormatConfig` knobs: indent width, trailing-comma
//! toggle, blank-line cap. Default-config round-trip is already
//! covered by `parse.rs::round_trip_all_examples`.

use wcl_lang::format::{FormatConfig, to_source, to_source_with};
use wcl_lang::parse_for_edit;

fn render(src: &str, cfg: &FormatConfig) -> String {
    let ast = parse_for_edit(src, "test").expect("parse ok");
    to_source_with(&ast, cfg)
}

#[test]
fn indent_widens_to_four_spaces() {
    let src = "type Foo {\n  a: utf8\n  b: utf8\n}\n";
    let cfg = FormatConfig {
        indent: 4,
        ..Default::default()
    };
    let out = render(src, &cfg);
    assert!(out.contains("\n    a: utf8\n"), "{out}");
    assert!(out.contains("\n    b: utf8\n"), "{out}");
    // Sanity: not still emitting two-space indents at depth 1.
    assert!(!out.contains("\n  a: utf8\n"), "{out}");
}

#[test]
fn trailing_comma_in_match_can_be_disabled() {
    let src = "f = match 1 {\n  1 => true,\n  _ => false,\n}\n";
    let cfg = FormatConfig {
        trailing_comma_in_match: false,
        ..Default::default()
    };
    let out = render(src, &cfg);
    // The final arm before `}` should now lack a trailing comma.
    assert!(out.contains("=> false\n"), "{out}");
    assert!(!out.contains("=> false,"), "{out}");
}

#[test]
fn blank_line_cap_zero_collapses_blank_lines() {
    let src = "a = 1\n\nb = 2\n";
    let cfg = FormatConfig {
        blank_line_cap: 0,
        ..Default::default()
    };
    let out = render(src, &cfg);
    assert!(
        !out.contains("\n\n"),
        "expected no blank line, got: {out:?}"
    );
}

#[test]
fn default_config_matches_to_source() {
    let src = "a = 1\n\nb = 2\n";
    let ast = parse_for_edit(src, "test").expect("parse ok");
    assert_eq!(
        to_source(&ast),
        to_source_with(&ast, &FormatConfig::default())
    );
}
