use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Environment, Registry, Value, disk_loader};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{
    DeckSectionNode, MenuNode, TocNode, escape_html, field_bool, field_id, field_symbol,
    field_symbol_list_opt, field_utf8, find_template, read_deck, read_menu, read_toc, render_block,
    render_class, render_page, render_template, site_theme_css,
};

/// The wdoc standard library, embedded in the binary and registered
/// under `wdoc/*.wcl` keys plus the public `wdoc.wcl` entry point. A
/// user document opts in with an explicit `import <wdoc.wcl>` line,
/// which pulls in the prelude; the prelude pulls in every other part
/// via importer-relative system imports (`import <core.wcl>` →
/// `wdoc/core.wcl`). The LSP reuses this registry so editing a wdoc
/// document resolves the same embedded library.
pub fn schema_registry() -> Registry {
    let mut r = Registry::new();
    r.register("wdoc.wcl", include_str!("../lib/wdoc.wcl"));
    r.register("wdoc/prelude.wcl", include_str!("../lib/prelude.wcl"));
    r.register("wdoc/core.wcl", include_str!("../lib/core.wcl"));
    r.register("wdoc/theme.wcl", include_str!("../lib/theme.wcl"));
    r.register(
        "wdoc/css-classes.wcl",
        include_str!("../lib/css-classes.wcl"),
    );
    r.register("wdoc/text.wcl", include_str!("../lib/text.wcl"));
    r.register("wdoc/callout.wcl", include_str!("../lib/callout.wcl"));
    r.register("wdoc/wireframe.wcl", include_str!("../lib/wireframe.wcl"));
    r.register("wdoc/table.wcl", include_str!("../lib/table.wcl"));
    r.register("wdoc/list.wcl", include_str!("../lib/list.wcl"));
    r.register("wdoc/components.wcl", include_str!("../lib/components.wcl"));
    r.register(
        "wdoc/diagram-core.wcl",
        include_str!("../lib/diagram-core.wcl"),
    );
    r.register("wdoc/templates.wcl", include_str!("../lib/templates.wcl"));
    r.register(
        "wdoc/presentation.wcl",
        include_str!("../lib/presentation.wcl"),
    );
    r.register("wdoc/inline.wcl", include_str!("../lib/inline.wcl"));
    r.register(
        "wdoc/inline-patterns.wcl",
        include_str!("../lib/inline-patterns.wcl"),
    );
    r.register("wdoc/icons.wcl", include_str!("../lib/icons.wcl"));
    r.register("wdoc/image.wcl", include_str!("../lib/image.wcl"));
    r.register("wdoc/tilemap.wcl", include_str!("../lib/tilemap.wcl"));
    r.register("wdoc/dopesheet.wcl", include_str!("../lib/dopesheet.wcl"));
    r.register("wdoc/map.wcl", include_str!("../lib/map.wcl"));
    r.register("wdoc/flowchart.wcl", include_str!("../lib/flowchart.wcl"));
    r.register("wdoc/charts.wcl", include_str!("../lib/charts.wcl"));
    r.register("wdoc/timeline.wcl", include_str!("../lib/timeline.wcl"));
    r.register("wdoc/card.wcl", include_str!("../lib/card.wcl"));
    r.register("wdoc/headings.wcl", include_str!("../lib/headings.wcl"));
    r.register("wdoc/p.wcl", include_str!("../lib/p.wcl"));
    r.register("wdoc/code.wcl", include_str!("../lib/code.wcl"));
    r.register("wdoc/terminal.wcl", include_str!("../lib/terminal.wcl"));
    r.register("wdoc/tui.wcl", include_str!("../lib/tui.wcl"));
    r.register("wdoc/math.wcl", include_str!("../lib/math.wcl"));
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

pub fn build(file: &Path, out_dir: &Path, site_filter: Option<&str>) -> Result<usize, BuildError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;

    let name = file.display().to_string();

    // The wdoc schema is pulled in by the author's own `import <wdoc.wcl>`
    // line, resolved through the embedded registry below. Relative
    // `import "./pages/foo.wcl"` statements resolve against the source
    // file's own directory, not the wdoc working directory — so disk
    // imports fall through to the disk loader with that base.
    let base_dir = file.parent().map(std::path::Path::to_path_buf);
    let loader = schema_registry().loader(disk_loader());
    let doc = Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &Environment::new(),
        loader,
    )
    .map_err(|e| BuildError::Parse(Report::new(e)))?;

    let errs = doc.schema_errors();
    if !errs.is_empty() {
        let n = errs.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for e in &errs {
            let report = Report::new(e.clone()).with_source_code(src.clone());
            eprintln!("{report:?}");
        }
        return Err(BuildError::Schema(n));
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", out_dir.display())))?;

    // Resolve the sites to build. A document may declare several named
    // `site` blocks; each renders into its own subdirectory. With one
    // site (or `--site`) the chosen site renders flat at `out_dir`, and
    // with none a synthetic default site reproduces the bare flat output.
    let site_blocks: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let all_pages: Vec<Block> = doc.blocks().filter(|b| b.kind() == "page").collect();
    let specs = collect_site_specs(&site_blocks, &all_pages)?;

    // At most one site may be the `root` site (rendered flat at the
    // output root instead of a subdirectory).
    let root_site = root_site_name(&specs)?;

    let build_set: Vec<&SiteSpec> = match site_filter {
        Some(want) => {
            let chosen: Vec<&SiteSpec> = specs
                .iter()
                .filter(|s| s.name.as_deref() == Some(want))
                .collect();
            if chosen.is_empty() {
                return Err(BuildError::BadPage(format!("unknown site \"{want}\"")));
            }
            chosen
        }
        None => specs.iter().collect(),
    };

    // Cross-site link context, built from every declared site (so a
    // `[text](site:page)` link resolves to any site, even under `--site`):
    // each site's page-name set, and its URL prefix in the full layout
    // (`""` for the root site, else `"<name>/"`).
    let mut site_pages: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut site_prefix: BTreeMap<String, String> = BTreeMap::new();
    for s in &specs {
        if let Some(name) = &s.name {
            site_pages.insert(name.clone(), s.pages.iter().filter_map(page_name).collect());
            let prefix = if Some(name) == root_site.as_ref() {
                String::new()
            } else {
                format!("{name}/")
            };
            site_prefix.insert(name.clone(), prefix);
        }
    }

    // The root site's title (or name), used as the "back to the main
    // site" link text the sub-site templates show.
    let root_title = match &root_site {
        Some(name) => specs
            .iter()
            .find(|s| s.name.as_ref() == Some(name))
            .and_then(|s| s.block.as_ref())
            .and_then(|b| field_utf8(b, "title"))
            .unwrap_or_else(|| name.clone()),
        None => "Home".to_string(),
    };

    // A site renders flat at the root when it's the only one built (a
    // single declared site or `--site`) or it's the `root` site; the rest
    // go to `<out>/<name>/`. A chooser index is generated only when there
    // are several sites and none claims the root.
    let multi = build_set.len() > 1;
    let mut count = 0;
    for spec in &build_set {
        let at_root = !multi || (root_site.is_some() && spec.name == root_site);
        let (site_out, current_prefix) = if at_root {
            (out_dir.to_path_buf(), String::new())
        } else {
            let name = spec.name.as_deref().unwrap_or("site");
            (out_dir.join(name), format!("{name}/"))
        };
        // Sub-sites (anything not at the root, in a multi-site build) get
        // a back-link to the root index; the root site itself gets none.
        let (home_href, home_title) = if at_root || !multi {
            (String::new(), String::new())
        } else {
            ("../index.html".to_string(), root_title.clone())
        };
        fs::create_dir_all(&site_out)
            .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", site_out.display())))?;
        count += build_site(
            &doc,
            base_dir.as_deref(),
            spec,
            &site_out,
            current_prefix,
            &site_pages,
            &site_prefix,
            &home_href,
            &home_title,
        )?;
        // Landing page: a page marked `start` is copied to this site's
        // `index.html`, so `/` (or `/<site>/`) serves it without needing
        // a page literally named `index`. The page also stays reachable
        // at its own `<name>.html`.
        if let Some(start) = site_start_page(spec)?
            && start != "index"
        {
            let src = site_out.join(format!("{start}.html"));
            let dst = site_out.join("index.html");
            fs::copy(&src, &dst)
                .map_err(|e| BuildError::Io(e, format!("copy {} to index.html", src.display())))?;
        }
        // Fall back to a redirect index for a multi-site sub-site that has
        // neither a `start` nor an `index` page (no-op if one now exists).
        if multi {
            ensure_site_index(&site_out, spec)?;
        }
    }
    if multi && root_site.is_none() {
        // No root site ⇒ the root is a generated chooser (site-agnostic,
        // so only the global/unscoped CSS).
        write_chooser_index(out_dir, &site_css(&doc, None, None), &build_set)?;
    }

    Ok(count)
}

