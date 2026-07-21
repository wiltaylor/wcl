use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use miette::{NamedSource, Report};
use wcl_lang::{Block, Document, Environment, Registry, Value, disk_loader, from_fn};

use crate::highlight;
use crate::inline::InlinePatterns;
use crate::render::{
    DeckSectionNode, FooterButtonNode, MAX_LOWER_DEPTH, MenuNode, TocNode, escape_html,
    expand_component_children, expand_instance_children, expand_repeater_children, field_bool,
    field_id, field_symbol, field_symbol_list_opt, field_utf8, field_utf8_list, find_template,
    label_string, read_deck, read_menu, read_sidebar_footer, read_toc, render_block, render_class,
    render_page, render_template, site_theme_css,
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
    r.register("wdoc/fonts.wcl", include_str!("../lib/fonts.wcl"));
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
    r.register("wdoc/project.wcl", include_str!("../lib/project.wcl"));
    r.register(
        "wdoc/diagram-core.wcl",
        include_str!("../lib/diagram-core.wcl"),
    );
    r.register("wdoc/templates.wcl", include_str!("../lib/templates.wcl"));
    r.register(
        "wdoc/presentation.wcl",
        include_str!("../lib/presentation.wcl"),
    );
    r.register("wdoc/website.wcl", include_str!("../lib/website.wcl"));
    r.register("wdoc/inline.wcl", include_str!("../lib/inline.wcl"));
    r.register(
        "wdoc/inline-patterns.wcl",
        include_str!("../lib/inline-patterns.wcl"),
    );
    r.register("wdoc/icons.wcl", include_str!("../lib/icons.wcl"));
    r.register("wdoc/image.wcl", include_str!("../lib/image.wcl"));
    r.register("wdoc/file.wcl", include_str!("../lib/file.wcl"));
    r.register("wdoc/include.wcl", include_str!("../lib/include.wcl"));
    r.register("wdoc/video.wcl", include_str!("../lib/video.wcl"));
    r.register("wdoc/tilemap.wcl", include_str!("../lib/tilemap.wcl"));
    r.register("wdoc/dopesheet.wcl", include_str!("../lib/dopesheet.wcl"));
    r.register("wdoc/map.wcl", include_str!("../lib/map.wcl"));
    r.register("wdoc/flowchart.wcl", include_str!("../lib/flowchart.wcl"));
    r.register("wdoc/charts.wcl", include_str!("../lib/charts.wcl"));
    r.register("wdoc/timeline.wcl", include_str!("../lib/timeline.wcl"));
    r.register("wdoc/card.wcl", include_str!("../lib/card.wcl"));
    r.register("wdoc/node_table.wcl", include_str!("../lib/node_table.wcl"));
    r.register("wdoc/tree.wcl", include_str!("../lib/tree.wcl"));
    r.register("wdoc/headings.wcl", include_str!("../lib/headings.wcl"));
    r.register(
        "wdoc/chapter_header.wcl",
        include_str!("../lib/chapter_header.wcl"),
    );
    r.register("wdoc/footnotes.wcl", include_str!("../lib/footnotes.wcl"));
    r.register("wdoc/p.wcl", include_str!("../lib/p.wcl"));
    r.register("wdoc/code.wcl", include_str!("../lib/code.wcl"));
    r.register(
        "wdoc/markdown_source.wcl",
        include_str!("../lib/markdown_source.wcl"),
    );
    r.register("wdoc/terminal.wcl", include_str!("../lib/terminal.wcl"));
    r.register("wdoc/tui.wcl", include_str!("../lib/tui.wcl"));
    r.register("wdoc/math.wcl", include_str!("../lib/math.wcl"));
    r.register("wdoc/typedoc.wcl", include_str!("../lib/typedoc.wcl"));
    r.register("wdoc/demo.wcl", include_str!("../lib/demo.wcl"));
    r.register("wdoc/sequence.wcl", include_str!("../lib/sequence.wcl"));
    r.register("wdoc/statechart.wcl", include_str!("../lib/statechart.wcl"));
    r.register("wdoc/visibility.wcl", include_str!("../lib/visibility.wcl"));
    r.register("wdoc/comment.wcl", include_str!("../lib/comment.wcl"));
    r.register(
        "wdoc/file_placement.wcl",
        include_str!("../lib/file_placement.wcl"),
    );
    r.register(
        "wdoc/edit_object.wcl",
        include_str!("../lib/edit_object.wcl"),
    );
    r.register("wdoc/edit_field.wcl", include_str!("../lib/edit_field.wcl"));
    // Guided answer mode. A top-level entry (not part of the wdoc prelude):
    // a data document opts in with `import <answer.wcl>` alone, without
    // pulling in the whole wdoc page/site vocabulary.
    r.register("answer.wcl", include_str!("../lib/answer.wcl"));
    r
}

/// The [`Environment`] every wdoc backend uses to open a document: the base
/// environment plus the `included_sites(options)` builtin. The builtin scans
/// a folder for sub-site entry points (see [`crate::include`]) and returns
/// `{ name, href, title, summary }` records a nav `wdoc_repeater` consumes;
/// it closes over `base_dir` so the scanned folder resolves relative to the
/// document, exactly like the `include` block it mirrors.
///
/// The single argument is an options record mirroring the `include` block's
/// fields, e.g. `included_sites({ folder: "…", entry: "main.wcl", site: "book" })`
/// — WCL has no keyword arguments, and a record keeps the call self-describing
/// and aligned with the block. (Record fields use `:`; block fields use `=`.)
pub fn wdoc_environment(base_dir: Option<&Path>) -> Environment {
    let mut env = Environment::new();
    let base = base_dir.map(Path::to_path_buf);
    env.add_builtin(
        "included_sites",
        from_fn(move |opts: Value| -> Result<Value, String> {
            // A malformed options record is a caller bug → eval error. A
            // valid-but-unscannable folder (e.g. not created yet) degrades to
            // an empty list; the build step is the authority on the set.
            let spec = include_spec_from_value(&opts)?;
            let sites = match crate::include::resolve_included(base.as_deref(), &spec) {
                Ok(sites) => sites,
                Err(_) => return Ok(Value::list(Vec::new())),
            };
            Ok(Value::list(
                sites
                    .iter()
                    .map(|s| {
                        let (title, summary) =
                            crate::include::read_entry_meta(&s.src_path, spec.site.as_deref());
                        let mut fields = BTreeMap::new();
                        fields.insert("name".to_string(), Value::Utf8(s.name.clone()));
                        fields.insert("href".to_string(), Value::Utf8(s.href.clone()));
                        fields.insert(
                            "title".to_string(),
                            Value::Utf8(title.unwrap_or_else(|| s.name.clone())),
                        );
                        fields.insert(
                            "summary".to_string(),
                            summary.map(Value::Utf8).unwrap_or(Value::None),
                        );
                        Value::record(Vec::new(), fields)
                    })
                    .collect(),
            ))
        })
        .doc(
            "Discover wdoc sub-sites under a folder for navigation (mirrors the `include` \
             block). Argument is an options record: `{ folder, pattern|entry, site? }`.",
        )
        .param(
            "options",
            "record",
            "`{ folder: utf8, pattern: utf8 | entry: utf8, site: utf8? }` — the same \
             options as the matching `include` block.",
        )
        .returns(
            "list",
            "One `{ name, href, title, summary }` record per discovered sub-site.",
        ),
    );
    env
}

/// Open `file` as an evaluated [`Document`] exactly the way [`build`] does —
/// the embedded wdoc schema registry plus the wdoc [`Environment`] — so callers
/// outside the build (the `wcl editor` save/locate pipeline, `wcl answer`)
/// introspect the same schemas (`@block` / `@table` / `@wdoc.file`) and
/// resolve the same block kinds the build sees. A plain `Document::from_file`
/// would miss the wdoc builtins and registry imports.
pub fn open_doc_for_edit(file: &Path) -> Result<Document, wcl_lang::ParseError> {
    // `ParseError` carries `#[from] io::Error`, so a read failure surfaces as
    // one error type alongside any syntax error.
    let user_src = fs::read_to_string(file)?;
    let name = file.display().to_string();
    let base_dir = file.parent().map(Path::to_path_buf);
    let loader = schema_registry().loader(disk_loader());
    Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &wdoc_environment(base_dir.as_deref()),
        loader,
    )
}

/// [`open_doc_for_edit`] with unsaved buffers shadowing disk: every import
/// (and the root file itself) resolves through `overlay` first, so the
/// `wcl editor` can validate or resolve against an edit without writing it.
/// Overlay keys should be canonical absolute paths (raw keys are accepted as
/// a fallback, matching [`wcl_lang::overlay_loader`]).
pub fn open_doc_for_edit_with_overlay(
    file: &Path,
    overlay: std::collections::HashMap<PathBuf, String>,
) -> Result<Document, wcl_lang::ParseError> {
    let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let user_src = match overlay.get(&canon).or_else(|| overlay.get(file)) {
        Some(s) => s.clone(),
        None => fs::read_to_string(file)?,
    };
    let name = file.display().to_string();
    let base_dir = file.parent().map(Path::to_path_buf);
    let loader = schema_registry().loader(wcl_lang::overlay_loader(overlay));
    Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &wdoc_environment(base_dir.as_deref()),
        loader,
    )
}

/// The entry document that owns `page_file` — a sub-site's entry `.wcl` when the
/// page belongs to an `include`d sub-site (e.g. a wskill book under the
/// top-level docs site), else `root_file` itself. Lets the `wcl editor`
/// introspect the schema/objects of the document a page actually came from
/// when it serves the top-level site. Falls back to `root_file` on any
/// resolution failure.
pub fn doc_entry_for_page(root_file: &Path, page_file: &Path) -> PathBuf {
    subsite_for_page(root_file, page_file)
        .map(|s| s.entry)
        .unwrap_or_else(|| root_file.to_path_buf())
}

/// The included sub-site that owns `page_file` — its entry document, on-disk
/// source root, and output subdirectory — or `None` when the page belongs to
/// the root document. Lets the dev server's `/__wdoc_rebuild` rebuild *only*
/// the sub-site a page lives in (e.g. one wskill) instead of the whole
/// top-level site.
pub struct PageSubSite {
    /// The sub-site entry `.wcl` to (re)build.
    pub entry: PathBuf,
    /// The sub-site's source directory; its pages' files live under it.
    pub src_root: PathBuf,
    /// Output subdirectory under the build root (`<prefix>/<name>`).
    pub out_subdir: PathBuf,
    /// The site selector for the sub-build (the include's `site` field).
    pub site: Option<String>,
}

