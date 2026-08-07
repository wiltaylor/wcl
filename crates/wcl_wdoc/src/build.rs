use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use miette::{NamedSource, Report};
use wcl_lang::{
    Block, DeclName, Document, Environment, EvalError, Registry, TypeRef, Value, disk_loader,
    from_fn,
};

use crate::inline::InlinePatterns;
use crate::render::{
    CollectionTemplateInput, DeckSectionNode, FooterButtonNode, MAX_LOWER_DEPTH, MenuNode, TocNode,
    escape_html, expand_component_children, expand_instance_children, expand_repeater_children,
    field_bool, field_id, field_symbol, field_symbol_list_opt, field_utf8, field_utf8_list,
    find_template, flat_toc_to_value, footer_to_value, label_string, menu_to_value, pages_to_value,
    read_deck, read_menu, read_sidebar_footer, read_toc, render_block, render_css_block,
    render_page, render_template, site_theme_css, toc_to_value,
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
    r.register("wdoc/highlight.wcl", include_str!("../lib/highlight.wcl"));
    r.register("wdoc/theme.wcl", include_str!("../lib/theme.wcl"));
    r.register(
        "wdoc/theme-rules.wcl",
        include_str!("../lib/theme-rules.wcl"),
    );
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
    r.register("wdoc/el.wcl", include_str!("../lib/el.wcl"));
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
    r.register("wdoc/content.wcl", include_str!("../lib/content.wcl"));
    for extra in EXTRA_STDLIBS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
    {
        r.extend(extra.clone());
    }
    r
}

/// Embedded stdlibs layered onto [`schema_registry`] by whoever composes the
/// toolchain — `wcl_wskill`'s wskill library is the one in this workspace.
///
/// It is a slot rather than a parameter because this crate builds every
/// [`FileLoader`](wcl_lang::FileLoader) itself, at four call sites reached
/// from a hundred (`build`, [`open_doc_for_edit`], and
/// `include::read_entry_meta`); a caller cannot pass a registry in at the
/// point that matters without threading one through all of them. The value it
/// holds is compiled-in static text, identical for the whole process and
/// installed once at startup, so the slot behaves as a constant that a lower
/// layer is allowed not to know about.
///
/// The alternative — registering the wskill files here — is what §5.2 of the
/// wdoc-substrate spec rules out: `wcl_wdoc` must not name the wskill format.
static EXTRA_STDLIBS: std::sync::RwLock<Vec<Registry>> = std::sync::RwLock::new(Vec::new());

/// Layer `extra` onto every registry [`schema_registry`] builds from now on.
///
/// Idempotent by content, not by call: installing the same library twice
/// registers the same keys twice, which is a no-op (a later registration under
/// a key wins). Callers that must not double-install guard it themselves —
/// [`wcl_wskill::install_stdlib`] does.
pub fn install_stdlib(extra: Registry) {
    EXTRA_STDLIBS
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .push(extra);
}

/// The [`Environment`] every wdoc backend uses to open a document: the base
/// environment, the wdoc [`Expander`](wcl_lang::Expander) for `@contextual`
/// block kinds, plus the `included_sites(options)` builtin. The builtin scans
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
    // Without this every `@children` projection over a repeater /
    // component instance is a hard error — which is exactly the point:
    // a wdoc document must be opened with the wdoc environment.
    env.set_expander(std::sync::Arc::new(crate::render::WdocExpander));
    crate::page_metadata::register(&mut env);
    env.add_builtin(
        "__wdoc_slot",
        from_fn(|slots: Value, requested: Value, field: Value| -> Result<Value, String> {
            let requested = match requested {
                Value::Symbol(name)
                | Value::Identifier(name)
                | Value::Utf8(name)
                | Value::Ascii(name) => name,
                other => {
                    return Err(format!(
                        "slot name must be a symbol, found {}",
                        other.type_name()
                    ));
                }
            };
            let Value::List(slots) = slots else {
                return Err("template slot table is not a list".to_string());
            };
            let field = match field {
                Value::Symbol(name)
                | Value::Identifier(name)
                | Value::Utf8(name)
                | Value::Ascii(name) => name,
                other => {
                    return Err(format!(
                        "slot field must be a symbol, found {}",
                        other.type_name()
                    ));
                }
            };
            for slot in slots.iter() {
                let Value::Record { fields, .. } = slot else {
                    continue;
                };
                let matches = matches!(
                    fields.get("name"),
                    Some(Value::Symbol(name) | Value::Identifier(name) | Value::Utf8(name) | Value::Ascii(name))
                        if name == &requested
                );
                if matches {
                    return fields
                        .get(&field)
                        .cloned()
                        .ok_or_else(|| format!("template slot has no `{field}` field"));
                }
            }
            Err(format!(
                "template references slot `{requested}` but does not declare it"
            ))
        }),
    );
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
/// outside the build (`wcl wskill`'s model loader, the write pipeline)
/// introspect the same schemas (`@block` / `@table`) and
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

