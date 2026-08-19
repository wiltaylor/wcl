//! Markdown backend for wdoc (`wcl wdoc build --type markdown`).
//!
//! A third consumer of the shared lowering pipeline (alongside HTML and
//! PDF): renders each `page` to a `.md` file and each diagram / terminal /
//! wireframe to a standalone `.svg`, in a folder structure that mirrors the
//! HTML build (single site flat at `out_dir`, multiple sites under
//! `<out_dir>/<name>/`, assets in `_wdoc/`). Front matter, prose, lists,
//! tables, code, callouts, images and equations map to native Markdown;
//! interactivity (zoomable diagrams) is dropped and videos become links
//! (local files are copied into `_wdoc/` like other assets).
//! Aimed at AI / text consumers.
//!
//! Reuses [`crate::build`]'s site grouping and registry setup, so failures
//! surface as the shared [`BuildError`] and map to the same CLI exit codes.

mod content;
pub(crate) mod emit;
mod yaml;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, disk_loader};

use crate::build::{
    BuildError, SiteSpec, collect_pages, collect_site_specs, page_name, root_site_name,
    schema_registry, site_start_page,
};
use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::inline::InlinePatterns;
use crate::tileset::TilesetRegistry;

/// Render `file` to a folder of Markdown pages (plus SVG assets) under
/// `out_dir`. Returns the number of pages written. `site_filter` restricts
/// rendering to a single named site.
pub fn markdown(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
) -> Result<usize, BuildError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    let name = file.display().to_string();
    let base_dir = file.parent().map(Path::to_path_buf);

    let loader = schema_registry().loader(disk_loader());
    let doc = Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &crate::build::wdoc_environment(),
        loader,
    )
    .map_err(|e| BuildError::Parse(Report::new(e)))?;

    let errs = crate::build::schema_errors(&doc);
    if !errs.is_empty() {
        let n = errs.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for e in &errs {
            let report = Report::new(e.clone()).with_source_code(src.clone());
            eprintln!("{report:?}");
        }
        return Err(BuildError::Schema(n));
    }

    // The schema-level rendering contract (see `contract_errors`) — fail
    // like a schema violation.
    let reserved = crate::build::contract_errors(&doc);
    if !reserved.is_empty() {
        let n = reserved.len();
        let src = NamedSource::new(name.clone(), user_src.clone());
        for r in reserved {
            eprintln!("{:?}", r.with_source_code(src.clone()));
        }
        return Err(BuildError::Schema(n));
    }

    fs::create_dir_all(out_dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", out_dir.display())))?;

    let site_blocks: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let all_pages = collect_pages(&doc)?;
    if all_pages.is_empty() {
        return Err(BuildError::BadPage("no `page` blocks to render".into()));
    }
    let specs = collect_site_specs(&site_blocks, &all_pages)?;
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

    // Cross-site link context (every declared site → its page-name set and
    // URL prefix), so `[text](site:page)` resolves even under `--site`.
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

    let multi = build_set.len() > 1;
    // Clear any routing error / render warnings stranded by an earlier
    // pass so stale messages can't leak into this one (mirrors `build`).
    let _ = crate::render::take_route_error();
    let _ = crate::render::take_render_warnings();
    let (result, eval_err) = crate::render::scoped_eval_errors(|| -> Result<usize, BuildError> {
        let mut count = 0;
        for spec in &build_set {
            let at_root = !multi || (root_site.is_some() && spec.name == root_site);
            let (site_out, current_prefix) = if at_root {
                (out_dir.to_path_buf(), String::new())
            } else {
                let name = spec.name.as_deref().unwrap_or("site");
                (out_dir.join(name), format!("{name}/"))
            };
            fs::create_dir_all(&site_out)
                .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", site_out.display())))?;
            count += markdown_site(
                &doc,
                base_dir.as_deref(),
                spec,
                &site_out,
                current_prefix,
                &site_pages,
                &site_prefix,
            )?;

            // Landing-page parity with the HTML build: copy the `start` page (or
            // none) to `index.md` so `<site>/` has an entry point.
            if let Some(start) = site_start_page(spec)?
                && start != "index"
            {
                let src = site_out.join(format!("{start}.md"));
                let dst = site_out.join("index.md");
                fs::copy(&src, &dst).map_err(|e| {
                    BuildError::Io(e, format!("copy {} to index.md", src.display()))
                })?;
            }
        }
        if multi && root_site.is_none() {
            write_chooser_index(out_dir, &build_set)?;
        }

        Ok(count)
    });
    // A swallowed block-eval error takes priority: it means a page block
    // would have been silently dropped, so fail loudly with a snippet.
    if let Some((e, src)) = eval_err {
        return Err(BuildError::eval(e, src));
    }
    // An unroutable diagram edge surfaces after the eval check, like the
    // HTML build — static diagrams render in Markdown output too.
    if let Some(msg) = crate::render::take_route_error() {
        return Err(BuildError::EdgeRouting(msg));
    }
    result
}