pub fn subsite_for_page(root_file: &Path, page_file: &Path) -> Option<PageSubSite> {
    let doc = open_doc_for_edit(root_file).ok()?;
    let base = root_file.parent();
    let s = crate::include::entry_for_page(&doc, base, page_file)?;
    Some(PageSubSite {
        entry: s.src_path,
        src_root: s.src_root,
        out_subdir: s.out_subdir,
        site: s.site,
    })
}

/// The names of the `page` blocks `page_file` declares, in source order,
/// with `overlay` buffers shadowing disk — how `wcl editor` derives the
/// page its preview pane should render from the file the user is editing.
/// The document is opened through the sub-site entry that owns `page_file`
/// (like [`doc_entry_for_page`]), so a sub-site's pages resolve too.
///
/// When the file declares no pages directly (a component/data library, or
/// pages generated by a `wdoc_repeater`, which carry no per-file origin),
/// the owning document's own page list is returned instead so a preview can
/// still show *something*. Empty only when the document has no pages at all
/// or fails to open.
pub fn pages_in_file(
    root_file: &Path,
    page_file: &Path,
    overlay: &std::collections::HashMap<PathBuf, String>,
) -> Vec<String> {
    let entry = doc_entry_for_page(root_file, page_file);
    let Ok(doc) = open_doc_for_edit_with_overlay(&entry, overlay.clone()) else {
        return Vec::new();
    };
    let target = fs::canonicalize(page_file).unwrap_or_else(|_| page_file.to_path_buf());
    let entry_canon = fs::canonicalize(&entry).unwrap_or_else(|_| entry.clone());
    let mut own = Vec::new();
    for (origin, block) in doc.blocks_with_source() {
        if block.kind() != "page" {
            continue;
        }
        // Blocks declared in the entry itself report no origin.
        let matches = match origin {
            Some(p) => fs::canonicalize(p).map(|c| c == target).unwrap_or(false),
            None => entry_canon == target,
        };
        if matches && let Some(name) = page_name(&block) {
            own.push(name);
        }
    }
    if !own.is_empty() {
        return own;
    }
    match collect_pages(&doc) {
        Ok(pages) => pages.iter().filter_map(page_name).collect(),
        Err(_) => Vec::new(),
    }
}

/// Read an `included_sites(...)` options record into an [`IncludeSpec`].
/// Errors (eval errors — caller bugs) when the argument is not a record, has
/// no `folder`, or sets neither / both of `pattern` and `entry`.
fn include_spec_from_value(v: &Value) -> Result<crate::include::IncludeSpec, String> {
    let Value::Record { fields, .. } = v else {
        return Err("included_sites expects a record argument, e.g. \
             included_sites({ folder: \"projects\", pattern: \"main.wcl\" })"
            .to_string());
    };
    let get = |k: &str| -> Option<String> {
        match fields.get(k) {
            Some(Value::Utf8(s) | Value::Ascii(s)) => Some(s.clone()),
            _ => None,
        }
    };
    let folder = get("folder")
        .ok_or_else(|| "included_sites: the options record needs a `folder` field".to_string())?;
    let spec = crate::include::IncludeSpec {
        folder,
        pattern: get("pattern"),
        entry: get("entry"),
        site: get("site"),
        prefix: get("prefix"),
    };
    match (&spec.pattern, &spec.entry) {
        (Some(_), Some(_)) => {
            Err("included_sites: set exactly one of `pattern` or `entry`, not both".to_string())
        }
        (None, None) => Err(
            "included_sites: set one of `pattern` (recursive filename glob) or `entry` \
                 (a path inside each immediate subdirectory)"
                .to_string(),
        ),
        _ => Ok(spec),
    }
}

pub enum BuildError {
    Io(std::io::Error, String),
    Parse(Report),
    Schema(usize),
    /// A block expression failed to evaluate during rendering (e.g. an
    /// unresolved name in a page block). Carries a pre-built miette report
    /// with the source snippet attached.
    Eval(Report),
    BadPage(String),
    DuplicateId {
        page: String,
        id: String,
    },
    /// Two pages in the same site resolve to the same route/name (e.g. a
    /// `wdoc_repeater` whose interpolated page labels collide). Carries the
    /// site name (or "default" for an unnamed single site) and the route.
    DuplicatePage {
        site: String,
        name: String,
    },
    BadLink(Vec<String>),
    BadTemplate(String),
    Tileset(String),
    /// A diagram edge could not be routed around the intervening shapes
    /// (the layout is too tightly packed). Carries a message naming the
    /// offending edge and a hint at how to fix it.
    EdgeRouting(String),
    /// An `include` chain referenced one of its own ancestors, or nested
    /// deeper than [`MAX_INCLUDE_DEPTH`]. Carries a describing message.
    IncludeCycle(String),
}

impl BuildError {
    pub fn report(&self) {
        match self {
            Self::Io(e, ctx) => eprintln!("{ctx}: {e}"),
            Self::Parse(r) => eprintln!("{r:?}"),
            Self::Schema(n) => eprintln!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::Eval(r) => eprintln!("{r:?}"),
            Self::BadPage(msg) => eprintln!("{msg}"),
            Self::DuplicateId { page, id } => {
                eprintln!("page \"{page}\": duplicate id \"{id}\"");
            }
            Self::DuplicatePage { site, name } => {
                eprintln!("site \"{site}\": duplicate page \"{name}\"");
            }
            Self::BadLink(msgs) => {
                for m in msgs {
                    eprintln!("{m}");
                }
            }
            Self::BadTemplate(name) => eprintln!("unknown template \"{name}\""),
            Self::Tileset(msg) => eprintln!("{msg}"),
            Self::EdgeRouting(msg) => eprintln!("{msg}"),
            Self::IncludeCycle(msg) => eprintln!("{msg}"),
        }
    }

    /// Render this error to a plain string (no ANSI escapes), suitable
    /// for embedding outside a terminal — e.g. the dev server's
    /// build-failure page. `report()` keeps its colored stderr output.
    pub fn render_plain(&self) -> String {
        match self {
            Self::Parse(r) | Self::Eval(r) => {
                let mut s = String::new();
                let handler = miette::GraphicalReportHandler::new_themed(
                    miette::GraphicalTheme::unicode_nocolor(),
                );
                if handler.render_report(&mut s, r.as_ref()).is_err() {
                    s = format!("{r}");
                }
                s
            }
            Self::Io(e, ctx) => format!("{ctx}: {e}"),
            Self::Schema(n) => format!("{n} schema violation{}", if *n == 1 { "" } else { "s" }),
            Self::BadPage(msg) => msg.clone(),
            Self::DuplicateId { page, id } => format!("page \"{page}\": duplicate id \"{id}\""),
            Self::DuplicatePage { site, name } => {
                format!("site \"{site}\": duplicate page \"{name}\"")
            }
            Self::BadLink(msgs) => msgs.join("\n"),
            Self::BadTemplate(name) => format!("unknown template \"{name}\""),
            Self::Tileset(msg) => msg.clone(),
            Self::EdgeRouting(msg) => msg.clone(),
            Self::IncludeCycle(msg) => msg.clone(),
        }
    }

    /// Wrap a render-time evaluation failure into a `BuildError::Eval`,
    /// attaching the source file the error was raised against so the miette
    /// report renders the snippet against the correct text (a cross-file
    /// span won't line up with the root document's source).
    pub(crate) fn eval(err: wcl_lang::EvalError, src: NamedSource<String>) -> Self {
        let report = Report::new(err).with_source_code(src);
        Self::Eval(report)
    }
}

/// Block kinds the renderers dispatch **in Rust**, ignoring any
/// schema-declared `lower`. A root-authored `@block`/`@table`
/// re-declaring one of these would win schema validation (root-authored
/// declarations shadow imported ones) while the renderer keeps using
/// the built-in path — the user's schema and `lower` would be silently
/// dead. The build flags the re-declaration instead. Pure-WCL stdlib
/// kinds (`process`, `h1`, `callout`, …) are *not* listed: shadowing
/// them swaps in the user's `lower`, which is the designed extension
/// mechanism. Wireframe widgets are covered by their `wf_` prefix.
const RUST_DISPATCHED_KINDS: &[&str] = &[
    // Page-level blocks (render/html.rs + pdf/collect.rs + markdown/emit.rs).
    "column",
    "fragment",
    "li",
    "list",
    "table",
    "image",
    "file",
    "video",
    "diagram",
    "sequence_diagram",
    "state_diagram",
    "terminal",
    "demo",
    // Structural / infra kinds every backend treats specially.
    "page",
    "site",
    "include",
    "partial",
    "collect",
    "notes",
    "frontmatter",
    "wdoc_component",
    "wdoc_repeater",
    "wdoc_instance",
    "wdoc_content",
    "wdoc_slot",
    "wdoc_body",
    // Diagram shapes special-cased in render/svg/shapes.rs.
    "rect",
    "circle",
    "line",
    "label",
    "polygon",
    "container",
    "boundary",
    "card",
    "node_table",
    "tree",
    "timeline",
    "map",
    "tilemap",
    "dopesheet",
];

/// Errors for every root-authored `@block`/`@table` declaration whose
/// kind the renderer dispatches in Rust. Shared by the HTML / PDF /
/// Markdown entry points, which run it right after `schema_errors`.
pub(crate) fn reserved_kind_errors(doc: &Document) -> Vec<Report> {
    use wcl_lang::DeclName;
    let mut out = Vec::new();
    for kind in RUST_DISPATCHED_KINDS {
        let Some(t) = doc.block_schema(kind).or_else(|| doc.table_schema(kind)) else {
            continue;
        };
        if t.is_imported() {
            continue;
        }
        let span = t.span();
        out.push(miette::miette!(
            labels = vec![miette::LabeledSpan::at(
                span.start..span.end,
                "declared here"
            )],
            code = "wdoc::reserved_kind",
            "type '{}' re-declares the built-in kind \"{kind}\" — the renderer \
             dispatches this kind in Rust, so the schema and its `lower` would be \
             silently ignored; pick a different kind name",
            t.name(),
        ));
    }
    out
}