/// The included sub-site that owns `page_file` — its entry document, on-disk
/// source root, and output subdirectory — or `None` when the page belongs to
/// the root document. Lets the dev server's `/__wdoc_rebuild` rebuild *only*
/// the sub-site a page lives in instead of the whole
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

/// Every schema-level contract a document must satisfy before any backend
/// renders it: the wdoc stdlib is in scope, no root-authored re-declaration of
/// a Rust-dispatched kind ([`reserved_kind_errors`]), and a coherent rendering
/// declaration on every block type ([`crate::native::native_errors`]).
///
/// The three entry points (HTML / Markdown / PDF) call this one
/// function right after `schema_errors`, so a fourth cannot pick up half the
/// contract.
pub(crate) fn contract_errors(doc: &Document) -> Vec<Report> {
    if !crate::native::has_wdoc_native_declaration(doc) {
        return vec![miette::miette!(
            labels = vec![miette::LabeledSpan::at(0..0, "add the import here")],
            code = "wdoc::missing_stdlib",
            "this document does not import the wdoc stdlib — add `import <wdoc.wcl>`",
        )];
    }
    let mut out = reserved_kind_errors(doc);
    out.extend(crate::native::native_errors(doc));
    out
}

/// Generic schema validation with wdoc's locally-scoped bare fills removed
/// from the generic error stream. A bare fill deliberately has no global
/// `@block` schema: its meaning comes from the resolved layout or component,
/// and the corresponding build-time contract checks below own its
/// cardinality and accepted child type. Keeping this filtering in the host
/// prevents a slot named like an unrelated component from inheriting that
/// component's schema.
pub(crate) fn schema_errors(doc: &Document) -> Vec<EvalError> {
    fn span_key(block: &Block<'_>) -> (usize, usize) {
        let span = block.span();
        (span.start, span.end.saturating_sub(span.start))
    }

    fn content_slot_names(holder: &Block<'_>) -> HashSet<String> {
        holder
            .blocks()
            .filter(|block| block.kind() == "slot")
            .filter(|slot| {
                matches!(
                    slot.slot_type_ref(),
                    Some(TypeRef::Named { path, .. })
                        if path.last().is_some_and(|name| name == "content")
                )
            })
            .filter_map(|slot| label_string(&slot))
            .collect()
    }

    fn mark_layout_fills(
        block: &Block<'_>,
        names: &HashSet<String>,
        spans: &mut HashSet<(usize, usize)>,
    ) {
        for child in block.blocks() {
            if names.contains(child.kind()) {
                spans.insert(span_key(&child));
            }
            if child.kind() == "wdoc_repeater" {
                mark_layout_fills(&child, names, spans);
            }
        }
    }

    fn mark_component_fills(
        doc: &Document,
        block: &Block<'_>,
        spans: &mut HashSet<(usize, usize)>,
    ) {
        if let Some(def) = doc.kind_declarer(block.kind()) {
            let names = content_slot_names(&def);
            for child in block.blocks() {
                if names.contains(child.kind()) {
                    spans.insert(span_key(&child));
                }
            }
        }
        for child in block.blocks() {
            mark_component_fills(doc, &child, spans);
        }
    }

    let layout_names: HashSet<String> = doc
        .blocks()
        .filter(|block| block.kind() == "template")
        .flat_map(|template| content_slot_names(&template))
        .collect();
    let mut fill_spans = HashSet::new();
    for block in doc.blocks() {
        if block.kind() == "page" {
            mark_layout_fills(&block, &layout_names, &mut fill_spans);
        }
        mark_component_fills(doc, &block, &mut fill_spans);
    }

    doc.schema_errors()
        .into_iter()
        .filter(|error| match error {
            EvalError::SchemaViolation { span, .. } => {
                !fill_spans.contains(&(span.offset(), span.len()))
            }
            _ => true,
        })
        .collect()
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

    let errs = schema_errors(&doc);
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
    let reserved = contract_errors(&doc);
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

    // Incremental dev-server path: when the caller passed the set of changed
    // files, try to re-render only the page(s) they touch instead of the
    // whole site. `affected_pages` returns `None` — fall through to the full
    // rebuild below — for any change that could invalidate shared state (an
    // imported library, the page set, CSS, an asset declaration, a repeater).
    let targets = changed.and_then(|changed| affected_pages(&doc, file, changed));
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
/// A changed file that declares a `site` / structured CSS rule / `iconset`
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
            "page" => {
                targets.insert(page_name(&block)?);
            }
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
        // Every page names its sites. An untagged page used to belong to
        // all of them, which was harmless while a site was only an output
        // folder — but the site also chooses the page's template, so adding
        // a site would re-render every untagged page under it without the
        // page changing. A genuinely shared page says so
        // (`sites = [:docs, :blog]`). Only `Page.sites` is required: a
        // CSS rule with no `sites` list stays global.
        for p in all_pages {
            if block_sites(p).is_none_or(|list| list.is_empty()) {
                let name = page_name(p).unwrap_or_else(|| "<unnamed>".to_string());
                return Err(BuildError::BadPage(format!(
                    "page \"{name}\" declares no `sites` — in a document with more \
                     than one site every page must name the sites it belongs to \
                     (declared: {})",
                    names
                        .iter()
                        .flatten()
                        .map(|n| format!(":{n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
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

/// A block's declared `sites` membership list (used by `page` and CSS
/// blocks). `None` ⇒ the field is absent, so the block belongs
/// to every site — same as an empty list.
fn block_sites(block: &Block<'_>) -> Option<Vec<String>> {
    field_symbol_list_opt(block, "sites")
}

/// Whether a block belongs to the site named `site_name`. An absent or
/// empty `sites` list means every site — which for a `page` can only
/// happen in a single-site document, since [`collect_site_specs`] rejects
/// an untagged page as soon as a second site is declared.
fn block_in_site(block: &Block<'_>, site_name: Option<&str>) -> bool {
    match block_sites(block) {
        None => true,
        Some(list) if list.is_empty() => true,
        Some(list) => site_name.is_some_and(|n| list.iter().any(|s| s == n)),
    }
}

/// Build the document's `<style>` content for one site from structured CSS
/// rules, ordered library-before-user
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
/// Rules are split by library (embedded-stdlib) vs user origin so the colour
/// theme can be spliced between them.
#[derive(Default)]
struct CssBuckets {
    lib_rules: Vec<String>,
    user_rules: Vec<String>,
}

/// Collect a top-level block's CSS contribution into `css`. A structured CSS
/// block deposits directly; a generator (`wdoc_repeater`,
/// `wdoc_instance`, or a `wdoc_component` instance) is expanded and its
/// generated blocks collected recursively — so a repeater driven by data
/// can emit `class` blocks (the "repeater anywhere" hook for design-system
/// classes). `is_lib` is the origin, carried through expansion. Non-CSS,
/// non-generator blocks (pages, etc.) contribute nothing, exactly as before.
fn collect_css_block(b: &Block<'_>, is_lib: bool, css: &mut CssBuckets) {
    match b.kind() {
        "class" | "base" | "font_face" | "media" | "keyframes" => {
            if let Some(rule) = render_css_block(b) {
                if is_lib {
                    &mut css.lib_rules
                } else {
                    &mut css.user_rules
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
            if let Some(def) = b.doc().kind_declarer(kind) {
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
        lib_rules,
        user_rules,
    } = css;
    // The colour theme sits between the library rules (whose defaults it
    // overrides) and the user rules (which still win).
    let theme_css = site_theme_css(doc, site_block);
    lib_rules
        .into_iter()
        .chain(theme_css.into_iter().filter(|s| !s.is_empty()))
        .chain(user_rules)
        .collect::<Vec<_>>()
        .join("\n")
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
    target: Option<&HashSet<String>>,
) -> Result<SiteBuild, BuildError> {
    // A targeted incremental render reuses the prior full build's aggregate
    // site-wide assets (player scripts, default favicon, copied `assets/`
    // folders, the search index and icon sprite) rather than rewriting them.
    let write_shared = target.is_none();

    // The page <style>: bundled theme + structured rules, scoped
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
    // Site descriptor: the default template + title a template can show.
    // `None` block ⇒ the synthetic default site, so pages render bare
    // unless they set their own `template`.
    let default_template = spec
        .block
        .as_ref()
        .and_then(|b| field_symbol(b, "default_template"));
    validate_slot_contracts(doc, spec)?;
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
    // A repeated slot declares a collection template. Collection-ness comes
    // entirely from the selected layout contract, never from its name.
    let collection_template = default_template
        .as_deref()
        .and_then(|name| find_template(doc, name))
        .filter(|template| declares_collection(&declared_slots(template)));
    let is_collection = collection_template.is_some();

    // Ordered (name, href) list of this site's pages for template nav,
    // and the name set the inline link pattern resolves `[text](page)`
    // against — both scoped to the site, so nav lists only this site's
    // pages and links resolve within it.
    let pages: Vec<(String, String, String)> = spec
        .pages
        .iter()
        .filter_map(|p| {
            page_name(p).map(|n| {
                let title = page_heading_title(p).unwrap_or_else(|| n.clone());
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
    if let Some(missing) = deck_missing_slide(&deck_nodes, &page_names) {
        return Err(BuildError::BadTemplate(format!(
            "deck slide links to unknown page \"{missing}\""
        )));
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

    // Immutable site facts are one Arc-backed value shared by every page.
    // The TOC's shared identity is the memoisation key for page_metadata.
    let pages_value = pages_to_value(&pages);
    let toc_value = if toc_nodes.is_empty() {
        flat_toc_to_value(&pages)
    } else {
        toc_to_value(&toc_nodes)
    };
    let menu_value = menu_to_value(&menu_nodes);
    let footer_value = footer_to_value(&footer_nodes, &inline_patterns);

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
        toc: &toc_value,
        menu: &menu_value,
        footer: &footer_value,
        deck_nodes: &deck_nodes,
        pages_value: &pages_value,
        home_href,
        home_title,
        players: PlayerScripts {
            terminals: uses_terminals,
            pan_zoom: uses_pan_zoom,
            map: uses_map,
            dopesheet: uses_dopesheet,
            search,
        },
        search,
    };

    // A collection renders all members into one `index.html`, so a
    // single-member edit cannot be isolated.
    if is_collection && target.is_some() {
        return Ok(SiteBuild {
            count: 0,
            rendered: Vec::new(),
            need_full: true,
        });
    }

    // A collection site renders all members into a single `index.html`;
    // every ordinary site renders one file per page. On the
    // incremental path only the pages named in `target` are re-rendered.
    let mut search_entries: Vec<SearchEntry> = Vec::new();
    let mut rendered: Vec<String> = Vec::new();
    let count = if let Some(template) = collection_template {
        build_collection_page(&ctx, &template)?
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

    // Video usage is recorded while consuming the content IR, rather than
    // inferred from authored block kinds: custom lowerings that emit a Video
    // therefore receive the same facade player as the stdlib `video` block.
    if inline_patterns.videos().is_used() {
        write_asset(out_dir, "wdoc-video.js", crate::render::WDOC_VIDEO_JS)?;
    }

    // Copy each local video file / poster referenced by a rendered video
    // node into `_wdoc/`. No-op when none were used.
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

#[derive(Clone)]
struct DeclaredSlot<'a> {
    name: String,
    declaration: Block<'a>,
}

fn declared_slots<'a>(template: &Block<'a>) -> Vec<DeclaredSlot<'a>> {
    template
        .blocks()
        .filter(|block| block.kind() == "slot")
        .filter_map(|declaration| {
            label_string(&declaration).map(|name| DeclaredSlot { name, declaration })
        })
        .collect()
}

fn declares_collection(slots: &[DeclaredSlot<'_>]) -> bool {
    slots
        .iter()
        .any(|slot| slot.name == "content" && slot.declaration.slot_repeated())
}

/// Check the page/layout side of the slot contract before rendering. The
/// selected template is the page's literal override or the site's default;
/// a page whose template cannot be resolved is left quiet, because a false
/// positive is worse than deferring the check to render.
fn validate_slot_contracts(doc: &Document, spec: &SiteSpec<'_>) -> Result<(), BuildError> {
    validate_component_slot_contracts(doc)?;
    let global_slot_names: HashSet<String> = doc
        .blocks()
        .filter(|block| block.kind() == "template")
        .flat_map(|template| declared_slots(&template))
        .map(|slot| slot.name)
        .collect();
    let site_slot_names = site_declared_slot_names(doc, spec);
    let mut validated_collection_templates = HashSet::new();
    if let Some(template_name) = spec
        .block
        .as_ref()
        .and_then(|site| site.field("default_template"))
        .and_then(|field| field.literal_symbol())
        && let Some(template) = find_template(doc, template_name)
    {
        let slots = declared_slots(&template);
        if declares_collection(&slots) {
            validate_collection_site_slots(spec, template_name, &slots)?;
            validated_collection_templates.insert(template_name.to_string());
        }
    }

    // Root repeater-generated pages deliberately stay quiet: their layout
    // pairing is dynamic. Direct authored pages are checked once each, while
    // repeaters *inside* a page are inspected structurally as possible fills.
    let authored_pages: Vec<Block<'_>> = doc
        .blocks()
        .filter(|block| block.kind() == "page")
        .filter(|page| block_in_site(page, spec.name.as_deref()))
        .collect();
    for page in &authored_pages {
        let template_name = if let Some(field) = page.field("template") {
            let Some(name) = field.literal_symbol() else {
                continue;
            };
            name.to_string()
        } else if let Some(field) = spec
            .block
            .as_ref()
            .and_then(|site| site.field("default_template"))
        {
            let Some(name) = field.literal_symbol() else {
                continue;
            };
            name.to_string()
        } else {
            continue;
        };
        let Some(template) = find_template(doc, &template_name) else {
            continue; // the existing unknown-template diagnostic owns this case
        };
        let declared = declared_slots(&template);
        if let Some(slot) = declared.iter().find(|slot| {
            slot.name == "content"
                && !matches!(
                    slot.declaration.slot_type_ref(),
                    Some(TypeRef::Named { path, .. })
                        if path.last().is_some_and(|name| name == "content")
                )
        }) {
            return Err(BuildError::BadPage(format!(
                "template `{template_name}`: reserved slot `content` must have a `content` type, found `{}`",
                slot.declaration
                    .slot_type_ref()
                    .map_or_else(|| "unknown".to_string(), ToString::to_string)
            )));
        }
        let collection = declares_collection(&declared);
        if collection
            && spec
                .block
                .as_ref()
                .and_then(|site| site.field("default_template"))
                .and_then(|field| field.literal_symbol())
                != Some(template_name.as_str())
        {
            let page_name = page_name(page).unwrap_or_else(|| "<unnamed>".to_string());
            return Err(BuildError::BadPage(format!(
                "page `{page_name}` selects collection template `{template_name}`, but collection templates must be selected by the site"
            )));
        }
        if collection && validated_collection_templates.insert(template_name.clone()) {
            validate_collection_site_slots(spec, &template_name, &declared)?;
        }
        let slots: Vec<_> = declared
            .into_iter()
            .filter(|slot| !collection || slot.declaration.slot_repeated())
            .collect();
        let slot_names: HashSet<&str> = slots.iter().map(|slot| slot.name.as_str()).collect();
        let page_name = page_name(page).unwrap_or_else(|| "<unnamed>".to_string());
        let mut fills: BTreeMap<String, usize> = BTreeMap::new();
        let mut fill_children: BTreeMap<String, Vec<Block<'_>>> = BTreeMap::new();
        let mut implicit_content = false;
        let mut content_children = Vec::new();

        for child in page.blocks() {
            if child.kind() == "wdoc_repeater" {
                let mut possible_fills = Vec::new();
                let mut possible_content = false;
                collect_possible_page_items(
                    &child,
                    &global_slot_names,
                    &mut possible_fills,
                    &mut possible_content,
                );
                for fill in possible_fills {
                    record_slot_fill(
                        &fill,
                        &slot_names,
                        &site_slot_names,
                        &global_slot_names,
                        &page_name,
                        &template_name,
                        &mut fills,
                        &mut fill_children,
                    )?;
                }
                if possible_content {
                    implicit_content = true;
                    content_children.push(child.clone());
                }
                continue;
            }
            if !record_slot_fill(
                &child,
                &slot_names,
                &site_slot_names,
                &global_slot_names,
                &page_name,
                &template_name,
                &mut fills,
                &mut fill_children,
            )? {
                implicit_content = true;
                content_children.push(child.clone());
            }
        }
        if implicit_content {
            if !slot_names.contains("content") {
                return Err(BuildError::BadPage(format!(
                    "page `{page_name}` has loose content, but template `{template_name}` does not declare the reserved `content` slot"
                )));
            }
            *fills.entry("content".to_string()).or_default() += 1;
            fill_children.insert("content".to_string(), content_children);
        }

        if let Some((name, _)) = fills.iter().find(|(_, count)| **count > 1) {
            return Err(BuildError::BadPage(format!(
                "page `{page_name}` fills slot `{name}` more than once"
            )));
        }

        for slot in slots {
            let required =
                !slot.declaration.slot_optional() && slot.declaration.field("default").is_none();
            if required && !fills.contains_key(&slot.name) {
                return Err(BuildError::BadPage(format!(
                    "page `{page_name}`: required slot `{}` is unfilled for template `{template_name}`",
                    slot.name
                )));
            }
            let Some(accepted) = slot.declaration.slot_type_ref().and_then(|ty| match ty {
                TypeRef::Named { path, args }
                    if path.last().is_some_and(|name| name == "content") =>
                {
                    args.first()
                }
                _ => None,
            }) else {
                continue;
            };
            let Some(children) = fill_children.get(&slot.name) else {
                continue;
            };
            for child in children {
                let contextual = child
                    .decorators()
                    .any(|decorator| decorator.name() == "contextual");
                if contextual || block_matches_accepted_type(child, accepted) {
                    continue;
                }
                return Err(BuildError::BadPage(format!(
                    "page `{page_name}`: slot `{}` accepts `{accepted}`, but found `{}`",
                    slot.name,
                    child.kind()
                )));
            }
        }
    }
    Ok(())
}

fn validate_collection_site_slots(
    spec: &SiteSpec<'_>,
    template_name: &str,
    slots: &[DeclaredSlot<'_>],
) -> Result<(), BuildError> {
    let site_slots: Vec<&DeclaredSlot<'_>> = slots
        .iter()
        .filter(|slot| !slot.declaration.slot_repeated())
        .collect();
    let names: HashSet<&str> = site_slots.iter().map(|slot| slot.name.as_str()).collect();
    let mut fills: BTreeMap<String, usize> = BTreeMap::new();
    let mut fill_children: BTreeMap<String, Vec<Block<'_>>> = BTreeMap::new();
    let mut loose_content = Vec::new();

    if let Some(site) = &spec.block {
        for child in site.blocks() {
            if names.contains(child.kind()) {
                *fills.entry(child.kind().to_string()).or_default() += 1;
                fill_children
                    .entry(child.kind().to_string())
                    .or_default()
                    .extend(child.blocks());
            } else if child
                .schema()
                .is_some_and(|schema| schema.is_descendant_of("wdoc.WdocBlock"))
            {
                loose_content.push(child);
            }
        }
    }
    if !loose_content.is_empty() {
        if !names.contains("content") {
            return Err(BuildError::BadPage(format!(
                "site has loose content, but collection template `{template_name}` does not declare a non-repeated `content` slot"
            )));
        }
        *fills.entry("content".to_string()).or_default() += 1;
        fill_children.insert("content".to_string(), loose_content);
    }
    if let Some((name, _)) = fills.iter().find(|(_, count)| **count > 1) {
        return Err(BuildError::BadPage(format!(
            "site fills slot `{name}` more than once"
        )));
    }
    for slot in site_slots {
        let required =
            !slot.declaration.slot_optional() && slot.declaration.field("default").is_none();
        if required && !fills.contains_key(&slot.name) {
            return Err(BuildError::BadPage(format!(
                "site: required slot `{}` is unfilled for collection template `{template_name}`",
                slot.name
            )));
        }
        let Some(accepted) = slot.declaration.slot_type_ref().and_then(|ty| match ty {
            TypeRef::Named { path, args } if path.last().is_some_and(|name| name == "content") => {
                args.first()
            }
            _ => None,
        }) else {
            continue;
        };
        for child in fill_children.get(&slot.name).into_iter().flatten() {
            if !block_matches_accepted_type(child, accepted) {
                return Err(BuildError::BadPage(format!(
                    "site: slot `{}` accepts `{accepted}`, but found `{}`",
                    slot.name,
                    child.kind()
                )));
            }
        }
    }
    Ok(())
}

fn site_declared_slot_names(doc: &Document, spec: &SiteSpec<'_>) -> HashSet<String> {
    let mut templates = HashSet::new();
    if let Some(name) = spec
        .block
        .as_ref()
        .and_then(|site| site.field("default_template"))
        .and_then(|field| field.literal_symbol())
    {
        templates.insert(name.to_string());
    }
    for page in &spec.pages {
        if let Some(name) = page
            .field("template")
            .and_then(|field| field.literal_symbol())
        {
            templates.insert(name.to_string());
        }
    }
    templates
        .into_iter()
        .filter_map(|name| find_template(doc, &name))
        .flat_map(|template| declared_slots(&template))
        .map(|slot| slot.name)
        .collect()
}

/// Check named content holes on component instances using the declaring
/// component as the scope. Scalar parameters remain owned by the generic
/// `@declares_kind` schema; this is the content half whose bare wrapper names
/// cannot be represented by a document-global block schema.
fn validate_component_slot_contracts(doc: &Document) -> Result<(), BuildError> {
    fn visit(doc: &Document, block: &Block<'_>, depth: usize) -> Result<(), BuildError> {
        if depth > MAX_LOWER_DEPTH {
            return Ok(());
        }
        if let Some(def) = doc.kind_declarer(block.kind()) {
            let slots: Vec<DeclaredSlot<'_>> = declared_slots(&def)
                .into_iter()
                .filter(|slot| {
                    matches!(
                        slot.declaration.slot_type_ref(),
                        Some(TypeRef::Named { path, .. })
                            if path.last().is_some_and(|name| name == "content")
                    )
                })
                .collect();
            if !slots.is_empty() {
                let names: HashSet<&str> = slots.iter().map(|slot| slot.name.as_str()).collect();
                let mut fills: BTreeMap<String, usize> = BTreeMap::new();
                let mut fill_children: BTreeMap<String, Vec<Block<'_>>> = BTreeMap::new();
                let mut loose = Vec::new();
                for child in block.blocks() {
                    if names.contains(child.kind()) {
                        *fills.entry(child.kind().to_string()).or_default() += 1;
                        fill_children
                            .entry(child.kind().to_string())
                            .or_default()
                            .extend(child.blocks());
                    } else {
                        loose.push(child);
                    }
                }
                if names.contains("content") && !loose.is_empty() {
                    *fills.entry("content".to_string()).or_default() += 1;
                    fill_children.insert("content".to_string(), loose.clone());
                }
                if let Some((name, _)) = fills.iter().find(|(_, count)| **count > 1) {
                    return Err(BuildError::BadPage(format!(
                        "component `{}` fills slot `{name}` more than once",
                        block.kind()
                    )));
                }
                for slot in &slots {
                    let required = !slot.declaration.slot_optional()
                        && slot.declaration.field("default").is_none();
                    if required && !fills.contains_key(&slot.name) {
                        return Err(BuildError::BadPage(format!(
                            "component `{}`: required slot `{}` is unfilled",
                            block.kind(),
                            slot.name
                        )));
                    }
                    let Some(accepted) = slot.declaration.slot_type_ref().and_then(|ty| match ty {
                        TypeRef::Named { path, args }
                            if path.last().is_some_and(|name| name == "content") =>
                        {
                            args.first()
                        }
                        _ => None,
                    }) else {
                        continue;
                    };
                    for child in fill_children.get(&slot.name).into_iter().flatten() {
                        if !block_matches_accepted_type(child, accepted) {
                            return Err(BuildError::BadPage(format!(
                                "component `{}`: slot `{}` accepts `{accepted}`, but found `{}`",
                                block.kind(),
                                slot.name,
                                child.kind()
                            )));
                        }
                    }
                }
                for child in block.blocks() {
                    if names.contains(child.kind()) {
                        for nested in child.blocks() {
                            visit(doc, &nested, depth + 1)?;
                        }
                    } else {
                        visit(doc, &child, depth + 1)?;
                    }
                }
                return Ok(());
            }
        }
        for child in block.blocks() {
            visit(doc, &child, depth + 1)?;
        }
        Ok(())
    }

    for root in doc.blocks() {
        if root.kind() == "page" || root.kind() == "wdoc_repeater" {
            visit(doc, &root, 0)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_slot_fill<'a>(
    fill: &Block<'a>,
    slot_names: &HashSet<&str>,
    site_slot_names: &HashSet<String>,
    global_slot_names: &HashSet<String>,
    page_name: &str,
    template_name: &str,
    fills: &mut BTreeMap<String, usize>,
    fill_children: &mut BTreeMap<String, Vec<Block<'a>>>,
) -> Result<bool, BuildError> {
    let name = fill.kind();
    if slot_names.contains(name) {
        *fills.entry(name.to_string()).or_default() += 1;
        fill_children
            .entry(name.to_string())
            .or_default()
            .extend(fill.blocks());
        return Ok(true);
    }
    if fill.is_conditional() && !site_slot_names.contains(name) {
        return Err(BuildError::BadPage(format!(
            "page `{page_name}` conditionally fills slot `{name}`, but no layout used by this site declares it"
        )));
    }
    if site_slot_names.contains(name) {
        if fill.is_conditional() {
            return Ok(true); // author requested that this absent fill be dropped
        }
        return Err(BuildError::BadPage(format!(
            "page `{page_name}` fills slot `{name}`, but template `{template_name}` does not declare it"
        )));
    }
    if global_slot_names.contains(name) {
        return Err(BuildError::BadPage(format!(
            "page `{page_name}` fills slot `{name}`, but no layout used by this site declares it"
        )));
    }
    Ok(false)
}

/// Classify authored sites inside a repeater without evaluating its data.
/// Each named fill remains one possible fill regardless of iteration count;
/// any other generated block is possible implicit `content`.
fn collect_possible_page_items<'a>(
    block: &Block<'a>,
    global_slot_names: &HashSet<String>,
    fills: &mut Vec<Block<'a>>,
    has_content: &mut bool,
) {
    for child in block.blocks() {
        if child.kind() == "wdoc_repeater" {
            collect_possible_page_items(&child, global_slot_names, fills, has_content);
        } else if global_slot_names.contains(child.kind()) {
            fills.push(child);
        } else {
            *has_content = true;
        }
    }
}

/// Expand page-level repeaters before routing their generated blocks into
/// slots. Component instances stay intact — they are ordinary content
/// handles whose own expansion happens only when the template places them.
fn expand_page_repeaters<'a>(block: Block<'a>, out: &mut Vec<Block<'a>>) {
    if block.kind() == "wdoc_repeater" {
        for generated in expand_repeater_children(&block) {
            expand_page_repeaters(generated, out);
        }
    } else {
        out.push(block);
    }
}

fn block_matches_accepted_type(block: &Block<'_>, accepted: &TypeRef) -> bool {
    let TypeRef::Named { path, .. } = accepted else {
        return true; // stay conservative for a type shape wdoc cannot classify
    };
    let expected = path.join(".");
    let Some(last) = path.last() else {
        return true;
    };
    block.schema().is_some_and(|schema| {
        schema.name() == last
            || schema.is_descendant_of(&expected)
            || (path.len() == 1 && schema.is_descendant_of(&format!("wdoc.{expected}")))
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
    search: bool,
}

impl PlayerScripts {
    /// Append the `<script>` tags for the players this site uses. Loaded
    /// once per page; each no-ops on a page that doesn't use it. Shared
    /// verbatim by the per-page loop and the presentation deck.
    fn inject(self, body: &mut String, video: bool) {
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
        if video {
            body.push_str("\n<script src=\"_wdoc/wdoc-video.js\" defer></script>\n");
        }
        if self.search {
            body.push_str("\n<script src=\"_wdoc/wdoc-search.js\" defer></script>\n");
        }
    }
}

/// Per-site rendering context shared by collection and ordinary page builds
/// — everything [`build_site`] resolves once that the
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
    toc: &'a Value,
    menu: &'a Value,
    footer: &'a Value,
    deck_nodes: &'a [DeckSectionNode],
    pages_value: &'a Value,
    home_href: &'a str,
    home_title: &'a str,
    players: PlayerScripts,
    search: bool,
}

/// Render any collection template once to the site's `index.html`. Member
/// handles are resolved only when the evaluated template places their slots.
fn build_collection_page(
    ctx: &PageRenderCtx<'_>,
    template: &Block<'_>,
) -> Result<usize, BuildError> {
    let (content_blocks, slot_blocks) = collection_site_fills(ctx.spec, template);
    let title = ctx
        .site_title
        .map(str::to_string)
        .or_else(|| ctx.spec.name.clone())
        .unwrap_or_else(|| "Collection".to_string());
    let mut rendered = render_template(
        ctx.doc,
        template,
        &content_blocks,
        &slot_blocks,
        ctx.base_dir,
        &title,
        "",
        ctx.pages_value,
        ctx.toc,
        ctx.menu,
        ctx.footer,
        Some(CollectionTemplateInput {
            pages: &ctx.spec.pages,
            deck: ctx.deck_nodes,
        }),
        ctx.theme_toggle,
        ctx.home_href,
        ctx.home_title,
        ctx.search,
        ctx.inline_patterns,
    );
    if let Some((page, id)) = rendered.duplicate_id.take() {
        return Err(BuildError::DuplicateId { page, id });
    }
    // Collection members may use the same interactive assets as an ordinary
    // page. A template part requests a bundled player with an inert marker;
    // deck metadata alone has no effect. Remove the marker before output and
    // append the requested player after the member players, preserving the
    // established presentation script order.
    const PRESENTATION_ASSET: &str = "<!--wdoc:bundled-script:presentation.js-->";
    let needs_presentation_player = rendered.body.contains(PRESENTATION_ASSET);
    if needs_presentation_player {
        rendered.body = rendered.body.replace(PRESENTATION_ASSET, "");
    }
    ctx.players
        .inject(&mut rendered.body, ctx.inline_patterns.videos().is_used());
    if needs_presentation_player {
        write_asset(
            ctx.out_dir,
            "presentation.js",
            crate::render::PRESENTATION_PLAYER_JS,
        )?;
        rendered
            .body
            .push_str("\n<script src=\"_wdoc/presentation.js\" defer></script>\n");
    }
    let head = format!("{}{}", ctx.head_extra, rendered.head);
    let html = render_page(&title, ctx.css, &rendered.body, Some(ctx.favicon), &head);
    let out_path = ctx.out_dir.join("index.html");
    fs::write(&out_path, html)
        .map_err(|e| BuildError::Io(e, format!("write {}", out_path.display())))?;
    Ok(1)
}

fn collection_site_fills<'a>(
    spec: &'a SiteSpec<'a>,
    template: &Block<'a>,
) -> (Vec<Block<'a>>, Vec<(String, Vec<Block<'a>>)>) {
    let names: HashSet<String> = declared_slots(template)
        .into_iter()
        .filter(|slot| !slot.declaration.slot_repeated())
        .map(|slot| slot.name)
        .collect();
    let mut content = Vec::new();
    let mut named: BTreeMap<String, Vec<Block<'a>>> = BTreeMap::new();
    let Some(site) = &spec.block else {
        return (content, Vec::new());
    };
    for child in site.blocks() {
        if names.contains(child.kind()) {
            named
                .entry(child.kind().to_string())
                .or_default()
                .extend(child.blocks());
        } else if names.contains("content")
            && child
                .schema()
                .is_some_and(|schema| schema.is_descendant_of("wdoc.WdocBlock"))
        {
            content.push(child);
        }
    }
    (content, named.into_iter().collect())
}

/// A page's first authored top-level heading, used anywhere a human-facing
/// title is needed without rendering and then searching HTML. Both `h1` and
/// `chapter_header` carry their title in the inline label.
fn page_heading_title(page: &Block<'_>) -> Option<String> {
    page.blocks()
        .find(|b| matches!(b.kind(), "h1" | "chapter_header"))
        .and_then(|b| label_string(&b))
        .filter(|title| !title.is_empty())
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

    // Resolve the template before partitioning the authored page tree: bare
    // child names matching its declarations are fills, while everything else
    // belongs to the reserved implicit `content` slot.
    let template_name =
        field_symbol(page, "template").or_else(|| ctx.default_template.map(str::to_string));
    let resolved_template = template_name
        .as_deref()
        .and_then(|name| find_template(ctx.doc, name));
    let slot_names: HashSet<String> = resolved_template
        .as_ref()
        .map(declared_slots)
        .unwrap_or_default()
        .into_iter()
        .map(|slot| slot.name)
        .collect();
    let site_slot_names = site_declared_slot_names(ctx.doc, ctx.spec);

    let mut page_blocks = Vec::new();
    for block in page.blocks() {
        expand_page_repeaters(block, &mut page_blocks);
    }
    // Templates receive lazy authored handles; only bare pages and searchable
    // sites need to lower their implicit content eagerly.
    let needs_eager_content = template_name.is_none() || ctx.search;
    let mut content_blocks = Vec::new();
    let mut slot_blocks: BTreeMap<String, Vec<Block<'_>>> = BTreeMap::new();
    let mut content = String::new();
    for b in &page_blocks {
        if slot_names.contains(b.kind()) {
            slot_blocks.insert(b.kind().to_string(), b.blocks().collect());
            continue;
        }
        if b.is_conditional() && site_slot_names.contains(b.kind()) {
            continue;
        }
        content_blocks.push(b.clone());
        if needs_eager_content
            && let Some(s) = render_block(ctx.doc, b, ctx.inline_patterns, ctx.base_dir)
        {
            content.push_str(&s);
            content.push('\n');
        }
    }
    let mut rendered = match template_name {
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
                &content_blocks,
                &slot_blocks.into_iter().collect::<Vec<_>>(),
                ctx.base_dir,
                &title,
                &page_name,
                ctx.pages_value,
                ctx.toc,
                ctx.menu,
                ctx.footer,
                None,
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
            page_heading: page_heading_title(page),
            duplicate_id: None,
        },
    };
    ctx.players
        .inject(&mut rendered.body, ctx.inline_patterns.videos().is_used());
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
    let title = rendered.page_heading.unwrap_or_else(|| page_name.clone());
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
/// (HTML / PDF / Markdown) consumes unchanged, so the rest of the
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
