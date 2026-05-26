use std::collections::HashSet;
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Environment, Value};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{
    field_id, field_symbol, field_utf8, find_template, render_block, render_class, render_page,
    render_template,
};

const SCHEMA: &str = include_str!("../wdoc.wcl");

pub enum BuildError {
    Io(std::io::Error, String),
    Parse(Report),
    Schema(usize),
    BadPage(String),
    DuplicateId { page: String, id: String },
    BadLink(Vec<String>),
    BadTemplate(String),
}

impl BuildError {
    pub fn report(&self) {
        match self {
            Self::Io(e, ctx) => eprintln!("{ctx}: {e}"),
            Self::Parse(r) => eprintln!("{r:?}"),
            Self::Schema(n) => eprintln!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::BadPage(msg) => eprintln!("{msg}"),
            Self::DuplicateId { page, id } => {
                eprintln!("page \"{page}\": duplicate id \"{id}\"");
            }
            Self::BadLink(msgs) => {
                for m in msgs {
                    eprintln!("{m}");
                }
            }
            Self::BadTemplate(name) => eprintln!("unknown template \"{name}\""),
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

    // Relative `import "./pages/foo.wcl"` statements inside the user
    // source must resolve against the source file's own directory,
    // not the wdoc working directory. Pass it through to open_at.
    let base_dir = file.parent().map(std::path::Path::to_path_buf);
    let doc = Document::open_at(&composed, &name, base_dir, &Environment::new())
        .map_err(|e| BuildError::Parse(Report::new(e)))?;

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

    // Document-global stylesheet: bundled code-block theme + every
    // @block("class") rule. Emitted into <head> on every page. The
    // theme comes first so user-declared classes can override it.
    let class_css: String = doc
        .blocks()
        .filter(|b| b.kind() == "class")
        .filter_map(|b| render_class(&b))
        .collect::<Vec<_>>()
        .join("\n");
    let css = format!(
        "{}\n{}\n{}\n{class_css}",
        highlight::theme_css(),
        crate::render::TABLE_CSS,
        crate::render::SITE_CSS
    );

    // Document descriptor (`site` block): the default template and the
    // site title a template can show. Optional — absent ⇒ pages render
    // bare unless they set their own `template`.
    let site = doc.blocks().find(|b| b.kind() == "site");
    let default_template = site
        .as_ref()
        .and_then(|b| field_symbol(b, "default_template"));
    let site_title = site.as_ref().and_then(|b| field_utf8(b, "title"));

    // Ordered (name, href) list of every page, handed to templates so
    // they can build navigation themselves.
    let pages: Vec<(String, String)> = doc
        .blocks()
        .filter(|b| b.kind() == "page")
        .filter_map(|p| page_name(&p).map(|n| (n.clone(), format!("{n}.html"))))
        .collect();

    // Page-name set used by the inline link pattern to recognise
    // `[text](page)` cross-page references. Built before rendering
    // so a link from `index` to `about` resolves regardless of
    // source order.
    let mut page_names: HashSet<String> = HashSet::new();
    for page in doc.blocks().filter(|b| b.kind() == "page") {
        if let Some(name) = page_name(&page) {
            page_names.insert(name);
        }
    }

    // Document-global inline-text pattern engine, compiled once
    // per build: every `@block("inline_pattern")` (built-in or
    // user-declared) contributes one regex + `to_span` function.
    let inline_patterns = InlinePatterns::load(&doc, page_names);

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

        let mut seen = HashSet::new();
        if let Some(dup) = collect_duplicate_id(&page, &mut seen) {
            return Err(BuildError::DuplicateId {
                page: page_name,
                id: dup,
            });
        }

        // The `content` part: this page's own blocks, rendered exactly
        // as before (trailing newline per block).
        let mut content = String::new();
        for b in page
            .blocks()
            .filter_map(|b| render_block(&doc, &b, &inline_patterns))
        {
            content.push_str(&b);
            content.push('\n');
        }

        // Resolve the template: the page's own `template` overrides the
        // document `default_template`. None ⇒ render content bare.
        let template_name = field_symbol(&page, "template").or_else(|| default_template.clone());
        let body = match template_name {
            Some(name) => {
                let Some(tmpl) = find_template(&doc, &name) else {
                    return Err(BuildError::BadTemplate(name));
                };
                let title = site_title.clone().unwrap_or_else(|| page_name.clone());
                render_template(&doc, &tmpl, &content, &title, &page_name, &pages)
            }
            None => content,
        };
        let html = render_page(&page_name, &css, &body);

        let out_path = out_dir.join(format!("{page_name}.html"));
        fs::write(&out_path, html)
            .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
        count += 1;
    }

    // Inline `[text](page)` references that didn't resolve to a
    // known page block surface as a build error here, after every
    // page has had a chance to render and report.
    let link_errors = inline_patterns.take_link_errors();
    if !link_errors.is_empty() {
        return Err(BuildError::BadLink(link_errors));
    }

    Ok(count)
}

/// Extract a page block's first label as a string identifier. The
/// page-name match for `[text](page)` cross-page links runs against
/// this set.
fn page_name(page: &Block<'_>) -> Option<String> {
    let labels = page.labels().ok()?;
    match labels.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

/// Walk a page's block tree collecting `id` values. Returns the first
/// duplicate encountered, or `None` if all ids are unique. Used to
/// enforce per-page id uniqueness so emitted HTML stays valid.
fn collect_duplicate_id(block: &Block<'_>, seen: &mut HashSet<String>) -> Option<String> {
    if let Some(id) = field_id(block, "id")
        && !seen.insert(id.clone())
    {
        return Some(id);
    }
    for child in block.blocks() {
        if let Some(dup) = collect_duplicate_id(&child, seen) {
            return Some(dup);
        }
    }
    None
}