/// Emit a build-progress line to stderr, only when stderr is a
/// terminal — an interactive `wcl wdoc build` (and `wdoc serve`) can
/// tell a slow build from a stuck one, while tests, CI, and piped
/// output stay clean.
fn progress(line: std::fmt::Arguments<'_>) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!("{line}");
    }
}

pub fn build(file: &Path, out_dir: &Path, site_filter: Option<&str>) -> Result<usize, BuildError> {
    build_with_options(file, out_dir, site_filter, &BuildOptions::default()).map(|(n, _)| n)
}

/// Options for [`build_with_options`]. `Default` matches plain [`build`].
#[derive(Default)]
pub struct BuildOptions {
    /// Record a call-tree profile of the document evaluation driving the
    /// build; the snapshot is returned alongside the page count.
    pub profile: bool,
    /// Comment mode (the `wcl editor` preview build): stamp each rendered
    /// block with `data-wcl-*` anchors so the editor's comment UI can attach
    /// review comments. Off for normal builds — no markup leaks.
    pub comment_mode: bool,
    /// Edit mode (the `wcl editor` preview build): stamp each rendered block
    /// with its source `data-wcl-span` / `data-wcl-file` (plus the shared
    /// `data-wcl-*` block anchors) and render `edit_object` buttons, so the
    /// editor can map a rendered block back to the source that declares it.
    /// Off for normal builds.
    pub edit_mode: bool,
    /// Render every block regardless of its `@only` / `@except` visibility
    /// (the editor's merged "all views" preview). Combined with `edit_mode`,
    /// each block's anchor additionally stamps its visibility metadata
    /// (`data-wcl-except` / `data-wcl-vis`) so the client can draw per-view
    /// indicators. Off for normal builds.
    pub all_sites: bool,
    /// Unsaved buffers shadowing disk (the `wcl editor` preview): every
    /// source read — the root file and all imports — resolves through this
    /// map first. Keys should be canonical absolute paths.
    pub overlay: Option<std::collections::HashMap<PathBuf, String>>,
    /// Render only these page names, skipping the changed-file analysis — the
    /// preview path re-renders just the page being looked at into an
    /// already-warm output dir. A page-set change still falls back to a full
    /// render (the targeted path bails via `need_full`).
    pub page_filter: Option<HashSet<String>>,
}

/// [`build`] with [`BuildOptions`]. Returns the page count plus, when
/// profiling was requested, the evaluation profile snapshot.
pub fn build_with_options(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
    opts: &BuildOptions,
) -> Result<(usize, Option<wcl_lang::Profile>), BuildError> {
    let mut seen = HashSet::new();
    let (outcome, profile) = build_guarded(file, out_dir, site_filter, opts, None, &mut seen, 0)?;
    Ok((outcome.pages(), profile))
}

/// Site-relative path of the per-site page manifest a full build writes
/// (`{"start": …, "pages": […]}`) — the map from a built `<name>.html`
/// back to its page name that the editor's lazy per-page preview rebuild
/// reads between full builds.
pub const PAGES_MANIFEST_HREF: &str = "_wdoc/pages.json";

/// Outcome of an incremental rebuild attempt ([`build_incremental`]).
pub enum RebuildOutcome {
    /// A full site rebuild ran — the safe fallback, identical to
    /// [`build_with_options`]. Carries the page count.
    Full { pages: usize },
    /// Only the listed pages were re-rendered in place; the prior full
    /// build's shared site-wide artifacts (icon sprite, search index, the
    /// CSS embedded per page) were left untouched.
    Targeted { pages: Vec<String> },
}

/// Incremental rebuild for the dev server. Re-parses the document (imports
/// force this regardless), then — from `changed_paths` mapped onto block
/// origins — decides whether a targeted per-page re-render is safe. When it
/// is, only the affected `<name>.html` files are rewritten and the shared
/// site-wide artifacts are reused; otherwise it falls back to a full
/// [`build_with_options`] and returns [`RebuildOutcome::Full`].
///
/// The savings come from skipping the per-page render of unaffected pages
/// (and the aggregate sprite / search-index writes), not from skipping the
/// parse — a change in an imported library, the page set, the CSS, an asset
/// declaration, a repeater, or a newly-referenced icon all force a full
/// rebuild.
pub fn build_incremental(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
    opts: &BuildOptions,
    changed_paths: &[PathBuf],
) -> Result<RebuildOutcome, BuildError> {
    let mut seen = HashSet::new();
    let (outcome, _) = build_guarded(
        file,
        out_dir,
        site_filter,
        opts,
        Some(changed_paths),
        &mut seen,
        0,
    )?;
    Ok(match outcome {
        BuildOutcome::Full(pages) => RebuildOutcome::Full { pages },
        BuildOutcome::Targeted(pages) => RebuildOutcome::Targeted { pages },
    })
}

/// What a build pass actually did: a full render (page count) or a targeted
/// incremental re-render (the page names rewritten in place).
enum BuildOutcome {
    Full(usize),
    Targeted(Vec<String>),
}

impl BuildOutcome {
    /// The number of pages rendered, either way.
    fn pages(&self) -> usize {
        match self {
            BuildOutcome::Full(n) => *n,
            BuildOutcome::Targeted(names) => names.len(),
        }
    }
}

