//! Native blocks: which kinds the renderers implement in **Rust**, on
//! which targets, and the checks that keep that fact and the stdlib's
//! `@native` declarations from drifting apart.
//!
//! A block is rendered exactly one of two ways and its type says which: a
//! WCL `lower` function, or `@native` (see `lib/core.wcl`). Before this
//! module existed, 57 stdlib types declared a `lower` returning `[]` while
//! the Rust dispatch intercepted them entirely — dead code that existed
//! only to satisfy the interface, and the thing the editor's schema
//! introspection read. [`native_errors`] refuses that shape now: a block
//! that declares both, or neither, fails the build.
//!
//! Three facts live here, and nowhere else:
//!
//! 1. **[`NATIVE_DISPATCH`]** — the registry: every natively-rendered kind
//!    with the backends whose dispatch handles it.
//! 2. **[`native_errors`]** — the schema-time cross-check, reached through
//!    [`crate::build::contract_errors`] by all four entry points. It
//!    reads both directions: a declared target with no Rust implementation,
//!    and a Rust implementation no type declares, are equally errors — the
//!    second is how the last stub `lower` got in.
//! 3. **[`refuse_uncovered`]** — using a block on a target it does not
//!    cover is a build error, waived per instance by the visibility
//!    system's backend axis (`@except(backends = [:pdf])`). Capability says
//!    *can't*, author intent says *don't want to*, and the build refuses
//!    until they agree.
//!
//! What was deliberately **not** here: blocks with a real `lower` that a
//! backend nonetheless special-cased. There were two, and the semantic
//! content IR (`lib/content.wcl`) took both. `callout` went first; `code`
//! followed with the five markup-using content blocks — its HTML lowering
//! used to build a code-card out of markup chrome, so Markdown reached
//! past the lowering and re-read the block's fields to emit a fence at
//! all. Both now return a node every backend reads from one declaration,
//! so there is no lowered-block-with-a-backend-shortcut left to exempt.

use miette::Report;
use wcl_lang::{Block, DeclName, Document, EvalError, TypeDecl, Value};

use crate::inline::{Backend, InlinePatterns};
use crate::render::record_lower_error;

use Backend::{Html, Markdown, Pdf, Skill};

/// Every output target. A `@native` with no `backends` argument means
/// this — the common case, since most native blocks (every diagram shape,
/// every structural wrapper) render through one shared implementation.
const EVERY_BACKEND: &[Backend] = &[Html, Pdf, Markdown, Skill];

/// One natively-rendered block kind and the backends that implement it.
struct NativeKind {
    kind: &'static str,
    backends: &'static [Backend],
}

const fn every(kind: &'static str) -> NativeKind {
    NativeKind {
        kind,
        backends: EVERY_BACKEND,
    }
}

