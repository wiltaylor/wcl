use std::collections::HashSet;
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Environment, Value};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{
    TocNode, field_bool, field_id, field_symbol, field_utf8, find_template, read_toc, render_block,
    render_class, render_page, render_template,
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
    Tileset(String),
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
            Self::Tileset(msg) => eprintln!("{msg}"),
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
    let doc = Document::open_at(&composed, &name, base_dir.clone(), &Environment::new())
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
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{class_css}",
        crate::render::BASE_CSS,
        crate::render::HEADING_CSS,
        highlight::theme_css(),
        crate::render::TABLE_CSS,
        crate::render::SITE_CSS,
        crate::render::BOOK_CSS,
        crate::render::CHART_CSS,
        crate::render::TERMINAL_CSS,
        crate::render::ICON_CSS,
        crate::render::CALLOUT_CSS,
        crate::render::TILEMAP_CSS,
    );

    // Terminals need the bundled font + replay player written alongside
    // the pages. Only emit them when the document actually uses a
    // terminal, so font-free sites pay nothing.
    let uses_terminals = doc.blocks().any(|b| crate::terminal::uses_terminal(&b));
    if uses_terminals {
        write_terminal_assets(out_dir)?;
    }

    // Document descriptor (`site` block): the default template and the
    // site title a template can show. Optional — absent ⇒ pages render
    // bare unless they set their own `template`.
    let site = doc.blocks().find(|b| b.kind() == "site");
    let default_template = site
        .as_ref()
        .and_then(|b| field_symbol(b, "default_template"));
    let site_title = site.as_ref().and_then(|b| field_utf8(b, "title"));
    let theme_toggle = site
        .as_ref()
        .and_then(|b| field_bool(b, "theme_toggle"))
        .unwrap_or(false);

    // Book table of contents (the `site` block's `toc`), shared by all
    // pages; the per-page `current` flag is applied at render time.
    // Empty when there's no `toc` (templates fall back to a flat list).
    let toc_nodes: Vec<TocNode> = site.as_ref().map(read_toc).unwrap_or_default();

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

    // A `toc` chapter that links to a page that doesn't exist is almost
    // always a typo — surface it as a build error rather than emitting a
    // dead link.
    if let Some(missing) = toc_missing_page(&toc_nodes, &page_names) {
        return Err(BuildError::BadTemplate(format!(
            "toc chapter links to unknown page \"{missing}\""
        )));
    }

    // Icon registry: reads every `@block("iconset")` so the inline
    // `:name:` handler and diagram `icon` blocks can resolve names
    // against the bundled packs. Stored inside the pattern engine so
    // inline rendering reaches it with no extra threading; the SVG path
    // pulls it back out via `inline_patterns.icons()`.
    let icons = crate::icons::IconRegistry::load(&doc);

    // Tileset registry: reads every `@block("tileset")` so diagram
    // `tilemap` blocks can resolve their `set` against a spritesheet
    // (and the build can copy the used images into `_wdoc/`). Reads each
    // sheet's pixel dimensions from disk, so a malformed declaration
    // fails the build here. Carried inside the pattern engine like
    // `icons`; the SVG path pulls it back out via `tilesets()`.
    let tilesets = crate::tileset::TilesetRegistry::load(&doc, base_dir.as_deref())?;

    // Document-global inline-text pattern engine, compiled once
    // per build: every `@block("inline_pattern")` (built-in or
    // user-declared) contributes one regex + `to_span` function.
    let inline_patterns = InlinePatterns::load(&doc, page_names, icons, tilesets);

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
            .filter_map(|b| render_block(&doc, &b, &inline_patterns, base_dir.as_deref()))
        {
            content.push_str(&b);
            content.push('\n');
        }

        // Resolve the template: the page's own `template` overrides the
        // document `default_template`. None ⇒ render content bare.
        let template_name = field_symbol(&page, "template").or_else(|| default_template.clone());
        let mut body = match template_name {
            Some(name) => {
                let Some(tmpl) = find_template(&doc, &name) else {
                    return Err(BuildError::BadTemplate(name));
                };
                let title = site_title.clone().unwrap_or_else(|| page_name.clone());
                render_template(
                    &doc,
                    &tmpl,
                    &content,
                    &title,
                    &page_name,
                    &pages,
                    &toc_nodes,
                    theme_toggle,
                    inline_patterns.icons(),
                )
            }
            None => content,
        };
        // Replay terminals are driven by the bundled player; load it once
        // per page (it no-ops on pages without a replay terminal).
        if uses_terminals {
            body.push_str("\n<script src=\"_wdoc/terminal-player.js\" defer></script>\n");
        }
        let html = render_page(&page_name, &css, &body);

        let out_path = out_dir.join(format!("{page_name}.html"));
        fs::write(&out_path, html)
            .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
        count += 1;
    }

    // Every icon resolved while rendering goes into one shared sprite
    // (`_wdoc/icons.svg`) that the pages reference via `<use>`. Written
    // after the page loop so it holds exactly the icons that were used.
    if let Some(sprite) = inline_patterns.icons().build_sprite() {
        write_icon_sprite(out_dir, &sprite)?;
    }

    // Copy each spritesheet referenced by a rendered tilemap into
    // `_wdoc/`. No-op when the document used no tilemap.
    inline_patterns.tilesets().copy_used_images(out_dir)?;

    // Inline `[text](page)` references that didn't resolve to a
    // known page block surface as a build error here, after every
    // page has had a chance to render and report.
    let link_errors = inline_patterns.take_link_errors();
    if !link_errors.is_empty() {
        return Err(BuildError::BadLink(link_errors));
    }

    Ok(count)
}

/// Write the bundled terminal assets (the JetBrains Mono Nerd Font
/// faces + the replay player JS) into `<out>/_wdoc/`. Pages reference
/// them by relative URL, so the dev server and any static host resolve
/// them the same way.
fn write_terminal_assets(out_dir: &Path) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    for (name, bytes) in crate::terminal::FONT_FILES {
        let path = dir.join(name);
        fs::write(&path, bytes)
            .map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    }
    let player = dir.join("terminal-player.js");
    fs::write(&player, crate::terminal::PLAYER_JS)
        .map_err(|e| BuildError::Io(e, format!("write {}", player.display())))?;
    Ok(())
}

/// Write the shared icon sprite into `<out>/_wdoc/icons.svg`. Pages
/// reference its `<symbol>`s by relative URL (`_wdoc/icons.svg#id`), so
/// the dev server and any static host resolve them the same way.
fn write_icon_sprite(out_dir: &Path, sprite: &str) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let path = dir.join(crate::icons::SPRITE_FILE);
    fs::write(&path, sprite).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    Ok(())
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

/// Return the first `toc` chapter `page` reference that isn't a known
/// page name, walking the tree depth-first. `None` if every link
/// resolves (or no chapter links a page).
fn toc_missing_page<'a>(nodes: &'a [TocNode], known: &HashSet<String>) -> Option<&'a str> {
    for n in nodes {
        if let Some(page) = &n.page
            && !known.contains(page)
        {
            return Some(page);
        }
        if let Some(missing) = toc_missing_page(&n.children, known) {
            return Some(missing);
        }
    }
    None
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