/// [`build_with_options`] wrapped in the `include` cycle / depth guard.
/// `seen` is the chain of documents currently being built (an ancestor
/// stack, pushed on entry and popped on exit) so a document that includes
/// its own ancestor is rejected, while the same document built in two
/// independent branches is still allowed.
fn build_guarded(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
    opts: &BuildOptions,
    changed: Option<&[PathBuf]>,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<(BuildOutcome, Option<wcl_lang::Profile>), BuildError> {
    let canon = crate::include::guard_enter(file, seen, depth)?;
    let result = build_inner(file, out_dir, site_filter, opts, changed, seen, depth);
    seen.remove(&canon);
    result
}

fn build_inner(
    file: &Path,
    out_dir: &Path,
    site_filter: Option<&str>,
    opts: &BuildOptions,
    changed: Option<&[PathBuf]>,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<(BuildOutcome, Option<wcl_lang::Profile>), BuildError> {
    // An overlay (the preview path) shadows disk for every read, including
    // the root file itself.
    let overlay_src = opts.overlay.as_ref().and_then(|o| {
        let canon = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        o.get(&canon).or_else(|| o.get(file)).cloned()
    });
    let user_src = match overlay_src {
        Some(s) => s,
        None => fs::read_to_string(file)
            .map_err(|e| BuildError::Io(e, format!("read {}", file.display())))?,
    };

    let name = file.display().to_string();

    // The wdoc schema is pulled in by the author's own `import <wdoc.wcl>`
    // line, resolved through the embedded registry below. Relative
    // `import "./pages/foo.wcl"` statements resolve against the source
    // file's own directory, not the wdoc working directory — so disk
    // imports fall through to the disk loader with that base.
    let base_dir = file.parent().map(std::path::Path::to_path_buf);
    let loader = match &opts.overlay {
        Some(o) => schema_registry().loader(wcl_lang::overlay_loader(o.clone())),
        None => schema_registry().loader(disk_loader()),
    };
    let mut doc = Document::open_at_with_loader(
        &user_src,
        &name,
        base_dir.clone(),
        &wdoc_environment(base_dir.as_deref()),
        loader,
    )
    .map_err(|e| BuildError::Parse(Report::new(e)))?;
    if opts.profile {
        doc.enable_profiling();
    }
    let doc = doc;

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

    // Root-authored re-declarations of renderer-built-in kinds would be
    // silently dead (see `reserved_kind_errors`) — fail like a schema
    // violation instead.
    let reserved = reserved_kind_errors(&doc);
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

    // Resolve the sites to build. A document may declare several named
    // `site` blocks; each renders into its own subdirectory. With one
    // site (or `--site`) the chosen site renders flat at `out_dir`, and
    // with none a synthetic default site reproduces the bare flat output.
    let site_blocks: Vec<Block> = doc.blocks().filter(|b| b.kind() == "site").collect();
    let all_pages = collect_pages(&doc)?;
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
            if chosen.iter().any(|s| is_skill_site(s)) {
                return Err(BuildError::BadPage(format!(
                    "site \"{want}\" is a skill (`default_template = :ai_skill`) — \
                     build it with `wcl wdoc skill`"
                )));
            }
            chosen
        }
        // Skill sites are a separate target — skip them in the HTML build.
        None => specs.iter().filter(|s| !is_skill_site(s)).collect(),
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

    // Incremental dev-server path: when the caller passed the set of changed
    // files, try to re-render only the page(s) they touch instead of the
    // whole site. `affected_pages` returns `None` — fall through to the full
    // rebuild below — for any change that could invalidate shared state (an
    // imported library, the page set, CSS, an asset declaration, a repeater).
    // An explicit `page_filter` (the preview path) names the targets directly.
    let targets = match (&opts.page_filter, changed) {
        (Some(pf), _) => Some(pf.clone()),
        (None, Some(changed)) => affected_pages(&doc, file, changed),
        (None, None) => None,
    };
    if let Some(targets) = targets {
        let _ = crate::render::take_route_error();
        let _ = crate::render::take_render_warnings();
        let (result, eval_err) =
            crate::render::scoped_eval_errors(|| -> Result<Option<Vec<String>>, BuildError> {
                let mut rendered = Vec::new();
                for spec in &build_set {
                    // Only this site's pages that the change actually touched.
                    let site_targets: HashSet<String> = spec
                        .pages
                        .iter()
                        .filter_map(page_name)
                        .filter(|n| targets.contains(n))
                        .collect();
                    if site_targets.is_empty() {
                        continue;
                    }
                    let (site_out, current_prefix, home_href, home_title) =
                        site_layout(spec, out_dir, multi, root_site.as_deref(), &root_title);
                    fs::create_dir_all(&site_out).map_err(|e| {
                        BuildError::Io(e, format!("create_dir_all {}", site_out.display()))
                    })?;
                    let built = build_site(
                        &doc,
                        base_dir.as_deref(),
                        spec,
                        &site_out,
                        current_prefix,
                        &site_pages,
                        &site_prefix,
                        &home_href,
                        &home_title,
                        opts.comment_mode,
                        opts.edit_mode,
                        opts.all_sites,
                        Some(&site_targets),
                    )?;
                    if built.need_full {
                        // A targeted render reached shared state (a new icon, or
                        // a presentation deck) — give up and full-rebuild.
                        return Ok(None);
                    }
                    // Re-copy the landing `index.html` when the start page was
                    // among those re-rendered.
                    if let Some(start) = site_start_page(spec)?
                        && start != "index"
                        && site_targets.contains(&start)
                    {
                        let src = site_out.join(format!("{start}.html"));
                        let dst = site_out.join("index.html");
                        fs::copy(&src, &dst).map_err(|e| {
                            BuildError::Io(e, format!("copy {} to index.html", src.display()))
                        })?;
                    }
                    rendered.extend(built.rendered);
                }
                Ok(Some(rendered))
            });
        if let Some((e, src)) = eval_err {
            return Err(BuildError::eval(e, src));
        }
        if let Some(msg) = crate::render::take_route_error() {
            return Err(BuildError::EdgeRouting(msg));
        }
        if let Some(rendered) = result? {
            return Ok((BuildOutcome::Targeted(rendered), doc.profile()));
        }
        // `need_full` ⇒ fall through to the full build below.
    }

    // Clear any routing error / render warnings stranded by an earlier build
    // (e.g. a previous `wcl wdoc serve` pass) so stale messages can't leak
    // into this one. Render warnings are left in the sink after a successful
    // build for the caller to drain via [`take_render_warnings`].
    let _ = crate::render::take_route_error();
    let _ = crate::render::take_render_warnings();
    let (result, eval_err) = crate::render::scoped_eval_errors(|| -> Result<usize, BuildError> {
        let mut count = 0;
        for spec in &build_set {
            progress(format_args!(
                "site {} ({} pages)",
                spec.name.as_deref().unwrap_or("site"),
                spec.pages.len()
            ));
            let (site_out, current_prefix, home_href, home_title) =
                site_layout(spec, out_dir, multi, root_site.as_deref(), &root_title);
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
                opts.comment_mode,
                opts.edit_mode,
                opts.all_sites,
                None,
            )?
            .count;
            // Landing page: a page marked `start` is copied to this site's
            // `index.html`, so `/` (or `/<site>/`) serves it without needing
            // a page literally named `index`. The page also stays reachable
            // at its own `<name>.html`.
            if let Some(start) = site_start_page(spec)?
                && start != "index"
            {
                let src = site_out.join(format!("{start}.html"));
                let dst = site_out.join("index.html");
                fs::copy(&src, &dst).map_err(|e| {
                    BuildError::Io(e, format!("copy {} to index.html", src.display()))
                })?;
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
    });
    if let Some((e, src)) = eval_err {
        return Err(BuildError::eval(e, src));
    }
    // An unroutable diagram edge surfaces after the eval check (an eval
    // failure is the more fundamental problem); the router only runs once a
    // block has otherwise evaluated and laid out.
    if let Some(msg) = crate::render::take_route_error() {
        return Err(BuildError::EdgeRouting(msg));
    }
    let mut count = result?;
    // Included sub-sites: build each discovered document independently into
    // its own subdirectory of the output (see `crate::include`). Run only
    // after the parent renders cleanly, so a parent failure surfaces first.
    // The directories occupied by non-root sites are reserved so an include
    // can't clobber a sibling site's output.
    let reserved_dirs: BTreeSet<String> = specs
        .iter()
        .filter_map(|s| s.name.clone())
        .filter(|n| Some(n) != root_site.as_ref())
        .collect();
    count += build_includes(
        &doc,
        base_dir.as_deref(),
        out_dir,
        &reserved_dirs,
        opts,
        seen,
        depth,
    )?;
    // `profile()` is `None` unless `opts.profile` enabled collection.
    Ok((BuildOutcome::Full(count), doc.profile()))
}

/// Decide whether a set of `changed` source files can be served by
/// re-rendering only the page(s) they touch, returning those page names —
/// or `None` to fall back to a full rebuild.
///
/// The decision is purely structural and conservative: a change is targetable
/// only when every changed file maps, through block origins
/// ([`Document::blocks_with_source`]), to `page` blocks *and nothing else*.
/// A changed file that declares a `site` / `class` / `stylesheet` / `iconset`
/// / `component` (or any non-page block), a top-level `wdoc_repeater` (whose
/// generated page set may shift), or that declares no top-level block at all
/// (a pure `fn` / `type` helper library imported for its definitions) all
/// force `None`. Because the change is then localized to one or more pages'
/// own content, the page set, the per-page-embedded CSS, and the asset
/// registries are unchanged by construction — no cross-build state is needed.
fn affected_pages(doc: &Document, file: &Path, changed: &[PathBuf]) -> Option<HashSet<String>> {
    if changed.is_empty() {
        return None;
    }
    // Canonicalize every changed path; one we can't resolve (e.g. a deleted
    // file) forces a full rebuild. The root document's blocks report no
    // origin, so they map to `file`.
    let mut changed_canon: HashSet<PathBuf> = HashSet::new();
    for p in changed {
        changed_canon.insert(fs::canonicalize(p).ok()?);
    }
    let root_canon = fs::canonicalize(file).ok()?;

    let mut targets: HashSet<String> = HashSet::new();
    let mut matched: HashSet<PathBuf> = HashSet::new();
    for (origin, block) in doc.blocks_with_source() {
        // Disk imports already store canonical paths; a synthetic system
        // import (the wdoc stdlib) won't canonicalize and is simply skipped
        // (it's never on disk in the watched tree, so never `changed`).
        let origin_canon = match origin {
            Some(p) => match fs::canonicalize(p) {
                Ok(c) => c,
                Err(_) => continue,
            },
            None => root_canon.clone(),
        };
        if !changed_canon.contains(&origin_canon) {
            continue;
        }
        matched.insert(origin_canon);
        match block.kind() {
            "page" => match page_name(&block) {
                Some(n) => {
                    targets.insert(n);
                }
                None => return None,
            },
            // Any non-page top-level block sharing a changed file could shift
            // site-wide state — fall back to a full rebuild.
            _ => return None,
        }
    }
    // Every changed file must have contributed at least one page block. A
    // changed file with no matched top-level block isn't safe to isolate.
    if matched.len() != changed_canon.len() || targets.is_empty() {
        return None;
    }
    Some(targets)
}

/// Build the sub-sites declared by every `include` block into `out_dir`,
/// returning the total pages written. Each match is built independently —
/// as if `wcl wdoc build` had been run on it — into
/// `<out_dir>/<folder-basename>/<name>/`, narrowed to the include's `site`
/// selector when set. The whole set is resolved and its output layout
/// validated (by [`crate::include::collect_includes`]) before anything builds.
fn build_includes(
    doc: &Document,
    base_dir: Option<&Path>,
    out_dir: &Path,
    reserved_dirs: &BTreeSet<String>,
    opts: &BuildOptions,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) -> Result<usize, BuildError> {
    let all = crate::include::collect_includes(doc, base_dir, reserved_dirs)?;
    let mut count = 0;
    for s in &all {
        progress(format_args!(
            "include {} -> {}",
            s.name,
            s.out_subdir.display()
        ));
        let (outcome, _) = build_guarded(
            &s.src_path,
            &out_dir.join(&s.out_subdir),
            s.site.as_deref(),
            opts,
            None,
            seen,
            depth + 1,
        )?;
        count += outcome.pages();
    }
    Ok(count)
}

/// Drain the non-fatal render warnings collected during the most recent
/// build/render pass on this thread: a diagram edge whose `source` /
/// `destination` named no rendered shape id, a block with no `lower`, an
/// image with no usable intrinsic size, … The render passes leave them
/// here rather than failing; the CLI / dev server drain and print them to
/// stderr after a successful run. A fresh pass clears any leftovers first,
/// so a stale warning can't leak.
pub fn take_render_warnings() -> Vec<String> {
    crate::render::take_render_warnings()
}

/// The name of the site marked `root = true`, if any. More than one root
/// site is a build error.
pub(crate) fn root_site_name(specs: &[SiteSpec<'_>]) -> Result<Option<String>, BuildError> {
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

/// Whether `spec` is a skill site (`default_template = :ai_skill`). Such a
/// site is built only by the skill target (`wcl wdoc skill`); the HTML /
/// PDF / Markdown targets skip it, so a single document can carry a web book
/// and a skill that share pages.
pub(crate) fn is_skill_site(spec: &SiteSpec<'_>) -> bool {
    spec.block.as_ref().is_some_and(is_skill_site_block)
}

/// Block-level form of [`is_skill_site`], shared with site enumeration.
pub(crate) fn is_skill_site_block(block: &Block<'_>) -> bool {
    field_symbol(block, "default_template").as_deref() == Some("ai_skill")
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
pub(crate) fn site_name(block: &Block<'_>) -> Option<String> {
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
/// `forge` default) is spliced in between the library and user `class`
/// rules, so it overrides the built-in defaults (chart palette, syntax
/// tokens) while user `class` blocks still win. `site_block` is the
/// `@block("site")` carrying the selection (`None` ⇒ bare/unthemed).
/// The four CSS buckets `site_css` assembles — `stylesheet` text and
/// rendered `class` rules, split by library (embedded-stdlib) vs user
/// origin so the colour theme can be spliced between them.
#[derive(Default)]
struct CssBuckets {
    lib_sheets: Vec<String>,
    user_sheets: Vec<String>,
    lib_classes: Vec<String>,
    user_classes: Vec<String>,
}

/// Collect a top-level block's CSS contribution into `css`. A `stylesheet`
/// or `class` block deposits directly; a generator (`wdoc_repeater`,
/// `wdoc_instance`, or a `wdoc_component` instance) is expanded and its
/// generated blocks collected recursively — so a repeater driven by data
/// can emit `class` blocks (the "repeater anywhere" hook for design-system
/// classes). `is_lib` is the origin, carried through expansion. Non-CSS,
/// non-generator blocks (pages, etc.) contribute nothing, exactly as before.
fn collect_css_block(b: &Block<'_>, is_lib: bool, css: &mut CssBuckets) {
    match b.kind() {
        "stylesheet" => {
            if let Some(text) = field_utf8(b, "css") {
                if is_lib {
                    &mut css.lib_sheets
                } else {
                    &mut css.user_sheets
                }
                .push(text);
            }
        }
        "class" => {
            if let Some(rule) = render_class(b) {
                if is_lib {
                    &mut css.lib_classes
                } else {
                    &mut css.user_classes
                }
                .push(rule);
            }
        }
        _ if b.binding_scope_depth() > MAX_LOWER_DEPTH => {}
        "wdoc_repeater" => {
            for c in expand_repeater_children(b) {
                collect_css_block(&c, is_lib, css);
            }
        }
        "wdoc_instance" => {
            for c in expand_instance_children(b) {
                collect_css_block(&c, is_lib, css);
            }
        }
        kind => {
            if let Some(def) = b.doc().component_def(kind) {
                for c in expand_component_children(b, &def) {
                    collect_css_block(&c, is_lib, css);
                }
            }
        }
    }
}

fn site_css(doc: &Document, site_name: Option<&str>, site_block: Option<&Block<'_>>) -> String {
    let mut css = CssBuckets::default();
    for (origin, b) in doc.blocks_with_source() {
        if !block_in_site(&b, site_name) {
            continue;
        }
        // `origin.is_some()` marks a library (embedded-stdlib) block; user
        // source has no origin. Carried through generator expansion so a
        // user `wdoc_repeater` that emits `class` blocks still lands in the
        // user bucket (and thus wins over library defaults).
        collect_css_block(&b, origin.is_some(), &mut css);
    }
    let CssBuckets {
        lib_sheets,
        user_sheets,
        lib_classes,
        user_classes,
    } = css;
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

/// The output dir, URL prefix, and home back-link for one site within the
/// overall layout — shared by the full per-site loop and the incremental
/// targeted path so both place a site's files identically. `at_root` (a
/// single/`--site` build, or the `root` site) renders flat at `out_dir`;
/// every other site renders under `<out_dir>/<name>/` and gets a back-link
/// to the root index.
fn site_layout(
    spec: &SiteSpec<'_>,
    out_dir: &Path,
    multi: bool,
    root_site: Option<&str>,
    root_title: &str,
) -> (PathBuf, String, String, String) {
    let at_root = !multi || (root_site.is_some() && spec.name.as_deref() == root_site);
    let (site_out, current_prefix) = if at_root {
        (out_dir.to_path_buf(), String::new())
    } else {
        let name = spec.name.as_deref().unwrap_or("site");
        (out_dir.join(name), format!("{name}/"))
    };
    let (home_href, home_title) = if at_root || !multi {
        (String::new(), String::new())
    } else {
        ("../index.html".to_string(), root_title.to_string())
    };
    (site_out, current_prefix, home_href, home_title)
}

/// What [`build_site`] produced — the page count, the page names actually
/// rendered (for the incremental path's report), and whether a targeted
/// render had to bail to a full rebuild (`need_full`, e.g. a newly-used
/// icon the shared sprite lacks, or a presentation deck).
struct SiteBuild {
    count: usize,
    rendered: Vec<String>,
    need_full: bool,
}

/// Render one site's pages into `out_dir`. Everything that scopes to a
/// site — its template/title/toc, nav page list, link-resolution name
/// set, and `_wdoc/` assets — comes from `spec`, so each site is a
/// self-contained directory whose pages use plain relative `_wdoc/…`
/// references.
///
/// `target` drives the dev server's incremental path: `None` is a normal
/// full render (every page, all shared site-wide assets written); `Some`
/// re-renders only the named pages, reusing the prior full build's
/// aggregate artifacts (icon sprite, search index) rather than rewriting
/// them — page-local media is still copied, and a newly-used icon the
/// on-disk sprite lacks sets `need_full` so the caller falls back to a full
/// rebuild.
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
    comment_mode: bool,
    edit_mode: bool,
    all_sites: bool,
    target: Option<&HashSet<String>>,
) -> Result<SiteBuild, BuildError> {
    // A targeted incremental render reuses the prior full build's aggregate
    // site-wide assets (player scripts, default favicon, copied `assets/`
    // folders, the search index and icon sprite) rather than rewriting them.
    let write_shared = target.is_none();

    // The page <style>: bundled theme + stylesheets + class rules, scoped
    // to this site (global blocks plus those whose `sites` list names it).
    let css = site_css(doc, spec.name.as_deref(), spec.block.as_ref());

    // Terminal + pan/zoom assets, scoped to this site's pages, so a site
    // that uses neither pays nothing. The `uses_*` flags drive each page's
    // `<script>` tags, so they're computed regardless; only the asset
    // writes are skipped on the incremental path.
    let uses_terminals = spec.pages.iter().any(crate::terminal::uses_terminal);
    if uses_terminals && write_shared {
        write_terminal_assets(out_dir)?;
    }
    let uses_pan_zoom = spec.pages.iter().any(crate::render::uses_pan_zoom);
    let uses_map = spec.pages.iter().any(crate::render::uses_map);
    // A map drives the same viewBox camera as a pan/zoom diagram, so it
    // needs the pan/zoom player too — plus its own layer/card player.
    if (uses_pan_zoom || uses_map) && write_shared {
        write_asset(
            out_dir,
            "diagram-pan-zoom.js",
            crate::render::DIAGRAM_PAN_ZOOM_JS,
        )?;
    }
    if uses_map && write_shared {
        write_asset(out_dir, "wdoc-map.js", crate::render::WDOC_MAP_JS)?;
    }
    let uses_dopesheet = spec.pages.iter().any(crate::dopesheet::uses_dopesheet);
    if uses_dopesheet && write_shared {
        write_asset(
            out_dir,
            "dopesheet-player.js",
            crate::render::DOPESHEET_PLAYER_JS,
        )?;
    }
    let uses_video = spec.pages.iter().any(crate::video::uses_video);
    if uses_video && write_shared {
        write_asset(out_dir, "wdoc-video.js", crate::render::WDOC_VIDEO_JS)?;
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
    let search = spec
        .block
        .as_ref()
        .and_then(|b| field_bool(b, "search"))
        .unwrap_or(false);
    let toc_nodes: Vec<TocNode> = spec.block.as_ref().map(read_toc).unwrap_or_default();
    let menu_nodes: Vec<MenuNode> = spec.block.as_ref().map(read_menu).unwrap_or_default();
    let deck_nodes: Vec<DeckSectionNode> = spec.block.as_ref().map(read_deck).unwrap_or_default();
    let footer_nodes: Vec<FooterButtonNode> = spec
        .block
        .as_ref()
        .map(read_sidebar_footer)
        .unwrap_or_default();
    // A `presentation` site renders all its slides into one `index.html`
    // (a deck), rather than one file per page. The `deck` block supplies
    // the slide grid; `default_template = :presentation` selects it.
    let is_presentation = default_template.as_deref() == Some("presentation");

    // Ordered (name, href) list of this site's pages for template nav,
    // and the name set the inline link pattern resolves `[text](page)`
    // against — both scoped to the site, so nav lists only this site's
    // pages and links resolve within it.
    let pages: Vec<(String, String, String)> = spec
        .pages
        .iter()
        .filter_map(|p| {
            page_name(p).map(|n| {
                let title = page_h1_title(p).unwrap_or_else(|| n.clone());
                (n.clone(), format!("{n}.html"), title)
            })
        })
        .collect();
    let mut page_names: HashSet<String> = HashSet::new();
    for (n, _, _) in &pages {
        // Routes must be unique within a site — two `wdoc_repeater`
        // elements whose interpolated labels collide would otherwise
        // silently overwrite one `<name>.html` with another.
        if !page_names.insert(n.clone()) {
            return Err(BuildError::DuplicatePage {
                site: spec.name.clone().unwrap_or_else(|| "default".into()),
                name: n.clone(),
            });
        }
    }

    // The per-site page manifest (`_wdoc/pages.json`): the start page plus
    // the ordered page names. Written only on full builds — the targeted
    // path can't change the page set (guarded below) — so a consumer (the
    // editor's lazy per-page preview rebuild) can map a requested
    // `<name>.html` back to its page between full builds.
    if write_shared {
        let start = site_start_page(spec)?.unwrap_or_else(|| "index".to_string());
        let names: Vec<&str> = pages.iter().map(|(n, _, _)| n.as_str()).collect();
        let manifest = serde_json::json!({ "start": start, "pages": names });
        write_asset(out_dir, "pages.json", manifest.to_string())?;
    }

    // Incremental: a page added or removed shifts every other page's
    // template `pages` list (auto nav / prev-next), so a targeted render
    // can't stay isolated — detect it against the prior build's on-disk
    // pages and bail to a full rebuild.
    if target.is_some() && !page_set_matches_disk(out_dir, &page_names) {
        return Ok(SiteBuild {
            count: 0,
            rendered: Vec::new(),
            need_full: true,
        });
    }

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
    if let Some(missing) = footer_missing_page(&footer_nodes, &page_names) {
        return Err(BuildError::BadTemplate(format!(
            "sidebar_footer button links to unknown page \"{missing}\""
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
    let videos = crate::video::VideoRegistry::new(base_dir.map(Path::to_path_buf));
    let files = crate::file::FileRegistry::new(base_dir.map(Path::to_path_buf));
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
        videos,
        files,
        crate::inline::Backend::Html,
    );
    inline_patterns.set_site_context(spec.name.clone(), default_template.clone());
    // Wireframe (`wf_*`) elements bake from this site's UI theme.
    inline_patterns.set_ui_theme(crate::render::resolve_ui_theme(spec.block.as_ref()));
    inline_patterns.set_comment_mode(comment_mode);
    inline_patterns.set_edit_mode(edit_mode);
    inline_patterns.set_all_sites(all_sites);
    // The `markdown_source` block writes its Markdown's diagram SVGs here.
    inline_patterns.set_output_dir(out_dir.to_path_buf());

    // Resolve the site favicon once. A user `icon` path is resolved + copied
    // via the image registry (already copied after the page loop); an
    // external URL passes through. Absent ⇒ ship the embedded default.
    // Book typography (Source Serif 4 / IBM Plex Sans / JetBrains Mono) is
    // referenced by the always-bundled `wdoc-fonts` `@font-face` rules, so a
    // themed site needs the faces on disk. Written once with the other shared
    // assets; a bare (site-less) document renders unthemed and skips them.
    if spec.block.is_some() && write_shared {
        write_book_font_assets(out_dir)?;
    }

    let site_icon = spec.block.as_ref().and_then(|b| field_utf8(b, "icon"));
    let favicon = match &site_icon {
        Some(src) => inline_patterns.images().register(src).url,
        None => {
            if write_shared {
                write_default_favicon(out_dir)?;
            }
            format!("{}/favicon.svg", crate::terminal::ASSET_DIR)
        }
    };

    // Extra `<head>` assets a custom (e.g. `website`) layout pulls in:
    // `stylesheets` + `fonts` become `<link rel="stylesheet">`, `scripts`
    // become deferred `<script>`. Hrefs are emitted verbatim (escaped),
    // so they may be URLs, copied `assets`, or shipped `file`s.
    let head_extra = site_head_extra(spec.block.as_ref());

    // Folders copied verbatim into this site's output (e.g. a Vite `dist/`),
    // so a layout can reference externally-built CSS/JS by its output path.
    if write_shared {
        for entry in spec
            .block
            .as_ref()
            .map(|b| field_utf8_list(b, "assets"))
            .unwrap_or_default()
        {
            let src = match base_dir {
                Some(dir) => dir.join(&entry),
                None => PathBuf::from(&entry),
            };
            let dest = out_dir.join(&entry);
            copy_dir_all(&src, &dest)
                .map_err(|e| BuildError::Io(e, format!("copy assets folder {entry}")))?;
        }
    }

    // Everything the page-rendering paths read but never mutate, resolved
    // once and shared by the presentation and per-page paths below.
    let ctx = PageRenderCtx {
        doc,
        base_dir,
        spec,
        out_dir,
        css: &css,
        favicon: &favicon,
        head_extra: &head_extra,
        inline_patterns: &inline_patterns,
        default_template: default_template.as_deref(),
        site_title: site_title.as_deref(),
        theme_toggle,
        toc_nodes: &toc_nodes,
        menu_nodes: &menu_nodes,
        footer_nodes: &footer_nodes,
        deck_nodes: &deck_nodes,
        pages: &pages,
        home_href,
        home_title,
        players: PlayerScripts {
            terminals: uses_terminals,
            pan_zoom: uses_pan_zoom,
            map: uses_map,
            dopesheet: uses_dopesheet,
            video: uses_video,
            search,
        },
        search,
    };

    // A presentation deck renders all its slides into one `index.html`, so a
    // single-page edit can't be isolated — bail to a full rebuild.
    if is_presentation && target.is_some() {
        return Ok(SiteBuild {
            count: 0,
            rendered: Vec::new(),
            need_full: true,
        });
    }

    // A `presentation` site renders all its slides into a single deck
    // `index.html`; every other site renders one file per page. On the
    // incremental path only the pages named in `target` are re-rendered.
    let mut search_entries: Vec<SearchEntry> = Vec::new();
    let mut rendered: Vec<String> = Vec::new();
    let count = if is_presentation {
        build_presentation_page(&ctx)?
    } else {
        let mut count = 0;
        let total = spec.pages.len();
        for (i, page) in spec.pages.iter().enumerate() {
            if let Some(want) = target
                && !page_name(page).is_some_and(|n| want.contains(&n))
            {
                continue;
            }
            progress(format_args!(
                "  page {}/{} {}",
                i + 1,
                total,
                page_name(page).unwrap_or_default()
            ));
            if let Some(entry) = build_normal_page(&ctx, page)? {
                search_entries.push(entry);
            }
            if let Some(n) = page_name(page) {
                rendered.push(n);
            }
            count += 1;
        }
        count
    };

    // The opt-in (`search = true`) site search: the client-side widget plus
    // the per-page text index it queries, both under `_wdoc/`. The index is
    // an aggregate over every page, so the incremental path can't rewrite it
    // from one page's entry — it's left from the prior full build (refreshed
    // on the next one).
    if search && write_shared {
        write_asset(out_dir, "wdoc-search.js", WDOC_SEARCH_JS)?;
        let entries: Vec<serde_json::Value> =
            search_entries.iter().map(SearchEntry::to_json).collect();
        let json = serde_json::Value::Array(entries).to_string();
        write_asset(out_dir, "search-index.json", json)?;
    }

    // Every icon resolved while rendering goes into one shared sprite
    // (`_wdoc/icons.svg`) that the pages reference via `<use>`. Written
    // after the page loop so it holds exactly the icons that were used.
    // The sprite is an aggregate over every page, so the incremental path
    // must not overwrite it with one page's subset: instead it verifies the
    // re-rendered page's icons are already in the on-disk sprite, falling
    // back to a full rebuild (`need_full`) when one is new.
    let mut need_full = false;
    if write_shared {
        if let Some(sprite) = inline_patterns.icons().build_sprite() {
            write_icon_sprite(out_dir, &sprite)?;
        }
    } else if !sprite_has_icons(out_dir, &inline_patterns.icons().used_ids()) {
        need_full = true;
    }

    // Page-local media: each referenced asset copies to its own file under
    // `_wdoc/`, never touching another page's, so the incremental path
    // copies them too (idempotent — only the re-rendered page's references).
    // Copy each spritesheet referenced by a rendered tilemap into
    // `_wdoc/`. No-op when the site used no tilemap.
    inline_patterns.tilesets().copy_used_images(out_dir)?;

    // Copy each local image referenced by a rendered `image` block (page
    // or diagram) into `_wdoc/`. No-op when none were used.
    inline_patterns.images().copy_used_images(out_dir)?;

    // Copy each local video file / poster referenced by a rendered `video`
    // block into `_wdoc/`. No-op when none were used.
    inline_patterns.videos().copy_used_assets(out_dir)?;

    // Copy each local file referenced by a rendered `file` block into its
    // `dir` (default `_wdoc/`). No-op when none were used.
    inline_patterns.files().copy_used(out_dir)?;

    // Inline `[text](page)` references that didn't resolve to a known
    // page in this site surface as a build error here. On the incremental
    // path this is scoped to the re-rendered pages — a broken link on an
    // unrendered page can't be newly introduced (its source didn't change).
    let link_errors = inline_patterns.take_link_errors();
    if !link_errors.is_empty() {
        return Err(BuildError::BadLink(link_errors));
    }

    Ok(SiteBuild {
        count,
        rendered,
        need_full,
    })
}

/// Whether the site's current page set matches the `<name>.html` files
/// already on disk (one per page, ignoring the `index.html` landing copy and
/// any page literally named `index`). The incremental path uses this to fall
/// back to a full rebuild when a page was added or removed — either shifts
/// every other page's template `pages` list (auto nav / prev-next).
fn page_set_matches_disk(site_out: &Path, page_names: &HashSet<String>) -> bool {
    let Ok(entries) = fs::read_dir(site_out) else {
        return false;
    };
    let mut on_disk: HashSet<String> = HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == "index.html" {
            continue;
        }
        if let Some(stem) = name.strip_suffix(".html") {
            on_disk.insert(stem.to_string());
        }
    }
    let expected: HashSet<String> = page_names
        .iter()
        .filter(|n| n.as_str() != "index")
        .cloned()
        .collect();
    on_disk == expected
}

/// Whether the on-disk shared icon sprite at `out_dir/_wdoc/icons.svg`
/// already defines a `<symbol>` for every `used` id (`{pack}-{name}`). Used
/// by the incremental path to detect a page edit that introduced an icon
/// the prior full build's sprite lacks. An absent sprite with no used icons
/// is trivially satisfied; an absent sprite with used icons is not.
fn sprite_has_icons(out_dir: &Path, used: &[String]) -> bool {
    if used.is_empty() {
        return true;
    }
    let Ok(text) = fs::read_to_string(out_dir.join(crate::icons::SPRITE_HREF)) else {
        return false;
    };
    used.iter().all(|id| text.contains(&format!("id=\"{id}\"")))
}

/// Which interactive players a site uses, so each page loads only the
/// `<script>` tags it needs. Computed once per site in [`build_site`].
#[derive(Clone, Copy)]
struct PlayerScripts {
    terminals: bool,
    pan_zoom: bool,
    map: bool,
    dopesheet: bool,
    video: bool,
    search: bool,
}

impl PlayerScripts {
    /// Append the `<script>` tags for the players this site uses. Loaded
    /// once per page; each no-ops on a page that doesn't use it. Shared
    /// verbatim by the per-page loop and the presentation deck.
    fn inject(self, body: &mut String) {
        if self.terminals {
            body.push_str("\n<script src=\"_wdoc/terminal-player.js\" defer></script>\n");
        }
        // A map drives the same viewBox camera as a pan/zoom diagram, so it
        // needs the pan/zoom player too — plus its own layer/card player.
        if self.pan_zoom || self.map {
            body.push_str("\n<script src=\"_wdoc/diagram-pan-zoom.js\" defer></script>\n");
        }
        if self.map {
            body.push_str("\n<script src=\"_wdoc/wdoc-map.js\" defer></script>\n");
        }
        if self.dopesheet {
            body.push_str("\n<script src=\"_wdoc/dopesheet-player.js\" defer></script>\n");
        }
        if self.video {
            body.push_str("\n<script src=\"_wdoc/wdoc-video.js\" defer></script>\n");
        }
        if self.search {
            body.push_str("\n<script src=\"_wdoc/wdoc-search.js\" defer></script>\n");
        }
    }
}

/// Per-site rendering context shared by [`build_presentation_page`] and
/// [`build_normal_page`] — everything [`build_site`] resolves once that the
/// page-rendering paths read but never mutate.
struct PageRenderCtx<'a> {
    doc: &'a Document,
    base_dir: Option<&'a Path>,
    spec: &'a SiteSpec<'a>,
    out_dir: &'a Path,
    css: &'a str,
    favicon: &'a str,
    // Extra `<head>` HTML from the site's `stylesheets` / `scripts` /
    // `fonts` fields, spliced into every page before `</head>`.
    head_extra: &'a str,
    inline_patterns: &'a InlinePatterns,
    default_template: Option<&'a str>,
    site_title: Option<&'a str>,
    theme_toggle: bool,
    toc_nodes: &'a [TocNode],
    menu_nodes: &'a [MenuNode],
    footer_nodes: &'a [FooterButtonNode],
    deck_nodes: &'a [DeckSectionNode],
    pages: &'a [(String, String, String)],
    home_href: &'a str,
    home_title: &'a str,
    players: PlayerScripts,
    search: bool,
}

/// Render a `presentation` site: every `slide` page becomes one section of
/// a single deck `index.html`, driven by the `presentation` template +
/// keyboard player. Returns the page count (always 1).
fn build_presentation_page(ctx: &PageRenderCtx<'_>) -> Result<usize, BuildError> {
    // Resolve each `slide` page to its rendered body + speaker notes,
    // building the `list<DeckSection>` the template lays out. Ids are
    // unique across the whole deck (it's one HTML document).
    let page_by_name: BTreeMap<String, &Block> = ctx
        .spec
        .pages
        .iter()
        .filter_map(|p| page_name(p).map(|n| (n, p)))
        .collect();
    let mut dup_seen = HashSet::new();
    let mut sections_val = Vec::new();
    for sec in ctx.deck_nodes {
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
                .filter_map(|b| render_block(ctx.doc, &b, ctx.inline_patterns, ctx.base_dir))
            {
                content.push_str(&b);
                content.push('\n');
            }
            // Speaker notes: the children of any `notes` block.
            let mut notes = String::new();
            for nb in page.blocks().filter(|b| b.kind() == "notes") {
                for cb in nb
                    .blocks()
                    .filter_map(|b| render_block(ctx.doc, &b, ctx.inline_patterns, ctx.base_dir))
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
                fields: std::sync::Arc::new(m),
            });
        }
        let mut sm = BTreeMap::new();
        sm.insert("title".to_string(), Value::Utf8(sec.title.clone()));
        sm.insert(
            "slides".to_string(),
            Value::List(std::sync::Arc::new(slides_val)),
        );
        sections_val.push(Value::Record {
            ty: vec!["DeckSection".to_string()],
            fields: std::sync::Arc::new(sm),
        });
    }

    let Some(tmpl) = find_template(ctx.doc, "presentation") else {
        return Err(BuildError::BadTemplate("presentation".into()));
    };
    let title = ctx
        .site_title
        .map(str::to_string)
        .or_else(|| ctx.spec.name.clone())
        .unwrap_or_else(|| "Presentation".to_string());
    let mut rendered = render_template(
        ctx.doc,
        &tmpl,
        "",
        // A presentation has no named regions; it lays out the `deck`.
        Value::List(std::sync::Arc::new(Vec::new())),
        &title,
        "",
        ctx.pages,
        ctx.toc_nodes,
        ctx.menu_nodes,
        ctx.footer_nodes,
        Value::List(std::sync::Arc::new(sections_val)),
        ctx.theme_toggle,
        ctx.home_href,
        ctx.home_title,
        ctx.search,
        ctx.inline_patterns,
    );
    // The slides may use the same interactive assets a normal page can.
    ctx.players.inject(&mut rendered.body);
    // The deck keyboard-navigation player.
    write_asset(
        ctx.out_dir,
        "presentation.js",
        crate::render::PRESENTATION_PLAYER_JS,
    )?;
    rendered
        .body
        .push_str("\n<script src=\"_wdoc/presentation.js\" defer></script>\n");
    let head = format!("{}{}", ctx.head_extra, rendered.head);
    let html = render_page(&title, ctx.css, &rendered.body, Some(ctx.favicon), &head);
    let out_path = ctx.out_dir.join("index.html");
    fs::write(&out_path, html)
        .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
    Ok(1)
}

/// A page's first `h1` label, used as its display title in the
/// no-`toc` navigation fallback (and anywhere else a human-facing page
/// title is wanted without rendering the page).
fn page_h1_title(page: &Block<'_>) -> Option<String> {
    page.blocks()
        .find(|b| b.kind() == "h1")
        .and_then(|b| label_string(&b))
        .filter(|t| !t.is_empty())
}

/// Render one ordinary page to `<name>.html`. Returns the page's
/// search-index entry when the site has `search = true`.
fn build_normal_page(
    ctx: &PageRenderCtx<'_>,
    page: &Block<'_>,
) -> Result<Option<SearchEntry>, BuildError> {
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

    // Split the page's blocks into the default `content` part (everything
    // outside a `region`) and the named `region "name" { … }` regions,
    // each rendered separately so a template can slot them independently.
    // A `region`'s inline label is its name; its children are wdoc blocks.
    let mut content = String::new();
    let mut regions: Vec<(String, String)> = Vec::new();
    for b in page.blocks() {
        if b.kind() == "region" {
            let name = label_string(&b).unwrap_or_default();
            let mut html = String::new();
            for cb in b
                .blocks()
                .filter_map(|c| render_block(ctx.doc, &c, ctx.inline_patterns, ctx.base_dir))
            {
                html.push_str(&cb);
                html.push('\n');
            }
            regions.push((name, html));
            continue;
        }
        if let Some(s) = render_block(ctx.doc, &b, ctx.inline_patterns, ctx.base_dir) {
            content.push_str(&s);
            content.push('\n');
        }
    }
    // Comment / edit mode (the `wcl editor` preview): wrap the page content
    // in a `display:contents` div carrying the page's source file (so the
    // client can locate the owning `comments.wcl` sidecar, or the file to
    // edit) and resolved name (the comment key). `display:contents` keeps it
    // invisible to layout.
    if ctx.inline_patterns.anchor_mode() {
        let src = page.named_source();
        // The page block's own span lets an editor client anchor edits to
        // the page itself; harmless for comment mode.
        let page_span = page.span();
        content = format!(
            "<div data-wcl-page-file=\"{}\" data-wcl-page-name=\"{}\" \
             data-wcl-page-span=\"{}:{}\" style=\"display:contents\">\n{content}</div>\n",
            escape_html(src.name()),
            escape_html(&page_name),
            page_span.start,
            page_span.end,
        );
    }
    let regions_val = Value::list(
        regions
            .iter()
            .map(|(name, html)| {
                let mut m = BTreeMap::new();
                m.insert("name".to_string(), Value::Utf8(name.clone()));
                m.insert("content".to_string(), Value::Utf8(html.clone()));
                Value::Record {
                    ty: vec!["Region".to_string()],
                    fields: std::sync::Arc::new(m),
                }
            })
            .collect(),
    );

    // Resolve the template: the page's own `template` overrides the
    // site `default_template`. None ⇒ render content bare.
    let template_name =
        field_symbol(page, "template").or_else(|| ctx.default_template.map(str::to_string));
    let mut rendered = match template_name {
        // `:ai_skill` is a Markdown-only target, not an HTML template.
        Some(name) if name == "ai_skill" => {
            return Err(BuildError::BadPage(
                "`default_template = :ai_skill` is a skill target, not an HTML template — \
                 build it with `wcl wdoc skill`"
                    .into(),
            ));
        }
        Some(name) => {
            let Some(tmpl) = find_template(ctx.doc, &name) else {
                return Err(BuildError::BadTemplate(name));
            };
            let title = ctx
                .site_title
                .map(str::to_string)
                .unwrap_or_else(|| page_name.clone());
            render_template(
                ctx.doc,
                &tmpl,
                &content,
                regions_val,
                &title,
                &page_name,
                ctx.pages,
                ctx.toc_nodes,
                ctx.menu_nodes,
                ctx.footer_nodes,
                Value::List(std::sync::Arc::new(Vec::new())),
                ctx.theme_toggle,
                ctx.home_href,
                ctx.home_title,
                ctx.search,
                ctx.inline_patterns,
            )
        }
        None => crate::render::Rendered {
            body: content.clone(),
            head: String::new(),
        },
    };
    ctx.players.inject(&mut rendered.body);
    // Browser tab title: the page's own `title` (else its name), suffixed
    // with the site title as `<page> — <site>` when the site sets one.
    let page_title = field_utf8(page, "title").unwrap_or_else(|| page_name.clone());
    let doc_title = match ctx.site_title {
        Some(st) if st != page_title.as_str() => format!("{page_title} — {st}"),
        _ => page_title,
    };
    // The site-level head assets come first, then any head the template
    // emitted (so a template can override the site's links).
    let head = format!("{}{}", ctx.head_extra, rendered.head);
    let html = render_page(
        &doc_title,
        ctx.css,
        &rendered.body,
        Some(ctx.favicon),
        &head,
    );

    let out_path = ctx.out_dir.join(format!("{page_name}.html"));
    fs::write(&out_path, html)
        .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
    if !ctx.search {
        return Ok(None);
    }
    // Index the page's own content (not the template shell, so nav
    // chrome doesn't match every query). The title is the first `h1`
    // when the page has one, else the page name.
    let text = html_to_text(&content);
    let title = first_h1_text(&content).unwrap_or_else(|| page_name.clone());
    Ok(Some(SearchEntry {
        href: format!("{page_name}.html"),
        title,
        text,
    }))
}

/// One page in the `search-index.json` a `search = true` site ships.
struct SearchEntry {
    href: String,
    title: String,
    text: String,
}

impl SearchEntry {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "href": self.href,
            "title": self.title,
            "text": self.text,
        })
    }
}

/// Plain text of rendered HTML for the search index: tags dropped,
/// `<script>` / `<style>` contents skipped (SVG text nodes — diagram
/// labels — survive, which is wanted), whitespace collapsed.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4);
    let mut rest = html;
    let mut last_ws = true;
    while let Some(lt) = rest.find('<') {
        for ch in rest[..lt].chars() {
            if ch.is_whitespace() {
                if !last_ws {
                    out.push(' ');
                    last_ws = true;
                }
            } else {
                out.push(ch);
                last_ws = false;
            }
        }
        rest = &rest[lt..];
        let lower = rest.get(..8).unwrap_or("").to_ascii_lowercase();
        let skip_to = if lower.starts_with("<script") {
            Some("</script>")
        } else if lower.starts_with("<style") {
            Some("</style>")
        } else {
            None
        };
        if let Some(close) = skip_to {
            match rest.to_ascii_lowercase().find(close) {
                Some(end) => rest = &rest[end + close.len()..],
                None => break,
            }
            continue;
        }
        match rest.find('>') {
            Some(gt) => rest = &rest[gt + 1..],
            None => break,
        }
    }
    for ch in rest.chars() {
        if ch.is_whitespace() {
            if !last_ws {
                out.push(' ');
                last_ws = true;
            }
        } else {
            out.push(ch);
            last_ws = false;
        }
    }
    // Decode the entities the HTML emitters produce, so the index (and
    // the widget, which sets textContent) shows `&`, not `&amp;`.
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.trim().to_string()
}

/// The text of the page's first top-level heading, if any. Headings
/// lower to `<p class="heading-1">…</p>`; raw-HTML `<h1>` is the
/// fallback.
fn first_h1_text(html: &str) -> Option<String> {
    if let Some(start) = html.find("heading-1")
        && let Some(open_end) = html[start..].find('>').map(|i| start + i + 1)
        && let Some(close) = html[open_end..].find("</p>").map(|i| open_end + i)
    {
        let text = html_to_text(&html[open_end..close]);
        if !text.is_empty() {
            return Some(text);
        }
    }
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<h1")?;
    let open_end = html[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</h1>")? + open_end;
    let text = html_to_text(&html[open_end..close]);
    if text.is_empty() { None } else { Some(text) }
}

/// The bundled client-side search widget (see `assets/wdoc-search.js`).
const WDOC_SEARCH_JS: &str = include_str!("../assets/wdoc-search.js");

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
    // The multi-site landing page gets the default favicon (no single site
    // owns it), written into the root `_wdoc/`.
    write_default_favicon(out_dir)?;
    let favicon = format!("{}/favicon.svg", crate::terminal::ASSET_DIR);
    let html = render_page("index", css, &body, Some(&favicon), "");
    let path = out_dir.join("index.html");
    fs::write(&path, html).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    Ok(())
}

/// Build the verbatim `<head>` HTML for a site's `stylesheets` / `fonts`
/// (`<link rel="stylesheet">`) and `scripts` (deferred `<script>`) fields.
/// Hrefs are HTML-escaped but otherwise emitted as authored, so they may be
/// URLs, paths under a copied `assets` folder, or shipped `file`s. Empty
/// when the site (or its block) declares none.
fn site_head_extra(site: Option<&Block<'_>>) -> String {
    let Some(site) = site else {
        return String::new();
    };
    let mut out = String::new();
    for href in field_utf8_list(site, "stylesheets")
        .iter()
        .chain(field_utf8_list(site, "fonts").iter())
    {
        out.push_str(&format!(
            "<link rel=\"stylesheet\" href=\"{}\">",
            escape_html(href)
        ));
    }
    for src in field_utf8_list(site, "scripts") {
        out.push_str(&format!(
            "<script src=\"{}\" defer></script>",
            escape_html(&src)
        ));
    }
    out
}

/// Recursively copy the directory tree at `src` into `dest`, creating
/// `dest` (and parents) as needed. Used to ship a site's `assets` folders
/// (an externally-built `dist/`, etc.) verbatim into the output.
fn copy_dir_all(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Write one bundled asset `bytes` to `<out>/_wdoc/<name>`, creating the
/// `_wdoc/` directory if needed. The single create-dir + write + error-map
/// path every bundled-asset writer shares (players, fonts, favicon, sprite).
fn write_asset(out_dir: &Path, name: &str, bytes: impl AsRef<[u8]>) -> Result<(), BuildError> {
    let dir = out_dir.join(crate::terminal::ASSET_DIR);
    fs::create_dir_all(&dir)
        .map_err(|e| BuildError::Io(e, format!("create_dir_all {}", dir.display())))?;
    let path = dir.join(name);
    fs::write(&path, bytes).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))?;
    Ok(())
}

/// Write the bundled terminal assets (the JetBrains Mono Nerd Font
/// faces + the replay player JS) into `<out>/_wdoc/`. Pages reference
/// them by relative URL, so the dev server and any static host resolve
/// them the same way.
fn write_terminal_assets(out_dir: &Path) -> Result<(), BuildError> {
    for (name, bytes) in crate::terminal::FONT_FILES {
        write_asset(out_dir, name, bytes)?;
    }
    write_asset(out_dir, "terminal-player.js", crate::terminal::PLAYER_JS)
}

/// Embedded book typography faces — Source Serif 4 (body), IBM Plex Sans
/// (headings), and JetBrains Mono (prose code) — written into `<out>/_wdoc/`
/// so the `wdoc-fonts` `@font-face` rules resolve. Distinct from the terminal
/// grid's `'JetBrainsMono Nerd Font'`; this is the plain `'JetBrains Mono'`
/// family. `(filename, bytes)`.
const BOOK_FONT_FILES: &[(&str, &[u8])] = &[
    (
        "SourceSerif4-Regular.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-Regular.woff2"),
    ),
    (
        "SourceSerif4-Medium.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-Medium.woff2"),
    ),
    (
        "SourceSerif4-SemiBold.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-SemiBold.woff2"),
    ),
    (
        "SourceSerif4-Bold.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-Bold.woff2"),
    ),
    (
        "SourceSerif4-Italic.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-Italic.woff2"),
    ),
    (
        "SourceSerif4-MediumItalic.woff2",
        include_bytes!("../assets/fonts/SourceSerif4-MediumItalic.woff2"),
    ),
    (
        "IBMPlexSans-Regular.woff2",
        include_bytes!("../assets/fonts/IBMPlexSans-Regular.woff2"),
    ),
    (
        "IBMPlexSans-Medium.woff2",
        include_bytes!("../assets/fonts/IBMPlexSans-Medium.woff2"),
    ),
    (
        "IBMPlexSans-SemiBold.woff2",
        include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.woff2"),
    ),
    (
        "IBMPlexSans-Bold.woff2",
        include_bytes!("../assets/fonts/IBMPlexSans-Bold.woff2"),
    ),
    (
        "JetBrainsMono-Regular.woff2",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.woff2"),
    ),
    (
        "JetBrainsMono-Medium.woff2",
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.woff2"),
    ),
    (
        "JetBrainsMono-SemiBold.woff2",
        include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.woff2"),
    ),
];