/// The Rust dispatch registry: which kinds each backend renders itself.
///
/// Content entries correspond to an arm in `render/html.rs`
/// (`render_block`), `pdf/collect.rs` (`collect_block`) and
/// `markdown/emit.rs` (`Emitter::block`) — the Markdown emitter also drives
/// the skill target, so those two agree by construction. Diagram entries
/// correspond to an arm in `render/svg/shapes.rs` (`render_shape`), which
/// every backend reaches: HTML and Markdown embed the rendered SVG, and the
/// PDF backend paints it. Terminal primitives are read by the terminal
/// renderer embedded by each backend.
///
/// `@native` declarations are checked against this table in both
/// directions, so an entry added here without its declaration (or the
/// reverse) fails the build rather than becoming the next stub `lower`.
///
const NATIVE_DISPATCH: &[NativeKind] = &[
    // ── Page-level blocks ──────────────────────────────────────────
    // Structural wrappers handled by the shared `walk_structural`.
    every("notes"),
    every("partial"),
    every("collect"),
    every("project"),
    // Transparent / layout wrappers with a per-backend arm.
    every("fragment"),
    every("edit_field"),
    // An editor affordance: it renders a button only in the `wcl editor`
    // preview's edit mode, and deliberately nothing anywhere else. Every
    // backend states that in its own dispatch, so the block is covered
    // everywhere and never has to be waived out of a published build.
    every("edit_object"),
    every("table"),
    every("list"),
    every("image"),
    every("terminal"),
    every("demo"),
    every("diagram"),
    every("wdoc_repeater"),
    every("wdoc_instance"),
    every("wdoc_content"),
    // Side-by-side layout is a CSS grid, so only HTML reproduces it — but
    // the content is not the layout, and the static targets stack the
    // children in place rather than dropping them (the degradation `region`
    // and `fragment` already took). Covered everywhere, because every
    // backend now says what it does with one.
    every("column"),
    // Previews a page's generated Markdown by tapping the Markdown
    // emitter from inside the HTML build. Book-only by construction.
    NativeKind {
        kind: "markdown_source",
        backends: &[Html],
    },
    // Ships a file into the output and optionally links it. A PDF is one
    // self-contained document: it has no output folder to copy into and a
    // link to a file that was never shipped is worse than no link, so the
    // PDF target does not cover `file`. See `lib/file.wcl`.
    NativeKind {
        kind: "file",
        backends: &[Html, Markdown, Skill],
    },
    // ── Diagram shapes (render/svg/shapes.rs) ──────────────────────
    // Every one of these is `every(...)`, and that is load-bearing:
    // `refuse_uncovered` is called by the three *page* renderers, not by
    // `render_shape`, so a shape declaring narrower coverage would drop
    // silently on the target it doesn't cover. Giving one a narrower set
    // means wiring the check into `render_shape` in the same change.
    every("rect"),
    every("circle"),
    every("line"),
    every("label"),
    every("polygon"),
    every("container"),
    every("boundary"),
    every("card"),
    every("icon"),
    every("map"),
    every("tilemap"),
    every("dopesheet"),
    every("timeline"),
    every("node_table"),
    every("tree"),
    // Read directly by their parent's Rust renderer — the subtree-shaped
    // natives of the spec: an interface `@children` list never reaches a
    // lowering record, so neither the row nor the node can be a leaf.
    every("node_row"),
    every("tree_node"),
    // ── Wireframe widgets (crate::wireframe) ───────────────────────
    every("wf_label"),
    every("wf_button"),
    every("wf_input"),
    every("wf_dropdown"),
    every("wf_checkbox"),
    every("wf_radio"),
    every("wf_toggle"),
    every("wf_window"),
    every("wf_browser"),
    every("wf_phone"),
    every("wf_tablet"),
    every("wf_panel"),
    every("wf_column"),
    every("wf_row"),
    every("wf_grid"),
    every("wf_node_graph"),
    // ── Terminal primitives (crate::terminal) ─────────────────────
    every("term_text"),
];

/// The registry entry for `kind`, or `None` when the kind is not natively
/// rendered at all (it lowers, or it isn't a block).
fn registered(kind: &str) -> Option<&'static NativeKind> {
    NATIVE_DISPATCH.iter().find(|n| n.kind == kind)
}

