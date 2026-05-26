use std::collections::HashSet;
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Environment, Registry, Value, disk_loader};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{
    TocNode, field_bool, field_id, field_symbol, field_utf8, find_template, read_toc, render_block,
    render_class, render_page, render_template,
};

/// The wdoc standard library, embedded in the binary and registered
/// under `wdoc/*.wcl` keys. A user document picks it up through the
/// single `import <wdoc/prelude.wcl>` line we prepend in [`build`]; the
/// prelude pulls in every other part via importer-relative system
/// imports (`import <core.wcl>` → `wdoc/core.wcl`).
fn schema_registry() -> Registry {
    let mut r = Registry::new();
    r.register("wdoc/prelude.wcl", include_str!("../lib/prelude.wcl"));
    r.register("wdoc/core.wcl", include_str!("../lib/core.wcl"));
    r.register(
        "wdoc/css-classes.wcl",
        include_str!("../lib/css-classes.wcl"),
    );
    r.register("wdoc/text.wcl", include_str!("../lib/text.wcl"));
    r.register("wdoc/callout.wcl", include_str!("../lib/callout.wcl"));
    r.register("wdoc/table.wcl", include_str!("../lib/table.wcl"));
    r.register(
        "wdoc/diagram-core.wcl",
        include_str!("../lib/diagram-core.wcl"),
    );
    r.register("wdoc/templates.wcl", include_str!("../lib/templates.wcl"));
    r.register("wdoc/inline.wcl", include_str!("../lib/inline.wcl"));
    r.register(
        "wdoc/inline-patterns.wcl",
        include_str!("../lib/inline-patterns.wcl"),
    );
    r.register("wdoc/icons.wcl", include_str!("../lib/icons.wcl"));
    r.register("wdoc/tilemap.wcl", include_str!("../lib/tilemap.wcl"));
    r.register("wdoc/flowchart.wcl", include_str!("../lib/flowchart.wcl"));
    r.register("wdoc/charts.wcl", include_str!("../lib/charts.wcl"));
    r.register("wdoc/headings.wcl", include_str!("../lib/headings.wcl"));
    r.register("wdoc/code.wcl", include_str!("../lib/code.wcl"));
    r.register("wdoc/terminal.wcl", include_str!("../lib/terminal.wcl"));
    r
}

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

    // Prepend a single line that pulls in the embedded wdoc schema via a
    // system import. Resolving it through the registry (rather than
    // inlining ~1.5k lines) keeps user line/column diagnostics shifted by
    // just one line; the schema itself reports against its own `<wdoc/…>`
    // source names.
    let composed = format!("import <wdoc/prelude.wcl>\n{user_src}");
    let name = file.display().to_string();

    // Relative `import "./pages/foo.wcl"` statements inside the user
    // source must resolve against the source file's own directory, not
    // the wdoc working directory — so disk imports fall through to the
    // disk loader with that base. The registry serves the system import.
    let base_dir = file.parent().map(std::path::Path::to_path_buf);
    let loader = schema_registry().loader(disk_loader());
    let doc = Document::open_at_with_loader(
        &composed,
        &name,
        base_dir.clone(),
        &Environment::new(),
        loader,
    )
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
    //
    // The bundled default classes (e.g. the chart palette) live in the
    // embedded schema (an import); the user's classes live in the root
    // document. CSS cascades, so the library defaults must come first to
    // remain overridable — emit imported class rules ahead of root ones,
    // each group in source order.
    let (lib_classes, user_classes): (Vec<String>, Vec<String>) = {
        let mut lib = Vec::new();
        let mut user = Vec::new();
        for (origin, b) in doc.blocks_with_source() {
            if b.kind() != "class" {
                continue;
            }
            if let Some(css) = render_class(&b) {
                if origin.is_some() {
                    &mut lib
                } else {
                    &mut user
                }
                .push(css);
            }
        }
        (lib, user)
    };
    let class_css: String = lib_classes
        .into_iter()
        .chain(user_classes)
        .collect::<Vec<_>>()
        .join("\n");
    // Chart styling (palette + axes) is no longer a constant here — it
    // migrated to bundled `class` blocks in wdoc.wcl and so rides the
    // `{class_css}` segment (emitted before user classes, after these).
    let css = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{class_css}",
        crate::render::BASE_CSS,
        crate::render::HEADING_CSS,
        highlight::theme_css(),
        crate::render::TABLE_CSS,
        crate::render::SITE_CSS,
        crate::render::BOOK_CSS,
        crate::render::TERMINAL_CSS,
        crate::render::ICON_CSS,
        crate::render::CALLOUT_CSS,
        crate::render::TILEMAP_CSS,
        crate::render::DIAGRAM_CSS,
    );

    // Terminals need the bundled font + replay player written alongside
    // the pages. Only emit them when the document actually uses a
    // terminal, so font-free sites pay nothing.
    let uses_terminals = doc.blocks().any(|b| crate::terminal::uses_terminal(&b));
    if uses_terminals {
        write_terminal_assets(out_dir)?;
    }

    // Interactive diagrams ship a small pan/zoom player, written + loaded
    // only when a diagram opts in via `pan_zoom`.
    let uses_pan_zoom = doc.blocks().any(|b| crate::render::uses_pan_zoom(&b));
    if uses_pan_zoom {
        write_diagram_assets(out_dir)?;
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
        // Interactive diagrams are driven by the bundled pan/zoom player,
        // loaded once per page (it no-ops on pages without one).
        if uses_pan_zoom {
            body.push_str("\n<script src=\"_wdoc/diagram-pan-zoom.js\" defer></script>\n");
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

/// Write the bundled diagram pan/zoom player into `<out>/_wdoc/`. Pages
/// with an interactive diagram reference it by relative URL.
fn write_diagram_assets(out_dir: &Path) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let player = dir.join("diagram-pan-zoom.js");
    fs::write(&player, crate::render::DIAGRAM_PAN_ZOOM_JS)
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
