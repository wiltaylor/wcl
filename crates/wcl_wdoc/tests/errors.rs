//! The public error types: each is a `std::error::Error` a host can box,
//! and each variant displays the message the CLI prints for it.

use std::error::Error as _;

use miette::{Diagnostic, NamedSource, Report};
use wcl_wdoc::content::{At, ContentError};
use wcl_wdoc::{BuildError, PdfError};

/// Compile-time: every public error can cross threads and be boxed as a
/// `dyn Error`, and the build and pdf errors render through miette.
#[test]
fn public_errors_are_send_sync_static_errors() {
    fn error<T: std::error::Error + Send + Sync + 'static>() {}
    fn diagnostic<T: Diagnostic + Send + Sync + 'static>() {}
    error::<BuildError>();
    error::<PdfError>();
    error::<ContentError>();
    error::<wcl_lang::LexError>();
    error::<wcl_lang::edit::EditError>();
    diagnostic::<BuildError>();
    diagnostic::<PdfError>();
    diagnostic::<wcl_lang::LexError>();
    diagnostic::<wcl_lang::edit::EditError>();
}

fn not_found() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, "no such file")
}

/// A miette report carrying a labelled snippet, the shape `Parse` and
/// `Eval` hold.
fn snippet_report() -> Report {
    #[derive(Debug, thiserror::Error, Diagnostic)]
    #[error("unexpected token")]
    #[diagnostic(code(wcl::parse))]
    struct Snippet {
        #[source_code]
        src: NamedSource<String>,
        #[label("here")]
        span: (usize, usize),
    }
    Report::new(Snippet {
        src: NamedSource::new("doc.wcl", "page index { = }\n".to_string()),
        span: (13, 1),
    })
}

#[test]
fn build_error_displays_each_variant() {
    let cases: Vec<(BuildError, &str)> = vec![
        (
            BuildError::Io(not_found(), "read doc.wcl".into()),
            "read doc.wcl: no such file",
        ),
        (BuildError::Parse(snippet_report()), "unexpected token"),
        (BuildError::Schema(1), "1 schema violation"),
        (BuildError::Schema(3), "3 schema violations"),
        (BuildError::Eval(snippet_report()), "unexpected token"),
        (
            BuildError::BadPage("page has no name".into()),
            "page has no name",
        ),
        (
            BuildError::DuplicateId {
                page: "index".into(),
                id: "intro".into(),
            },
            "page \"index\": duplicate id \"intro\"",
        ),
        (
            BuildError::DuplicatePage {
                site: "docs".into(),
                name: "cont_x".into(),
            },
            "site \"docs\": duplicate page \"cont_x\"",
        ),
        (
            BuildError::BadLink(vec![
                "link to unknown page 'nope'".into(),
                "link to unknown page 'gone'".into(),
            ]),
            "link to unknown page 'nope'\nlink to unknown page 'gone'",
        ),
        // The build passes whole sentences; `Display` must not wrap them in
        // `unknown template "…"` a second time.
        (
            BuildError::BadTemplate("menu item links to unknown page \"nope\"".into()),
            "menu item links to unknown page \"nope\"",
        ),
        (
            BuildError::Tileset("tileset missing".into()),
            "tileset missing",
        ),
        (
            BuildError::EdgeRouting("edge a -> b cannot be routed".into()),
            "edge a -> b cannot be routed",
        ),
        (
            BuildError::CodeInclude("gone.rs: not found".into()),
            "gone.rs: not found",
        ),
    ];
    for (err, want) in cases {
        assert_eq!(err.to_string(), want, "{err:?}");
    }
}

#[test]
fn pdf_error_displays_each_variant() {
    let cases: Vec<(PdfError, &str)> = vec![
        (
            PdfError::Io(not_found(), "write out.pdf".into()),
            "write out.pdf: no such file",
        ),
        (PdfError::Parse(snippet_report()), "unexpected token"),
        (PdfError::Schema(1), "1 schema violation"),
        (PdfError::Schema(2), "2 schema violations"),
        (PdfError::Eval(snippet_report()), "unexpected token"),
        (PdfError::BadDoc("no pages".into()), "no pages"),
        (
            PdfError::Render("font missing".into()),
            "pdf render failed: font missing",
        ),
    ];
    for (err, want) in cases {
        assert_eq!(err.to_string(), want, "{err:?}");
    }
}

