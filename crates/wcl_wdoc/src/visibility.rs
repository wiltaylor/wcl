//! Block-visibility filtering — the `@only` / `@except` decorators.
//!
//! Any block instance in a document can carry `@only(...)` and/or
//! `@except(...)` decorators (declared in the stdlib's `visibility.wcl`) to
//! scope it to a subset of the build. Each decorator takes up to three
//! optional `list<symbol>` axes:
//!
//! - `sites` — the site name (the `@inline(0) name` on the `site` block)
//! - `templates` — the site's template kind (`:webpage` / `:book` /
//!   `:presentation`)
//! - `backends` — the output backend (`:html` / `:pdf` / `:markdown`)
//!
//! Semantics: within an axis the listed values are OR'd; the specified axes
//! are AND'd. A block renders iff `(@only absent OR @only matches)` **and**
//! `(@except absent OR @except does NOT fully match)`. An axis whose current
//! value is unknown (e.g. the synthetic unnamed site, or a site with no
//! `default_template`) never matches.
//!
//! The current site / template / backend are read from the per-render
//! [`InlinePatterns`] context, which every backend threads through its block
//! dispatch — so the same predicate gates HTML, PDF, and Markdown.

use wcl_lang::{Block, Decorator, Value};

use crate::inline::InlinePatterns;

/// Whether `block` should render in the current site / template / backend,
/// honouring its `@only` / `@except` decorators. Blocks with neither
/// decorator are always visible.
pub(crate) fn block_visible(block: &Block<'_>, ctx: &InlinePatterns) -> bool {
    // The editor's merged "all views" preview renders everything — the
    // per-block visibility instead surfaces as anchor stamps (see
    // `classify_visibility`) so the client can indicate and toggle it.
    if ctx.all_sites() {
        return true;
    }
    let site = ctx.vis_site();
    let template = ctx.vis_template();
    let backend = ctx.backend().symbol();

    let mut only_ok = true; // no `@only` ⇒ unconstrained
    let mut except_hit = false;

    for d in block.decorators() {
        match d.full_name().as_str() {
            "only" => {
                only_ok &= axes_match(&d, site.as_deref(), template.as_deref(), backend);
            }
            // `@except` hides the block only when every axis it specifies
            // matches the current context.
            "except" => {
                if axes_match(&d, site.as_deref(), template.as_deref(), backend) {
                    except_hit = true;
                }
            }
            _ => {}
        }
    }

    only_ok && !except_hit
}

/// Whether every axis a decorator specifies matches the current context.
/// Unspecified axes don't constrain (vacuously match).
fn axes_match(
    d: &Decorator<'_>,
    site: Option<&str>,
    template: Option<&str>,
    backend: &str,
) -> bool {
    axis_ok(d, "sites", site)
        && axis_ok(d, "templates", template)
        && axis_ok(d, "backends", Some(backend))
}

/// Whether the `name` axis is satisfied: absent ⇒ unconstrained (true); else
/// the current value must be present in the listed symbols. A constrained axis
/// with an unknown current value never matches.
fn axis_ok(d: &Decorator<'_>, name: &str, current: Option<&str>) -> bool {
    let Some(list) = symbol_list_arg(d, name) else {
        return true;
    };
    match current {
        Some(v) => list.iter().any(|s| s == v),
        None => false,
    }
}

/// The block's visibility as the editor's merged-view gutter understands it:
/// the `@except(sites = [:…])` site names, plus a `custom` flag for anything
/// richer (`@only`, positional args, `templates`/`backends` axes, non-symbol
/// lists) — the client defers custom blocks to source editing. Mirrors the
/// editor's `visibility_json` classification so the stamps agree with what
/// the `set_visibility` block op will accept.
pub(crate) fn classify_visibility(block: &Block<'_>) -> (Vec<String>, bool) {
    let mut except_sites: Vec<String> = Vec::new();
    let mut custom = false;
    for d in block.decorators() {
        let name = match d.name_segments() {
            [n] => n.as_str(),
            [ns, n] if ns == "wdoc" => n.as_str(),
            _ => continue,
        };
        match name {
            "only" => custom = true,
            "except" => {
                if !d.positional().map(|p| p.is_empty()).unwrap_or(false) {
                    custom = true;
                }
                for arg in d.named() {
                    if arg.name() != "sites" {
                        custom = true;
                        continue;
                    }
                    match arg.value() {
                        Ok(Value::List(items))
                            if items.iter().all(|v| matches!(v, Value::Symbol(_))) =>
                        {
                            except_sites.extend(items.iter().filter_map(|v| match v {
                                Value::Symbol(s) => Some(s.clone()),
                                _ => None,
                            }));
                        }
                        _ => custom = true,
                    }
                }
            }
            _ => {}
        }
    }
    (except_sites, custom)
}

/// Read a decorator's `name` argument as a list of symbol names. Returns
/// `None` when the argument is absent, not a list, or fails to evaluate — a
/// fail-open default so a malformed filter never silently drops content.
fn symbol_list_arg(d: &Decorator<'_>, name: &str) -> Option<Vec<String>> {
    let value = d
        .resolved_arg_value(name)
        .or_else(|| d.named_arg(name))?
        .ok()?;
    let Value::List(items) = value else {
        return None;
    };
    Some(
        std::sync::Arc::unwrap_or_clone(items)
            .into_iter()
            .filter_map(|v| match v {
                Value::Symbol(s) => Some(s),
                _ => None,
            })
            .collect(),
    )
}