/// Write the bundled book typography faces into `<out>/_wdoc/`. The
/// `wdoc-fonts` stylesheet references them by relative URL, so the dev
/// server and any static host resolve them the same way.
fn write_book_font_assets(out_dir: &Path) -> Result<(), BuildError> {
    for (name, bytes) in BOOK_FONT_FILES {
        write_asset(out_dir, name, bytes)?;
    }
    Ok(())
}

/// The default favicon, shipped when a `site` sets no `icon`.
const DEFAULT_FAVICON: &str = include_str!("../assets/favicon.svg");

/// Write the default favicon into `<out>/_wdoc/favicon.svg`. Pages
/// reference it by relative URL, so the dev server and any static host
/// resolve it the same way. Idempotent (rewrites the same bytes).
fn write_default_favicon(out_dir: &Path) -> Result<(), BuildError> {
    write_asset(out_dir, "favicon.svg", DEFAULT_FAVICON)
}

/// Write the shared icon sprite into `<out>/_wdoc/icons.svg`. Pages
/// reference its `<symbol>`s by relative URL (`_wdoc/icons.svg#id`), so
/// the dev server and any static host resolve them the same way.
fn write_icon_sprite(out_dir: &Path, sprite: &str) -> Result<(), BuildError> {
    write_asset(out_dir, crate::icons::SPRITE_FILE, sprite)
}