/// Refuse a visible block whose kind is native but uncovered here,
/// recording a build error against the instance. Returns `true` when the
/// block was refused (the caller renders nothing for it).
///
/// **Two targets have to cover it, because two are involved.** `renderer`
/// is the backend actually running, which is not always the one the build
/// was started for — a `card`'s body is HTML in whichever target embeds
/// the SVG — and `patterns.backend()` is the output the build is producing.
/// A block must be covered by both: `markdown_source` needs the HTML build's
/// machinery (a renderer question), while `file` needs an output folder
/// beside the document (an output question), and a `file` in a card must
/// not reach a PDF just because the card's body happens to render as HTML.
/// In the ordinary case the two are the same backend and this is one check.
pub(crate) fn refuse_uncovered(
    block: &Block<'_>,
    patterns: &InlinePatterns,
    renderer: Backend,
) -> bool {
    let kind = block.kind();
    let Some(entry) = registered(kind) else {
        return false;
    };
    let output = patterns.backend();
    let Some(failing) = [output, renderer]
        .into_iter()
        .find(|t| !entry.backends.contains(t))
    else {
        return false;
    };
    // The waiver is spelled with the *output* target, because that is what
    // the visibility axis matches against at this point in the build.
    let aside = if failing == output {
        String::new()
    } else {
        format!(
            " — its content renders as :{} inside a diagram card, even in a :{} build",
            renderer.symbol(),
            output.symbol(),
        )
    };
    record_lower_error(
        block,
        EvalError::user_error(
            format!(
                "`{kind}` has no :{} implementation (it is native on {}){aside}; \
                 remove the block or waive it here with `@except(backends = [:{}])`",
                failing.symbol(),
                symbols(entry.backends),
                output.symbol(),
            ),
            block.span(),
        ),
    );
    true
}

/// Read a `@native` decorator's declared coverage. `None` when the
/// decorator carries no `backends` argument (⇒ every target); `Err` when
/// the argument isn't a list of known backend symbols.
fn declared_backends(dec: &wcl_lang::Decorator<'_>) -> Result<Option<Vec<Backend>>, String> {
    let Some(arg) = dec.named_arg("backends") else {
        return Ok(None);
    };
    let Ok(Value::List(items)) = arg else {
        return Err("`backends` must be a list of symbols".to_string());
    };
    let mut out = Vec::new();
    for item in items.iter() {
        let Value::Symbol(name) = item else {
            return Err("`backends` must be a list of symbols".to_string());
        };
        match EVERY_BACKEND.iter().find(|b| b.symbol() == name) {
            Some(b) => out.push(*b),
            None => {
                return Err(format!(
                    "unknown backend `:{name}` (expected one of {})",
                    symbols(EVERY_BACKEND)
                ));
            }
        }
    }
    Ok(Some(out))
}

/// The kind string of a type's `@block("kind")` decorator, if it has one.
fn block_kind(decl: &TypeDecl<'_>) -> Option<String> {
    decl.decorators().find_map(|d| {
        if d.full_name() != "block" {
            return None;
        }
        match d.positional().ok()?.into_iter().next() {
            Some(Value::Utf8(s) | Value::Ascii(s)) => Some(s),
            _ => None,
        }
    })
}

fn error_at(decl: &TypeDecl<'_>, msg: String) -> Report {
    let span = decl.span();
    miette::miette!(
        labels = vec![miette::LabeledSpan::at(
            span.start..span.end,
            "declared here"
        )],
        code = "wdoc::native",
        "{msg}",
    )
}