#[test]
fn an_io_error_is_the_source_of_the_io_variant() {
    let err = BuildError::Io(not_found(), "read doc.wcl".into());
    let source = err.source().expect("Io has a source");
    assert_eq!(source.to_string(), "no such file");
    assert!(source.downcast_ref::<std::io::Error>().is_some());

    let err = PdfError::Io(not_found(), "write out.pdf".into());
    assert_eq!(
        err.source().expect("Io has a source").to_string(),
        "no such file"
    );

    assert!(BuildError::BadPage("x".into()).source().is_none());
}

#[test]
fn a_report_variant_is_diagnostic_transparent() {
    let err = BuildError::Parse(snippet_report());
    assert_eq!(
        err.code().map(|c| c.to_string()).as_deref(),
        Some("wcl::parse")
    );
    assert!(err.labels().is_some_and(|mut l| l.next().is_some()));
    let plain = err.render_plain();
    assert!(plain.contains("doc.wcl"), "{plain}");
    assert!(plain.contains("here"), "{plain}");
    assert!(!plain.contains('\u{1b}'), "no escapes: {plain}");

    let plain = PdfError::Eval(snippet_report()).render_plain();
    assert!(plain.contains("doc.wcl"), "{plain}");
}

#[test]
fn render_plain_keeps_every_bad_link() {
    let plain = BuildError::BadLink(vec![
        "link to unknown page 'nope'".into(),
        "link to unknown page 'gone'".into(),
    ])
    .render_plain();
    assert!(plain.contains("'nope'"), "{plain}");
    assert!(plain.contains("'gone'"), "{plain}");
}

#[test]
fn content_error_displays_each_variant() {
    let at = At {
        owner: "Content::Heading",
        field: "level",
    };
    let cases = [
        (
            ContentError::NotAVariant { owner: "Content" },
            "expected a `Content` variant value",
        ),
        (
            ContentError::NotARecord {
                owner: "ContentTocEntry",
            },
            "expected a `ContentTocEntry` record value",
        ),
        (
            ContentError::NotASymbol { owner: "Align" },
            "expected a `Align` symbol",
        ),
        (
            ContentError::UnknownVariant {
                owner: "Content",
                variant: "Bogus".into(),
            },
            "`Content` declares no variant `Bogus`",
        ),
        (
            ContentError::UnknownSymbol {
                owner: "Align",
                symbol: "up".into(),
            },
            "`Align` declares no symbol `:up`",
        ),
        (
            ContentError::MissingField { at },
            "`Content::Heading` is missing required field `level`",
        ),
        (
            ContentError::FieldType { at, expected: "u8" },
            "`Content::Heading`'s field `level` is not a u8",
        ),
    ];
    for (err, want) in cases {
        assert_eq!(err.to_string(), want);
    }
}

/// Build `body` (after `import <wdoc.wcl>`) and return the failure.
fn build_err(body: &str) -> BuildError {
    let tmp = tempfile::TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(&src, format!("import <wdoc.wcl>\n{body}")).expect("write fixture");
    let out = tmp.path().join("out");
    match wcl_wdoc::build(&src, &out, None) {
        Ok(n) => panic!("expected a build failure, built {n} pages"),
        Err(err) => err,
    }
}

#[test]
fn an_unknown_template_names_it_once() {
    let err = build_err("page index { template = :nosuch  h1 \"x\" }\n");
    assert!(matches!(err, BuildError::BadTemplate(_)), "{err:?}");
    assert_eq!(err.to_string(), "unknown template \"nosuch\"");
}

#[test]
fn a_template_sentence_is_not_wrapped_as_a_template_name() {
    let err = build_err(
        r#"
site docbook {
  default_template = :book
  title = "Catalog"
  toc { chapter "Home" { page = index } }
  sidebar_footer { button "Reference" { page = nope } }
}
page index { sites = [:docbook]  start = true  h1 "Home" }
"#,
    );
    assert!(matches!(err, BuildError::BadTemplate(_)), "{err:?}");
    assert_eq!(
        err.to_string(),
        "sidebar_footer button links to unknown page \"nope\""
    );
}