/// Extract a page block's first label as a string identifier. The
/// page-name match for `[text](page)` cross-page links runs against
/// this set.
pub(crate) fn page_name(page: &Block<'_>) -> Option<String> {
    let labels = page.labels().ok()?;
    match labels.into_iter().next()? {
        Value::Identifier(s) | Value::Utf8(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

/// Discover the document's pages, expanding any document-level
/// `wdoc_repeater` (whose body is `page` blocks) into one concrete page
/// per element of its `each` list — the data-driven page generator.
/// Pages keep source order (each repeater expands in place); nested
/// repeaters recurse. The result is an ordinary page list every backend
/// (HTML / PDF / Markdown / skill) consumes unchanged, so the rest of the
/// pipeline (site grouping, link validation, rendering) needs no further
/// awareness of generation.
pub(crate) fn collect_pages(doc: &Document) -> Result<Vec<Block<'_>>, BuildError> {
    let mut out = Vec::new();
    collect_pages_into(doc.blocks(), &mut out);
    // A generated page's route is its interpolated label, which can be any
    // string — reject anything that wouldn't form a clean `<name>.html` /
    // link target. Static `identifier` labels always pass.
    for p in &out {
        if let Some(name) = page_name(p)
            && !is_slug_safe(&name)
        {
            return Err(BuildError::BadPage(format!(
                "page route \"{name}\" is not slug-safe — a generated page name \
                 must be non-empty and contain only [A-Za-z0-9_-]; build one with \
                 e.g. `to_lower(replace(s, \" \", \"-\"))`"
            )));
        }
    }
    Ok(out)
}

fn collect_pages_into<'a>(blocks: impl Iterator<Item = Block<'a>>, out: &mut Vec<Block<'a>>) {
    for b in blocks {
        match b.kind() {
            "page" => out.push(b),
            // Stop runaway expansion (mirrors the HTML / diagram guards);
            // then expand this repeater's `page` (and nested repeater)
            // children once per element of `each`.
            "wdoc_repeater" if b.binding_scope_depth() <= MAX_LOWER_DEPTH => {
                collect_pages_into(expand_repeater_children(&b).into_iter(), out);
            }
            _ => {}
        }
    }
}

/// Whether a page route forms a clean filename / link target.
fn is_slug_safe(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The name of the page marked `start = true` in this site, if any —
/// the page served when no page is specified (`/` or `/<site>/`).
/// Errors if more than one page in the site claims it.
pub(crate) fn site_start_page(spec: &SiteSpec<'_>) -> Result<Option<String>, BuildError> {
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

/// Return the first `sidebar_footer` button `page` reference that isn't a
/// known page name, in source order. External `href`s are not checked.
/// `None` if every page link resolves (or no button links a page).
fn footer_missing_page<'a>(
    nodes: &'a [FooterButtonNode],
    known: &HashSet<String>,
) -> Option<&'a str> {
    for n in nodes {
        if let Some(page) = &n.page
            && !known.contains(page)
        {
            return Some(page);
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
