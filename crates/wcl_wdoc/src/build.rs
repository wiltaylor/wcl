use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Document, Value};

use crate::render::{render_block, render_class, render_page};

const SCHEMA: &str = include_str!("../wdoc.wcl");

pub enum BuildError {
    Io(std::io::Error, String),
    Parse(Report),
    Schema(usize),
    BadPage(String),
}

impl BuildError {
    pub fn report(&self) {
        match self {
            Self::Io(e, ctx) => eprintln!("{ctx}: {e}"),
            Self::Parse(r) => eprintln!("{r:?}"),
            Self::Schema(n) => eprintln!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::BadPage(msg) => eprintln!("{msg}"),
        }
    }
}

pub fn build(file: &Path, out_dir: &Path) -> Result<usize, BuildError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;

    // Stitch the schema in front of the user source. Diagnostics
    // referencing user lines/columns stay correct as long as we never
    // touch the user portion — the schema lives at the top.
    let composed = format!("{SCHEMA}\n{user_src}");
    let name = file.display().to_string();

    let doc = Document::open(&composed, &name).map_err(|e| BuildError::Parse(Report::new(e)))?;

    let errs = doc.schema_errors();
    if !errs.is_empty() {
        let n = errs.len();
        let src = NamedSource::new(name.clone(), composed.clone());
        for e in &errs {
            let report = Report::new(e.clone()).with_source_code(src.clone());
            eprintln!("{report:?}");
        }
        return Err(BuildError::Schema(n));
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", out_dir.display())))?;

    // Document-global stylesheet: every @block("class") becomes one
    // `.name { ... }` rule. Emitted into <head> on every page.
    let css: String = doc
        .blocks()
        .filter(|b| b.kind() == "class")
        .filter_map(|b| render_class(&b))
        .collect::<Vec<_>>()
        .join("\n");

    let mut count = 0;
    for page in doc.blocks().filter(|b| b.kind() == "page") {
        let labels = page
            .labels()
            .map_err(|e| BuildError::BadPage(format!("page label eval: {e}")))?;
        let page_name = match labels.into_iter().next() {
            Some(Value::Identifier(s)) | Some(Value::Utf8(s)) | Some(Value::Symbol(s)) => s,
            Some(other) => {
                return Err(BuildError::BadPage(format!(
                    "expected identifier page name, got {other}"
                )));
            }
            None => return Err(BuildError::BadPage("page has no name label".into())),
        };

        let rendered_blocks = page.blocks().filter_map(|b| render_block(&doc, &b));
        let html = render_page(&page_name, &css, rendered_blocks);

        let out_path = out_dir.join(format!("{page_name}.html"));
        fs::write(&out_path, html)
            .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
        count += 1;
    }

    Ok(count)
}