/// The name of the site marked `root = true`, if any. More than one root
/// site is a build error.
fn root_site_name(specs: &[SiteSpec<'_>]) -> Result<Option<String>, BuildError> {
    let mut root: Option<String> = None;
    for s in specs {
        let is_root = s
            .block
            .as_ref()
            .and_then(|b| field_bool(b, "root"))
            .unwrap_or(false);
        if is_root {
            if root.is_some() {
                return Err(BuildError::BadPage(
                    "more than one `site` is marked `root = true`".into(),
                ));
            }
            root = s.name.clone();
        }
    }
    Ok(root)
}

/// One site to render: its name (the `site` block's inline label, `None`
/// for an unnamed single site or the synthetic default), the config
/// block (`None` for the synthetic default), and its member pages in
/// source order.
pub(crate) struct SiteSpec<'a> {
    pub(crate) name: Option<String>,
    pub(crate) block: Option<Block<'a>>,
    pub(crate) pages: Vec<Block<'a>>,
}

/// Group the document's pages under the declared `site` blocks. With no
/// `site` block, returns a single synthetic default site owning every
/// page (reproducing the pre-multi-site bare flat build).
pub(crate) fn collect_site_specs<'a>(
    site_blocks: &[Block<'a>],
    all_pages: &[Block<'a>],
) -> Result<Vec<SiteSpec<'a>>, BuildError> {
    if site_blocks.is_empty() {
        return Ok(vec![SiteSpec {
            name: None,
            block: None,
            pages: all_pages.to_vec(),
        }]);
    }

    let names: Vec<Option<String>> = site_blocks.iter().map(site_name).collect();
    if site_blocks.len() > 1 {
        if names.iter().any(Option::is_none) {
            return Err(BuildError::BadPage(
                "a document with multiple `site` blocks must name each one \
                 (e.g. `site docs { … }`)"
                    .into(),
            ));
        }
        let mut seen = HashSet::new();
        for n in names.iter().flatten() {
            if !seen.insert(n.as_str()) {
                return Err(BuildError::BadPage(format!("duplicate site name \"{n}\"")));
            }
        }
    }

    // A page's `sites` list must reference declared site names.
    let known: HashSet<&str> = names.iter().flatten().map(String::as_str).collect();
    for p in all_pages {
        for r in block_sites(p).into_iter().flatten() {
            if !known.contains(r.as_str()) {
                return Err(BuildError::BadPage(format!(
                    "page references unknown site \"{r}\""
                )));
            }
        }
    }

    Ok(site_blocks
        .iter()
        .zip(names)
        .map(|(block, name)| {
            let pages = all_pages
                .iter()
                .filter(|p| block_in_site(p, name.as_deref()))
                .cloned()
                .collect();
            SiteSpec {
                name,
                block: Some(block.clone()),
                pages,
            }
        })
        .collect())
}