/// Errors for every block type whose rendering contract is broken:
/// declaring both a `lower` and `@native` or neither, claiming a target no
/// backend implements, or leaving a Rust-implemented kind undeclared.
/// Shared by the HTML / PDF / Markdown / skill entry points, which run it
/// right after `schema_errors`.
pub(crate) fn native_errors(doc: &Document) -> Vec<Report> {
    let mut out = Vec::new();
    // Registry kinds a `wdoc`-namespace `@native` block accounted for, so
    // the reverse direction (implemented but undeclared) can be read off
    // the same pass.
    let mut accounted: Vec<&str> = Vec::new();

    for decl in doc.type_decls() {
        let native = decl.decorators().find(|d| d.full_name() == "native");
        // Only a type in the lowering contract — one that inherits `lower`
        // from an output interface (transitively, so `Widget` counts)
        // — has a rendering to declare. Structural block types (`page`,
        // `frontmatter`, `li`) render as part of their parent and have
        // neither.
        let lowerable = ["wdoc.ContentBlock", "wdoc.SvgBlock", "wdoc.TermPrimitive"]
            .iter()
            .any(|interface| decl.is_descendant_of(interface));
        let own_lower = decl.fields().any(|f| f.name() == "lower");
        let kind = block_kind(&decl);

        let Some(native) = native else {
            if lowerable && !own_lower {
                out.push(error_at(
                    &decl,
                    format!(
                        "type '{}' declares neither a `lower` nor `@native` — a block is \
                         rendered by a WCL lowering or by wdoc's Rust dispatch, and its \
                         type must say which",
                        decl.name()
                    ),
                ));
            }
            // A kind the registry claims, declared with a `lower`: the
            // lowering is dead code the renderer never calls.
            if let Some(kind) = &kind
                && is_wdoc_ns(&decl)
                && let Some(entry) = registered(kind)
            {
                accounted.push(entry.kind);
                out.push(error_at(
                    &decl,
                    format!(
                        "type '{}' declares a `lower`, but wdoc renders \"{kind}\" natively \
                         on {} — the lowering is never called; declare `@native` instead",
                        decl.name(),
                        symbols(entry.backends),
                    ),
                ));
            }
            continue;
        };

        if !lowerable {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' carries `@native` but is not a renderable block — only a type \
                     extending `ContentBlock`, `SvgBlock`, or `TermPrimitive` has a \
                     rendering to declare",
                    decl.name()
                ),
            ));
            continue;
        }
        if own_lower {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' declares both a `lower` and `@native` — a block is rendered \
                     one way or the other, never both",
                    decl.name()
                ),
            ));
            continue;
        }
        let Some(kind) = kind else {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' carries `@native` but has no `@block(\"…\")` kind to dispatch on",
                    decl.name()
                ),
            ));
            continue;
        };
        let declared = match declared_backends(&native) {
            Ok(d) => d.unwrap_or_else(|| EVERY_BACKEND.to_vec()),
            Err(msg) => {
                out.push(error_at(&decl, format!("type '{}': {msg}", decl.name())));
                continue;
            }
        };
        let Some(entry) = registered(&kind) else {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' declares `@native`, but wdoc implements no dispatch for \
                     \"{kind}\" — only wdoc's own blocks can be native; a user block is \
                     rendered by its `lower`",
                    decl.name()
                ),
            ));
            continue;
        };
        if is_wdoc_ns(&decl) {
            accounted.push(entry.kind);
        }
        let implemented = entry.backends;
        let missing: Vec<Backend> = declared
            .iter()
            .filter(|b| !implemented.contains(b))
            .copied()
            .collect();
        let unclaimed: Vec<Backend> = implemented
            .iter()
            .filter(|b| !declared.contains(b))
            .copied()
            .collect();
        if !missing.is_empty() {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' declares \"{kind}\" native on {}, which no backend implements \
                     (wdoc dispatches it on {})",
                    decl.name(),
                    symbols(&missing),
                    symbols(implemented),
                ),
            ));
        }
        if !unclaimed.is_empty() {
            out.push(error_at(
                &decl,
                format!(
                    "type '{}' omits {} from \"{kind}\"'s `backends`, but wdoc implements \
                     the kind there — an unclaimed target renders nothing and nobody finds out",
                    decl.name(),
                    symbols(&unclaimed),
                ),
            ));
        }
    }

    out.extend(undeclared_errors(&accounted));
    out
}

/// Whether the document has any of the wdoc stdlib's native block
/// declarations in scope. The registry cross-check only has meaning once
/// this premise holds; otherwise every registry entry looks undeclared.
pub(crate) fn has_wdoc_native_declaration(doc: &Document) -> bool {
    doc.type_decls()
        .any(|decl| is_wdoc_ns(&decl) && decl.decorators().any(|d| d.full_name() == "native"))
}

