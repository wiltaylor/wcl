//! Skill-folder target (`wcl wdoc skill`).
//!
//! A Markdown-backed variant that lays a site out as a Claude / Agent
//! **Skill folder**: the site's `start` page becomes `SKILL.md` at the
//! folder root (its YAML front matter built from the site's `skill { }`
//! block — the required `name` / `description`), every other page is written
//! under `references/<name>.md`, and `file` blocks ship arbitrary files into
//! their `dir` (`scripts/`, `assets/`, …). It reuses the Markdown emitter,
//! the shared registries, and `build`'s site grouping; failures surface as
//! the shared [`BuildError`].
//!
//! A site opts in with `default_template = :ai_skill`; the backend refuses a
//! site that hasn't (so a plain Markdown site isn't silently re-laid-out).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, disk_loader};

use super::{emit, yaml};
use crate::build::{
    BuildError, SiteSpec, collect_pages, collect_site_specs, is_skill_site, page_name,
    root_site_name, schema_registry, site_start_page,
};
use crate::icons::IconRegistry;
use crate::image::ImageRegistry;
use crate::inline::InlinePatterns;
use crate::render::field_symbol;
use crate::tileset::TilesetRegistry;

/// Render `file` to a skill folder under `out_dir`. Returns the number of
/// pages written (SKILL.md + every reference page). `site_filter` restricts
/// rendering to a single named site.
pub fn skill(file: &Path, out_dir: &Path, site_filter: Option<&str>) -> Result<usize, BuildError> {
    let user_src = fs::read_to_string(file)
        .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?;
    let name = file.display().to_string();
    let base_dir = file.parent().map(Path::to_path_buf);

    let loader = schema_registry().loader(disk_loader());
    let doc = Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &crate::build::wdoc_environment(base_dir.as_deref()),
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
        // Build every skill site; non-skill sites belong to the other targets.
        None => {
            let chosen: Vec<&SiteSpec> = specs.iter().filter(|s| is_skill_site(s)).collect();
            if chosen.is_empty() {
                return Err(BuildError::BadPage(
                    "no `:ai_skill` site to build — set `default_template = :ai_skill` \
                     on a `site` (with a `skill { … }` block)"
                        .into(),
                ));
            }
            chosen
        }
    };

    // Cross-site link context (every declared site → its page-name set and
    // URL prefix). Skill links are intra-site, but `[text](site:page)` still
    // resolves through these.
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
    // Clear stale sinks, then bound the render in a scoped pass so a
    // swallowed block-eval error fails the build with a snippet — the
    // same contract as the Markdown and HTML targets.
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
            count += skill_site(
                &doc,
                base_dir.as_deref(),
                spec,
                &site_out,
                current_prefix,
                &site_pages,
                &site_prefix,
            )?;
        }
        Ok(count)
    });
    if let Some((e, src)) = eval_err {
        return Err(BuildError::eval(e, src));
    }
    if let Some(msg) = crate::render::take_route_error() {
        return Err(BuildError::EdgeRouting(msg));
    }
    result
}

/// Render one site as a skill folder into `out_dir`. The start page →
/// `SKILL.md`; every other page → `references/<name>.md`.
#[allow(clippy::too_many_arguments)]
fn skill_site(
    doc: &Document,
    base_dir: Option<&Path>,
    spec: &SiteSpec<'_>,
    out_dir: &Path,
    current_prefix: String,
    site_pages: &BTreeMap<String, HashSet<String>>,
    site_prefix: &BTreeMap<String, String>,
) -> Result<usize, BuildError> {
    let label = spec.name.as_deref().unwrap_or("(unnamed)");
    let block = spec.block.as_ref().ok_or_else(|| {
        BuildError::BadPage(
            "`wcl wdoc skill` needs a `site` block with `default_template = :ai_skill` \
             and a `skill { name = … description = … }` block"
                .into(),
        )
    })?;
    if field_symbol(block, "default_template").as_deref() != Some("ai_skill") {
        return Err(BuildError::BadPage(format!(
            "site \"{label}\" is not a skill — set `default_template = :ai_skill` to build it \
             with `wcl wdoc skill`"
        )));
    }
    let skill_cfg = block
        .blocks()
        .find(|b| b.kind() == "skill")
        .ok_or_else(|| {
            BuildError::BadPage(format!(
                "skill site \"{label}\" is missing its `skill {{ name = … description = … }}` block"
            ))
        })?;
    let start = site_start_page(spec)?.ok_or_else(|| {
        BuildError::BadPage(format!(
            "skill site \"{label}\" needs a start page (`start = true`) to become SKILL.md"
        ))
    })?;

    let mut page_names: HashSet<String> = HashSet::new();
    for n in spec.pages.iter().filter_map(page_name) {
        // Routes must be unique within a site — colliding `wdoc_repeater`
        // page labels would otherwise overwrite one reference file.
        if !page_names.insert(n.clone()) {
            return Err(BuildError::DuplicatePage {
                site: spec.name.clone().unwrap_or_else(|| "default".into()),
                name: n,
            });
        }
    }

    // Asset registries, fresh per site (as in `markdown_site`).
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
    patterns.set_ui_theme(crate::render::resolve_ui_theme(spec.block.as_ref()));
    patterns.set_site_context(spec.name.clone(), Some("ai_skill".to_string()));
    // Internal page links resolve into the skill folder layout.
    patterns.set_skill_layout(start.clone());

    let refs_dir = out_dir.join("references");
    let mut count = 0;
    for page in &spec.pages {
        let Some(pn) = page_name(page) else {
            return Err(BuildError::BadPage("page has no name label".into()));
        };
        let is_start = pn == start;
        // Reference pages live one level deep, so their asset / page links get
        // a `../` prefix.
        patterns.set_skill_current_reference(!is_start);
        if is_start {
            let fm = yaml::skill_front_matter(&skill_cfg, page)?;
            let md = emit::emit_page_with_front_matter(
                doc,
                page,
                &pn,
                &patterns,
                base_dir,
                out_dir,
                Some(fm),
            )?;
            let path = out_dir.join("SKILL.md");
            fs::write(&path, md)
                .map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
        } else {
            fs::create_dir_all(&refs_dir)
                .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", refs_dir.display())))?;
            let md = emit::emit_page(doc, page, &pn, &patterns, base_dir, out_dir)?;
            let path = refs_dir.join(format!("{pn}.md"));
            fs::write(&path, md)
                .map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
        }
        count += 1;
    }

    // Assets referenced while rendering. Diagrams/terminals are written under
    // `_wdoc/` (referenced with the per-page `../` prefix); the icon sprite,
    // images, and `file` blocks copy in the same way.
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