/// The `site` block's inline name label, if any.
fn site_name(block: &Block<'_>) -> Option<String> {
    match block.labels().ok()?.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

/// A block's declared `sites` membership list (used by `page`, `class`,
/// and `stylesheet`). `None` ⇒ the field is absent, so the block belongs
/// to every site — same as an empty list.
fn block_sites(block: &Block<'_>) -> Option<Vec<String>> {
    field_symbol_list_opt(block, "sites")
}

/// Whether a block belongs to the site named `site_name`. An absent or
/// empty `sites` list means every site.
fn block_in_site(block: &Block<'_>, site_name: Option<&str>) -> bool {
    match block_sites(block) {
        None => true,
        Some(list) if list.is_empty() => true,
        Some(list) => site_name.is_some_and(|n| list.iter().any(|s| s == n)),
    }
}

/// Build the document's `<style>` content for one site: the bundled
/// syntax-highlight theme, then every `@block("stylesheet")`, then every
/// `@block("class")` rule — each group ordered library-before-user
/// (imported blocks first) so user declarations override by cascade, and
/// each filtered to the blocks belonging to `site_name` (blocks with no
/// `sites` field are global). This lets one site carry its own theme in a
/// multi-site document without affecting the others.
///
/// The site's selected colour theme (its `theme`/`accent` fields, or the
/// `nord` default) is spliced in between the library and user `class`
/// rules, so it overrides the built-in defaults (chart palette, syntax
/// tokens) while user `class` blocks still win. `site_block` is the
/// `@block("site")` carrying the selection (`None` ⇒ bare/unthemed).
fn site_css(doc: &Document, site_name: Option<&str>, site_block: Option<&Block<'_>>) -> String {
    let mut lib_sheets = Vec::new();
    let mut user_sheets = Vec::new();
    let mut lib_classes = Vec::new();
    let mut user_classes = Vec::new();
    for (origin, b) in doc.blocks_with_source() {
        if !block_in_site(&b, site_name) {
            continue;
        }
        match b.kind() {
            "stylesheet" => {
                if let Some(css) = field_utf8(&b, "css") {
                    if origin.is_some() {
                        &mut lib_sheets
                    } else {
                        &mut user_sheets
                    }
                    .push(css);
                }
            }
            "class" => {
                if let Some(css) = render_class(&b) {
                    if origin.is_some() {
                        &mut lib_classes
                    } else {
                        &mut user_classes
                    }
                    .push(css);
                }
            }
            _ => {}
        }
    }
    let stylesheet_css = lib_sheets
        .into_iter()
        .chain(user_sheets)
        .collect::<Vec<_>>()
        .join("\n");
    // The colour theme sits between the library classes (whose defaults
    // it overrides) and the user classes (which still win).
    let theme_css = site_theme_css(doc, site_block);
    let class_css = lib_classes
        .into_iter()
        .chain(theme_css.into_iter().filter(|s| !s.is_empty()))
        .chain(user_classes)
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{stylesheet_css}\n{class_css}", highlight::theme_css())
}

/// Render one site's pages into `out_dir`. Everything that scopes to a
/// site — its template/title/toc, nav page list, link-resolution name
/// set, and `_wdoc/` assets — comes from `spec`, so each site is a
/// self-contained directory whose pages use plain relative `_wdoc/…`
/// references. Returns the number of pages written.
#[allow(clippy::too_many_arguments)]
fn build_site(
    doc: &Document,
    base_dir: Option<&Path>,
    spec: &SiteSpec<'_>,
    out_dir: &Path,
    current_prefix: String,
    site_pages: &BTreeMap<String, HashSet<String>>,
    site_prefix: &BTreeMap<String, String>,
    home_href: &str,
    home_title: &str,
) -> Result<usize, BuildError> {
    // The page <style>: bundled theme + stylesheets + class rules, scoped
    // to this site (global blocks plus those whose `sites` list names it).
    let css = site_css(doc, spec.name.as_deref(), spec.block.as_ref());

    // Terminal + pan/zoom assets, scoped to this site's pages, so a site
    // that uses neither pays nothing.
    let uses_terminals = spec.pages.iter().any(crate::terminal::uses_terminal);
    if uses_terminals {
        write_terminal_assets(out_dir)?;
    }
    let uses_pan_zoom = spec.pages.iter().any(crate::render::uses_pan_zoom);
    let uses_map = spec.pages.iter().any(crate::render::uses_map);
    // A map drives the same viewBox camera as a pan/zoom diagram, so it
    // needs the pan/zoom player too — plus its own layer/card player.
    if uses_pan_zoom || uses_map {
        write_diagram_assets(out_dir)?;
    }
    if uses_map {
        write_map_assets(out_dir)?;
    }
    let uses_dopesheet = spec.pages.iter().any(crate::dopesheet::uses_dopesheet);
    if uses_dopesheet {
        write_dopesheet_assets(out_dir)?;
    }

    // Site descriptor: the default template + title a template can show.
    // `None` block ⇒ the synthetic default site, so pages render bare
    // unless they set their own `template`.
    let default_template = spec
        .block
        .as_ref()
        .and_then(|b| field_symbol(b, "default_template"));
    let site_title = spec.block.as_ref().and_then(|b| field_utf8(b, "title"));
    let theme_toggle = spec
        .block
        .as_ref()
        .and_then(|b| field_bool(b, "theme_toggle"))
        .unwrap_or(false);
    let toc_nodes: Vec<TocNode> = spec.block.as_ref().map(read_toc).unwrap_or_default();
    let menu_nodes: Vec<MenuNode> = spec.block.as_ref().map(read_menu).unwrap_or_default();
    let deck_nodes: Vec<DeckSectionNode> = spec.block.as_ref().map(read_deck).unwrap_or_default();
    // A `presentation` site renders all its slides into one `index.html`
    // (a deck), rather than one file per page. The `deck` block supplies
    // the slide grid; `default_template = :presentation` selects it.
    let is_presentation = default_template.as_deref() == Some("presentation");

    // Ordered (name, href) list of this site's pages for template nav,
    // and the name set the inline link pattern resolves `[text](page)`
    // against — both scoped to the site, so nav lists only this site's
    // pages and links resolve within it.
    let pages: Vec<(String, String)> = spec
        .pages
        .iter()
        .filter_map(|p| page_name(p).map(|n| (n.clone(), format!("{n}.html"))))
        .collect();
    let page_names: HashSet<String> = pages.iter().map(|(n, _)| n.clone()).collect();

    if let Some(missing) = toc_missing_page(&toc_nodes, &page_names) {
        return Err(BuildError::BadTemplate(format!(
            "toc chapter links to unknown page \"{missing}\""
        )));
    }
    if let Some(missing) = menu_missing_page(&menu_nodes, &page_names) {
        return Err(BuildError::BadTemplate(format!(
            "menu item links to unknown page \"{missing}\""
        )));
    }
    if is_presentation {
        if deck_nodes.is_empty() {
            return Err(BuildError::BadTemplate(
                "a `presentation` site needs a `deck { section { slide … } }` block".into(),
            ));
        }
        if let Some(missing) = deck_missing_slide(&deck_nodes, &page_names) {
            return Err(BuildError::BadTemplate(format!(
                "deck slide links to unknown page \"{missing}\""
            )));
        }
    }

    // Asset registries — fresh per site so the icon sprite + copied
    // images cover exactly this site's usage. They read the document's
    // global iconset/tileset declarations but record usage during render.
    let icons = crate::icons::IconRegistry::load(doc);
    let tilesets = crate::tileset::TilesetRegistry::load(doc, base_dir)?;
    let images = crate::image::ImageRegistry::new(base_dir.map(Path::to_path_buf));
    let inline_patterns = InlinePatterns::load(
        doc,
        page_names,
        spec.name.clone(),
        current_prefix,
        site_pages.clone(),
        site_prefix.clone(),
        icons,
        tilesets,
        images,
    );

    let mut count = 0;

    // Presentation site: render every slide once into a single deck page
    // (`index.html`), driven by the `presentation` template + player. The
    // per-page loop below is skipped (slides aren't standalone files).
    if is_presentation {
        // Resolve each `slide` page to its rendered body + speaker notes,
        // building the `list<DeckSection>` the template lays out. Ids are
        // unique across the whole deck (it's one HTML document).
        let page_by_name: BTreeMap<String, &Block> = spec
            .pages
            .iter()
            .filter_map(|p| page_name(p).map(|n| (n, p)))
            .collect();
        let mut dup_seen = HashSet::new();
        let mut sections_val = Vec::new();
        for sec in &deck_nodes {
            let mut slides_val = Vec::new();
            for slide_page in &sec.slides {
                let Some(&page) = page_by_name.get(slide_page) else {
                    continue; // unreachable: validated by deck_missing_slide
                };
                if let Some(dup) = collect_duplicate_id(page, &mut dup_seen) {
                    return Err(BuildError::DuplicateId {
                        page: slide_page.clone(),
                        id: dup,
                    });
                }
                // Visible content: the page's blocks minus its `notes`.
                let mut content = String::new();
                for b in page
                    .blocks()
                    .filter(|b| b.kind() != "notes")
                    .filter_map(|b| render_block(doc, &b, &inline_patterns, base_dir))
                {
                    content.push_str(&b);
                    content.push('\n');
                }
                // Speaker notes: the children of any `notes` block.
                let mut notes = String::new();
                for nb in page.blocks().filter(|b| b.kind() == "notes") {
                    for cb in nb
                        .blocks()
                        .filter_map(|b| render_block(doc, &b, &inline_patterns, base_dir))
                    {
                        notes.push_str(&cb);
                        notes.push('\n');
                    }
                }
                let mut m = BTreeMap::new();
                m.insert("content".to_string(), Value::Utf8(content));
                m.insert("notes".to_string(), Value::Utf8(notes));
                m.insert("title".to_string(), Value::Utf8(slide_page.clone()));
                slides_val.push(Value::Record {
                    ty: vec!["DeckSlide".to_string()],
                    fields: m,
                });
            }
            let mut sm = BTreeMap::new();
            sm.insert("title".to_string(), Value::Utf8(sec.title.clone()));
            sm.insert("slides".to_string(), Value::List(slides_val));
            sections_val.push(Value::Record {
                ty: vec!["DeckSection".to_string()],
                fields: sm,
            });
        }

        let Some(tmpl) = find_template(doc, "presentation") else {
            return Err(BuildError::BadTemplate("presentation".into()));
        };
        let title = site_title
            .clone()
            .or_else(|| spec.name.clone())
            .unwrap_or_else(|| "Presentation".to_string());
        let mut body = render_template(
            doc,
            &tmpl,
            "",
            &title,
            "",
            &pages,
            &toc_nodes,
            &menu_nodes,
            Value::List(sections_val),
            theme_toggle,
            home_href,
            home_title,
            &inline_patterns,
        );
        // The slides may use the same interactive assets a normal page can.
        if uses_terminals {
            body.push_str("\n<script src=\"_wdoc/terminal-player.js\" defer></script>\n");
        }
        if uses_pan_zoom || uses_map {
            body.push_str("\n<script src=\"_wdoc/diagram-pan-zoom.js\" defer></script>\n");
        }
        if uses_map {
            body.push_str("\n<script src=\"_wdoc/wdoc-map.js\" defer></script>\n");
        }
        if uses_dopesheet {
            body.push_str("\n<script src=\"_wdoc/dopesheet-player.js\" defer></script>\n");
        }
        // The deck keyboard-navigation player.
        write_presentation_assets(out_dir)?;
        body.push_str("\n<script src=\"_wdoc/presentation.js\" defer></script>\n");
        let html = render_page(&title, &css, &body);
        let out_path = out_dir.join("index.html");
        fs::write(&out_path, html)
            .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
        count = 1;
    }

    for page in &spec.pages {
        if is_presentation {
            break;
        }
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
        if let Some(dup) = collect_duplicate_id(page, &mut seen) {
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
            .filter_map(|b| render_block(doc, &b, &inline_patterns, base_dir))
        {
            content.push_str(&b);
            content.push('\n');
        }

        // Resolve the template: the page's own `template` overrides the
        // site `default_template`. None ⇒ render content bare.
        let template_name = field_symbol(page, "template").or_else(|| default_template.clone());
        let mut body = match template_name {
            Some(name) => {
                let Some(tmpl) = find_template(doc, &name) else {
                    return Err(BuildError::BadTemplate(name));
                };
                let title = site_title.clone().unwrap_or_else(|| page_name.clone());
                render_template(
                    doc,
                    &tmpl,
                    &content,
                    &title,
                    &page_name,
                    &pages,
                    &toc_nodes,
                    &menu_nodes,
                    Value::List(Vec::new()),
                    theme_toggle,
                    home_href,
                    home_title,
                    &inline_patterns,
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
        // loaded once per page (it no-ops on pages without one). A map
        // needs that camera plus its own layer/card player.
        if uses_pan_zoom || uses_map {
            body.push_str("\n<script src=\"_wdoc/diagram-pan-zoom.js\" defer></script>\n");
        }
        if uses_map {
            body.push_str("\n<script src=\"_wdoc/wdoc-map.js\" defer></script>\n");
        }
        // Animated dopesheets are driven by the bundled player, loaded
        // once per page (it no-ops on pages without one).
        if uses_dopesheet {
            body.push_str("\n<script src=\"_wdoc/dopesheet-player.js\" defer></script>\n");
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
    // `_wdoc/`. No-op when the site used no tilemap.
    inline_patterns.tilesets().copy_used_images(out_dir)?;

    // Copy each local image referenced by a rendered `image` block (page
    // or diagram) into `_wdoc/`. No-op when none were used.
    inline_patterns.images().copy_used_images(out_dir)?;

    // Inline `[text](page)` references that didn't resolve to a known
    // page in this site surface as a build error here.
    let link_errors = inline_patterns.take_link_errors();
    if !link_errors.is_empty() {
        return Err(BuildError::BadLink(link_errors));
    }

    Ok(count)
}

/// Ensure a site subdirectory has an `index.html` so `/<site>/` lands
/// somewhere. A site that already has an `index` page wrote one; else
/// write a minimal redirect to its first page (none for an empty site).
fn ensure_site_index(out_dir: &Path, spec: &SiteSpec<'_>) -> Result<(), BuildError> {
    let index = out_dir.join("index.html");
    if index.exists() {
        return Ok(());
    }
    let Some(first) = spec.pages.iter().find_map(page_name) else {
        return Ok(());
    };
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"refresh\" content=\"0; url={first}.html\">\
         <title>Redirecting…</title></head>\
         <body><a href=\"{first}.html\">Continue</a></body></html>"
    );
    fs::write(&index, html).map_err(|e| BuildError::Io(e, format!("write {}", index.display())))?;
    Ok(())
}

/// Write the top-level chooser `index.html` for a multi-site build: a
/// list linking to each site's subdirectory, labelled by its title (or
/// name). Reuses the page shell so it inherits the global stylesheet.
fn write_chooser_index(
    out_dir: &Path,
    css: &str,
    sites: &[&SiteSpec<'_>],
) -> Result<(), BuildError> {
    let mut items = String::new();
    for s in sites {
        let name = s.name.as_deref().unwrap_or("site");
        let title = s
            .block
            .as_ref()
            .and_then(|b| field_utf8(b, "title"))
            .unwrap_or_else(|| name.to_string());
        items.push_str(&format!(
            "<li><a href=\"{name}/\">{}</a></li>",
            escape_html(&title)
        ));
    }
    let body = format!("<h1>Sites</h1>\n<ul class=\"wdoc-site-index\">{items}</ul>");
    let html = render_page("index", css, &body);
    let path = out_dir.join("index.html");
    fs::write(&path, html).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    Ok(())
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

/// Write the bundled map player (layer level-of-detail + popup cards) into
/// `<out>/_wdoc/`. Pages with a `map` reference it by relative URL.
fn write_map_assets(out_dir: &Path) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let player = dir.join("wdoc-map.js");
    fs::write(&player, crate::render::WDOC_MAP_JS)
        .map_err(|e| BuildError::Io(e, format!("write {}", player.display())))?;
    Ok(())
}

/// Write the bundled dopesheet player into `<out>/_wdoc/`. Pages with a
/// `dopesheet` reference it by relative URL.
fn write_dopesheet_assets(out_dir: &Path) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let player = dir.join("dopesheet-player.js");
    fs::write(&player, crate::render::DOPESHEET_PLAYER_JS)
        .map_err(|e| BuildError::Io(e, format!("write {}", player.display())))?;
    Ok(())
}

/// Write the bundled presentation (deck) navigation player into
/// `<out>/_wdoc/`. A `presentation` site's single deck page references
/// it by relative URL.
fn write_presentation_assets(out_dir: &Path) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let player = dir.join("presentation.js");
    fs::write(&player, crate::render::PRESENTATION_PLAYER_JS)
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

/// The name of the page marked `start = true` in this site, if any —
/// the page served when no page is specified (`/` or `/<site>/`).
/// Errors if more than one page in the site claims it.
fn site_start_page(spec: &SiteSpec<'_>) -> Result<Option<String>, BuildError> {
    let mut start: Option<String> = None;
    for p in &spec.pages {
        if field_bool(p, "start") == Some(true) {
            let name = page_name(p).unwrap_or_default();
            if let Some(prev) = &start {
                return Err(BuildError::BadPage(format!(
                    "site has multiple start pages (\"{prev}\" and \"{name}\"); \
                     only one page may set start = true"
                )));
            }
            start = Some(name);
        }
    }
    Ok(start)
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

/// Return the first `menu` item `page` reference that isn't a known page
/// name, walking the tree depth-first. External `href`s are not checked.
/// `None` if every page link resolves (or no item links a page).
fn menu_missing_page<'a>(nodes: &'a [MenuNode], known: &HashSet<String>) -> Option<&'a str> {
    for n in nodes {
        if let Some(page) = &n.page
            && !known.contains(page)
        {
            return Some(page);
        }
        if let Some(missing) = menu_missing_page(&n.children, known) {
            return Some(missing);
        }
    }
    None
}

/// Return the first deck `slide` page reference that isn't a known page
/// name, in source order. `None` if every slide resolves.
fn deck_missing_slide<'a>(
    nodes: &'a [DeckSectionNode],
    known: &HashSet<String>,
) -> Option<&'a str> {
    for n in nodes {
        for s in &n.slides {
            if !known.contains(s) {
                return Some(s);
            }
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
