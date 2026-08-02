//! The `el` constructor family (`lib/el.wcl`) against the long form.
//!
//! Every constructor is meant to be *exactly* its `Html`
//! record with the field names dropped, which is what let the stdlib's
//! element construction sites be ported without moving a byte of output.
//! These tests build the same markup both ways in one document and
//! compare the rendered bodies, so a constructor that drifts from its
//! long form fails here rather than silently in someone's site.

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, build};

/// The two `lower`s below build the same tree — the left one with the
/// constructors, the right one with the records. Every member of the
/// family appears, and `id` / `class` are exercised both set and unset
/// (the `_none` blocks), because "unset renders as an omitted field" is
/// the property the ported sites lean on hardest.
const DOC: &str = r#"import <wdoc.wcl>

@block("el_form")
type ElForm extends ContentBlock {
  id: identifier?
  lower = fn(b: ElForm) -> list<Html> [
    eli("div", b.id, ["wrap", "outer"], [
      ela("a", ["link"], [["href", "x.html"], ["title", "T & U"]], [raw("<b>raw</b>")]),
      el("p", [], [inl("**bold** and `code`")]),
      el("span", none, []),
      ela("em", [], [], []),
      icon("lucide.info", ["ic"]),
      para(["cap"], ["one", "two"]),
    ]),
  ]
}

@block("long_form")
type LongForm extends ContentBlock {
  id: identifier?
  lower = fn(b: LongForm) -> list<Html> [
    Html::Element {
      tag: "div", id: b.id, class: ["wrap", "outer"],
      children: [
        Html::Element {
          tag: "a", class: ["link"],
          attrs: [["href", "x.html"], ["title", "T & U"]],
          children: [ Html::Raw { html: "<b>raw</b>" } ],
        },
        Html::Element {
          tag: "p",
          children: [ Html::Inline { text: "**bold** and `code`" } ],
        },
        Html::Element { tag: "span", children: [] },
        Html::Element { tag: "em", children: [] },
        Html::Icon { name: "lucide.info", class: ["ic"] },
        Html::Paragraph { class: ["cap"], spans: ["one", "two"] },
      ],
    },
  ]
}

page short { el_form { id = anchor } }
page long  { long_form { id = anchor } }
page short_none { el_form { } }
page long_none  { long_form { } }
"#;

fn build_ok(file: &Path, out: &Path) {
    match build(file, out, None) {
        Ok(_) => {}
        Err(BuildError::Schema(n)) => panic!("schema error: {n} violations"),
        Err(BuildError::Parse(r)) | Err(BuildError::Eval(r)) => panic!("{r:?}"),
        Err(_) => panic!("build failed"),
    }
}

/// Everything between the `<body …>` open tag and `</body>` — the page's
/// own lowered content, without the `<head>` (whose `<title>` names the
/// page and so differs between the two).
fn body_of(out: &Path, page: &str) -> String {
    let html = std::fs::read_to_string(out.join(format!("{page}.html")))
        .unwrap_or_else(|e| panic!("read {page}.html: {e}"));
    let start = html.find("<body").expect("page has a <body>");
    let open_end = html[start..].find('>').expect("body open tag closes") + start + 1;
    let end = html.find("</body>").expect("page closes its <body>");
    html[open_end..end].to_string()
}

fn build_fixture() -> TempDir {
    let dir = TempDir::new().expect("mkdir tempdir");
    let main = dir.path().join("main.wcl");
    std::fs::write(&main, DOC).expect("write fixture");
    let out = dir.path().join("_site");
    build_ok(&main, &out);
    dir
}

#[test]
fn constructors_render_exactly_their_long_form() {
    let dir = build_fixture();
    let out = dir.path().join("_site");
    let short = body_of(&out, "short");
    assert_eq!(
        short,
        body_of(&out, "long"),
        "constructor body != record body"
    );
    // Guard the comparison itself: an empty body on both sides would
    // pass the assert above while proving nothing.
    assert!(
        short.contains("<div class=\"wrap outer\" id=\"anchor\">"),
        "{short}"
    );
    assert!(
        short.contains("<a class=\"link\" href=\"x.html\" title=\"T &amp; U\">"),
        "{short}"
    );
    assert!(short.contains("<b>raw</b>"), "{short}"); // raw() is not escaped
    assert!(
        short.contains("<span class=\"bold\">bold</span>"),
        "{short}"
    ); // inl() ran the patterns
    assert!(short.contains("<svg class=\"wdoc-icon ic\">"), "{short}");
    assert!(
        short.contains("<p class=\"cap\"><span>one</span><span>two</span></p>"),
        "{short}"
    );
}

/// An unset `id` / `class` handed to `eli` / `el` must emit no attribute
/// at all — not `id=""` / `class=""`. The stdlib passes optional fields
/// straight through (`eli("p", p.id, p.class, …)`), so this is the
/// difference between a byte-identical port and a broken one.
#[test]
fn unset_id_and_class_emit_no_attribute() {
    let dir = build_fixture();
    let out = dir.path().join("_site");
    let short = body_of(&out, "short_none");
    assert_eq!(
        short,
        body_of(&out, "long_none"),
        "constructor body != record body with id/class unset"
    );
    assert!(short.contains("<div class=\"wrap outer\">"), "{short}");
    assert!(!short.contains("id=\"\""), "{short}");
    assert!(!short.contains("class=\"\""), "{short}");
    // `el("span", none, [])` and `ela("em", [], [], [])` — the unset and
    // the empty-list forms of both attribute-bearing parameters.
    assert!(short.contains("<span></span><em></em>"), "{short}");
}