/// Render one site's pages into `out_dir`. Mirrors `build::build_site`'s
/// per-site registry setup, but emits Markdown + SVG instead of HTML and
/// skips all CSS / template / nav / JS concerns.
#[allow(clippy::too_many_arguments)]
fn markdown_site(
    doc: &Document,
    base_dir: Option<&Path>,
    spec: &SiteSpec<'_>,
    out_dir: &Path,
    current_prefix: String,
    site_pages: &BTreeMap<String, HashSet<String>>,
    site_prefix: &BTreeMap<String, String>,
) -> Result<usize, BuildError> {
    let mut page_names: HashSet<String> = HashSet::new();
    for n in spec.pages.iter().filter_map(page_name) {
        // Routes must be unique within a site — colliding `wdoc_repeater`
        // page labels would otherwise overwrite one `<name>.md` with another.
        if !page_names.insert(n.clone()) {
            return Err(BuildError::DuplicatePage {
                site: spec.name.clone().unwrap_or_else(|| "default".into()),
                name: n,
            });
        }
    }

    // Asset registries, fresh per site (read the document's global
    // iconset/tileset declarations; record usage during render).
    let icons = IconRegistry::load(doc);
    let tilesets = TilesetRegistry::load(doc, base_dir)?;
    let images = ImageRegistry::new(base_dir.map(Path::to_path_buf));
    let videos = crate::video::VideoRegistry::new(base_dir.map(Path::to_path_buf));
    let files = crate::file::FileRegistry::new(base_dir.map(Path::to_path_buf));
    let patterns = InlinePatterns::load(
        doc,
        page_names,
        spec.name.clone(),
        current_prefix,
        site_pages.clone(),
        site_prefix.clone(),
        icons,
        tilesets,
        images,
        videos,
        files,
        crate::inline::Backend::Markdown,
    );
    // Wireframe (`wf_*`) elements bake from this site's UI theme.
    patterns.set_ui_theme(crate::render::resolve_ui_theme(spec.block.as_ref()));
    // Site name + template kind for `@only`/`@except` block visibility.
    let default_template = spec
        .block
        .as_ref()
        .and_then(|b| crate::render::field_symbol(b, "default_template"));
    patterns.set_site_context(spec.name.clone(), default_template);

    let mut count = 0;
    for page in &spec.pages {
        let Some(pn) = page_name(page) else {
            return Err(BuildError::BadPage("page has no name label".into()));
        };
        let md = emit::emit_page(doc, page, &pn, &patterns, base_dir, out_dir)?;
        let path = out_dir.join(format!("{pn}.md"));
        fs::write(&path, md).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
        count += 1;
    }

    // Assets referenced while rendering. The icon sprite lets a diagram's
    // `<use href="_wdoc/icons.svg#…">` resolve when the output is served;
    // images and tileset spritesheets are copied into `_wdoc/`.
    if let Some(sprite) = patterns.icons().build_sprite() {
        let dir = out_dir.join(crate::terminal::ASSET_DIR);
        fs::create_dir_all(&dir)
            .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
        let path = dir.join(crate::icons::SPRITE_FILE);
        fs::write(&path, sprite)
            .map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    }
    patterns.tilesets().copy_used_images(out_dir)?;
    patterns.images().copy_used_images(out_dir)?;
    patterns.files().copy_used(out_dir)?;
    patterns.videos().copy_used_assets(out_dir)?;

    // Internal `[text](page)` links that didn't resolve fail the build.
    let link_errors = patterns.take_link_errors();
    if !link_errors.is_empty() {
        return Err(BuildError::BadLink(link_errors));
    }

    Ok(count)
}

/// Write a top-level `index.md` chooser for a multi-site build with no root
/// site: a list linking to each site's `index.md`.
fn write_chooser_index(out_dir: &Path, sites: &[&SiteSpec<'_>]) -> Result<(), BuildError> {
    let mut body = String::from("# Sites\n\n");
    for s in sites {
        let name = s.name.as_deref().unwrap_or("site");
        let title = s
            .block
            .as_ref()
            .and_then(|b| crate::render::field_utf8(b, "title"))
            .unwrap_or_else(|| name.to_string());
        body.push_str(&format!("- [{title}]({name}/index.md)\n"));
    }
    let path = out_dir.join("index.md");
    fs::write(&path, body).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    Ok(())
}