/// The other direction: a kind this build dispatches in Rust that no stdlib
/// type declared at all. Split out so it can be tested against a registry
/// the document can't produce — every kind IS declared, which is the point.
fn undeclared_errors(accounted: &[&str]) -> Vec<Report> {
    NATIVE_DISPATCH
        .iter()
        .filter(|entry| !accounted.contains(&entry.kind))
        .map(|entry| {
            miette::miette!(
                code = "wdoc::native",
                "wdoc renders \"{}\" natively on {}, but no `@native` block type declares \
                 it — the kind's schema and the renderer disagree",
                entry.kind,
                symbols(entry.backends),
            )
        })
        .collect()
}

/// `true` when the declaration lives in wdoc's own namespace, i.e. it is
/// the stdlib declaration of a kind the registry describes rather than a
/// user type that happens to share the name.
fn is_wdoc_ns(decl: &TypeDecl<'_>) -> bool {
    decl.file_ns() == ["wdoc".to_string()]
}

fn symbols(backends: &[Backend]) -> String {
    backends
        .iter()
        .map(|b| format!(":{}", b.symbol()))
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::{Environment, disk_loader};

    fn open_raw(src: &str) -> Document {
        let loader = crate::schema_registry().loader(disk_loader());
        Document::open_at_with_loader(src, "native-test.wcl", None, &Environment::new(), loader)
            .expect("open native fixture")
    }

    /// Open a fixture through the embedded wdoc registry — the same
    /// schema every real build sees, so the stdlib's own declarations are
    /// under test alongside the fixture's.
    fn open_wdoc(extra: &str) -> Document {
        open_raw(&format!("import <wdoc.wcl>\n{extra}"))
    }

    /// The same, in wdoc's own namespace — the position a stdlib
    /// declaration speaks from, which is what the registry cross-check
    /// keys on. `namespace` must precede the import.
    fn open_wdoc_ns(extra: &str) -> Document {
        let src = format!("namespace wdoc\nimport <wdoc.wcl>\n{extra}");
        let loader = crate::schema_registry().loader(disk_loader());
        Document::open_at_with_loader(&src, "native-test.wcl", None, &Environment::new(), loader)
            .expect("open native fixture")
    }

    fn messages(doc: &Document) -> Vec<String> {
        native_errors(doc).iter().map(|r| r.to_string()).collect()
    }

    fn assert_missing_stdlib(src: &str) {
        let errs = crate::build::contract_errors(&open_raw(src));
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(
            errs[0].to_string().contains("import <wdoc.wcl>"),
            "{errs:#?}"
        );
    }

    #[test]
    fn a_document_without_the_wdoc_stdlib_gets_one_actionable_error() {
        assert_missing_stdlib("// nothing here\n");
    }

    #[test]
    fn a_partial_wdoc_import_gets_the_same_actionable_error() {
        assert_missing_stdlib("import <wdoc/core.wcl>\n");
    }

    #[test]
    fn the_stdlib_and_the_registry_agree() {
        let errs = messages(&open_wdoc(""));
        assert!(
            errs.is_empty(),
            "wdoc stdlib fails its own check: {errs:#?}"
        );
    }

    #[test]
    fn a_block_with_neither_lowering_nor_native_is_refused() {
        let errs = messages(&open_wdoc(
            "@block(\"mine\")\ntype Mine extends wdoc.ContentBlock { }\n",
        ));
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(
            errs[0].contains("neither a `lower` nor `@native`"),
            "{errs:#?}"
        );
    }

    #[test]
    fn an_unrelated_interface_with_a_lower_field_is_not_a_block_contract() {
        let errs = messages(&open_wdoc(
            "interface HelperContract {\n  \
             lower: fn(&HelperContract) -> list<wdoc.Content>?\n}\n\
             type Helper extends HelperContract { }\n",
        ));
        assert!(errs.is_empty(), "{errs:#?}");
    }

    #[test]
    fn a_block_with_both_is_refused() {
        let errs = messages(&open_wdoc(
            "@block(\"mine\") @native\ntype Mine extends wdoc.ContentBlock {\n  \
             lower = fn(m: Mine) -> list<wdoc.Html> []\n}\n",
        ));
        assert!(
            errs.iter()
                .any(|e| e.contains("both a `lower` and `@native`")),
            "{errs:#?}"
        );
    }

    #[test]
    fn native_on_a_kind_no_backend_implements_is_refused() {
        let errs = messages(&open_wdoc(
            "@block(\"mine\") @native\ntype Mine extends wdoc.ContentBlock { }\n",
        ));
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(errs[0].contains("implements no dispatch"), "{errs:#?}");
    }

    #[test]
    fn native_on_a_non_block_type_is_refused() {
        let errs = messages(&open_wdoc("@native\ntype Mine { name: utf8 }\n"));
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(errs[0].contains("not a renderable block"), "{errs:#?}");
    }

    #[test]
    fn an_unknown_backend_symbol_is_refused() {
        let errs = messages(&open_wdoc(
            "@block(\"mine\") @native(backends = [:paper])\n\
             type Mine extends wdoc.ContentBlock { }\n",
        ));
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(errs[0].contains("unknown backend `:paper`"), "{errs:#?}");
    }

    #[test]
    fn a_coverage_mismatch_is_reported_in_both_directions() {
        // A `wdoc`-namespace declaration of a registered kind claiming the
        // one backend that doesn't implement it, and omitting the three that
        // do: the check reports each side separately, because they are
        // different mistakes (a target that will render nothing, and a
        // target that renders something nobody declared).
        let errs = messages(&open_wdoc_ns(
            "@block(\"file\") @native(backends = [:pdf])\n\
             type MyFile extends ContentBlock { }\n",
        ));
        assert!(
            errs.iter()
                .any(|e| e.contains("native on :pdf") && e.contains("no backend implements")),
            "{errs:#?}"
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("omits :html, :markdown, :skill")),
            "{errs:#?}"
        );
    }

    #[test]
    fn a_kind_no_type_declares_is_reported() {
        // Keep the full stdlib in scope but replace one imported module with
        // an empty namespace, reproducing a kind whose declaration vanished.
        // Driving the shared gate proves the missing-stdlib guard does not
        // hide the registry's per-kind regression diagnostic.
        let mut registry = crate::schema_registry();
        registry.register("wdoc/markdown_source.wcl", "namespace wdoc\n");
        let loader = registry.loader(disk_loader());
        let doc = Document::open_at_with_loader(
            "import <wdoc.wcl>\n",
            "native-test.wcl",
            None,
            &Environment::new(),
            loader,
        )
        .expect("open stdlib with one missing native declaration");
        let errs: Vec<String> = crate::build::contract_errors(&doc)
            .iter()
            .map(|r| r.to_string())
            .collect();
        assert_eq!(errs.len(), 1, "{errs:#?}");
        assert!(
            errs[0].contains("markdown_source")
                && errs[0].contains("no `@native` block type declares"),
            "{errs:#?}"
        );
    }

    #[test]
    fn every_registry_entry_names_a_real_backend_set() {
        for entry in NATIVE_DISPATCH {
            assert!(
                !entry.backends.is_empty(),
                "\"{}\" covers no backend at all",
                entry.kind
            );
            let mut seen: Vec<Backend> = Vec::new();
            for b in entry.backends {
                assert!(!seen.contains(b), "\"{}\" lists {b:?} twice", entry.kind);
                seen.push(*b);
            }
        }
    }

    #[test]
    fn the_registry_has_no_duplicate_kinds() {
        let mut seen: Vec<&str> = Vec::new();
        for entry in NATIVE_DISPATCH {
            assert!(
                !seen.contains(&entry.kind),
                "\"{}\" is registered twice",
                entry.kind
            );
            seen.push(entry.kind);
        }
    }
}
