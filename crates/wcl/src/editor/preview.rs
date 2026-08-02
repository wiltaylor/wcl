//! The editor's wdoc preview machinery: the `/api/preview` full/targeted
//! build ([`handle_preview`] → [`preview_site`]), the per-slug
//! [`PreviewSession`] bookkeeping that remembers each build's inputs, and
//! the lazy per-page rebuild that materializes stale pages when the iframe
//! navigates to them ([`handle_preview_file`]).
//!
//! Two decisions used to be spread across the request handler, the build
//! function and the client, and now each has exactly one owner:
//!
//! - **where a build lands** — [`PreviewTarget::slug`], called by the build
//!   path and named verbatim in the `/api/preview/sites/<slug>/…` hrefs the
//!   lazy fetch reads back.
//! - **what a build renders** — [`BuildRequest::plan`] (a POST) and
//!   [`PreviewSession::lazy_plan`] (a navigation), both answering the same
//!   [`BuildPlan`] that [`run_preview_build`] executes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{EditorState, api_result, err_str};
use crate::serve::{content_type, json_error, parse_json_body};

/// The warm preview state of every built output slug — and the handle a
/// write path uses to invalidate it.
///
/// A write outside `/api/preview` changes the disk under every already-built
/// page, so the lazy per-page rebuild must stop treating them as fresh.
/// [`Sessions::invalidate`] is the whole of what a write handler may do here;
/// the rest of the map is the preview module's own business, which is why
/// only it takes the full [`EditorState`].
#[derive(Default)]
pub(crate) struct Sessions {
    inner: std::sync::Mutex<std::collections::HashMap<String, PreviewSession>>,
}

impl Sessions {
    /// Mark every built preview stale: bump each session's generation so the
    /// lazy per-page GET re-renders instead of serving pre-commit HTML as
    /// fresh (the in-place commit paths never POST `/api/preview`).
    pub(crate) fn invalidate(&self) {
        for s in self.inner.lock().unwrap().values_mut() {
            s.generation += 1;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, PreviewSession>> {
        self.inner.lock().unwrap()
    }
}

/// Everything that decides *which output directory* a preview build lands
/// in: the entry document, the site selected within it, and the two render
/// variants that must never share a directory with the plain one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviewTarget {
    /// The entry as the client posted it (repo-relative) — what
    /// distinguishes two selections in the site picker.
    entry: String,
    site: Option<String>,
    /// The merged all-views render (`merged: true`): visibility bypassed,
    /// per-block visibility stamped. Its pages must never shadow the normal
    /// per-view output, nor be shadowed by it.
    merged: bool,
    /// A synthetic unit page rides in this directory. Unit previews share
    /// ONE `__unit` directory per (entry, site): the extra page is always
    /// named `__wcl_unit_preview`, so the dir's page set stays stable
    /// across units and switching units is a warm targeted rebuild of that
    /// one page instead of a cold full build per unit. (They must not share
    /// the plain merged dir: without the unit overlay the document lacks
    /// the synthetic page, and the page-set drift would force a full
    /// rebuild on every alternation.)
    unit: bool,
}

impl PreviewTarget {
    /// The `sites/<slug>` output directory name for this target — the one
    /// function the build path calls, and the name the served
    /// `/api/preview/sites/<slug>/…` hrefs carry back to the lazy fetch.
    ///
    /// The readable prefix keeps the scratch tree browsable; the FNV-1a
    /// suffix over the exact fields is what makes it *unique*. The prefix
    /// alone collapses every non-alphanumeric byte, so `docs/main.wcl` and
    /// `docs_main.wcl` would name one directory and serve each other's
    /// pages — a failure that reads as inexplicable rather than as a bug.
    pub(crate) fn slug(&self) -> String {
        // NUL-separated so no field's contents can imitate a boundary, and
        // an absent site is encoded apart from an empty one.
        let key = format!(
            "{}\0{}\0{}\0{}",
            self.entry,
            match &self.site {
                Some(s) => format!("={s}"),
                None => "-".to_string(),
            },
            self.merged,
            self.unit,
        );
        let readable: String = self
            .entry
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!(
            "{readable}{}{}_{}",
            if self.merged { "__merged" } else { "" },
            if self.unit { "__unit" } else { "" },
            fnv1a_hex(key.as_bytes()),
        )
    }
}

/// FNV-1a 64-bit as lowercase hex — dependency-free and stable, the same
/// hash `wcl_wdoc`'s review handshake names its marker directories with.
fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// What the last `/api/preview` build of one output slug rendered, and with
/// which inputs — enough to lazily materialize a stale page on request.
///
/// After a targeted rebuild only the named pages reflect the latest inputs;
/// every other `<name>.html` on disk is a leftover from an earlier build.
/// The `generation` counter (bumped per `/api/preview` POST) tracks that: a
/// page is fresh iff the last build was full (`full_gen == generation`) or
/// the page itself was rendered at the current generation. Lazy rebuilds
/// materialize the *current* generation, so they never bump it.
///
/// A session existing at all is also what "the output directory is warm"
/// means: the scratch tree is created with this process, so a slug has been
/// built into iff its session is recorded.
pub(crate) struct PreviewSession {
    entry_abs: PathBuf,
    site: Option<String>,
    /// Whether this slug is the merged all-views build. Lazy rebuilds must
    /// reuse it or a stale merged page would silently re-render without the
    /// visibility bypass.
    all_sites: bool,
    /// The unsaved buffers the last POST built with — lazy rebuilds reuse
    /// them so navigation stays consistent with the last Rebuild.
    overlay: HashMap<PathBuf, String>,
    generation: u64,
    full_gen: u64,
    page_gen: HashMap<String, u64>,
}

impl PreviewSession {
    fn is_fresh(&self, page: &str) -> bool {
        self.full_gen == self.generation || self.page_gen.get(page) == Some(&self.generation)
    }

    /// Cheap freshness pre-check for a served `<stem>.html`, answerable
    /// without touching the disk. `index.html` is a copy of the start page,
    /// so its freshness needs the manifest and it never short-circuits.
    fn maybe_fresh(&self, stem: &str) -> bool {
        self.full_gen == self.generation || (stem != "index" && self.is_fresh(stem))
    }

    /// What serving one page needs first: `None` when what's on disk
    /// already reflects the current generation, else the plan that
    /// materializes it. `page` is the manifest-resolved page name; `None`
    /// (unknown stem, no manifest) can only be answered by a full build.
    fn lazy_plan(&self, page: Option<&str>) -> Option<BuildPlan> {
        match page {
            Some(p) if self.is_fresh(p) => None,
            Some(p) => Some(BuildPlan::Targeted {
                pages: Some(HashSet::from([p.to_string()])),
            }),
            None if self.full_gen == self.generation => None,
            None => Some(BuildPlan::Full),
        }
    }

    /// Record what a build pass rendered at the current generation.
    fn note(&mut self, result: &PreviewBuild) {
        match result {
            PreviewBuild::Full => {
                self.full_gen = self.generation;
                self.page_gen.clear();
            }
            PreviewBuild::Targeted(pages) => {
                for p in pages {
                    self.page_gen.insert(p.clone(), self.generation);
                }
            }
        }
    }
}

/// What a preview build pass *should* render — the whole site, or only the
/// pages a warm output directory needs refreshed. The single answer to the
/// targeted-versus-full question; [`run_preview_build`] only executes it
/// (and self-falls-back to a full pass when the engine reports it must).
#[derive(Clone, Debug, PartialEq, Eq)]
enum BuildPlan {
    Full,
    /// Re-render into a warm output dir. `pages` is the explicit filter;
    /// `None` means "whatever the changed files map onto".
    Targeted {
        pages: Option<HashSet<String>>,
    },
}

/// The parts of a `/api/preview` POST the build decision depends on.
struct BuildRequest {
    target: PreviewTarget,
    pages: Option<HashSet<String>>,
    changed: Vec<PathBuf>,
}

impl BuildRequest {
    /// The targeted-versus-full decision for this POST, as a pure function
    /// of the request and the recorded session (`None` = nothing built into
    /// this directory yet, so there is nothing to render *into*). The POST
    /// half of the pair [`PreviewSession::lazy_plan`] answers for a
    /// navigation.
    ///
    /// A unit preview is warm even on a cold directory: its slug only ever
    /// serves the one synthetic page, so rendering the entry's other ~N
    /// pages first is pure waste (a minute-plus on a big WAD in a debug
    /// build). When that single-page render genuinely needs shared state (a
    /// new icon), the build engine falls back to full on its own.
    fn plan(&self, session: Option<&PreviewSession>) -> BuildPlan {
        let warm = self.target.unit || session.is_some();
        if !warm || (self.pages.is_none() && self.changed.is_empty()) {
            return BuildPlan::Full;
        }
        BuildPlan::Targeted {
            pages: self.pages.clone(),
        }
    }
}

/// What a preview build pass did — the session bookkeeping half of
/// [`wcl_wdoc::RebuildOutcome`].
enum PreviewBuild {
    Full,
    Targeted(Vec<String>),
}

/// `POST /api/preview` — build of the selected site (`entry` + optional
/// `site` name) with the posted unsaved buffers overlaid, into the preview
/// scratch tree. Manual-rebuild semantics: the client only calls this from
/// its Rebuild button (or a design-mode commit), and the long-held POST is
/// the build's progress signal. Serialized behind the preview gate and run
/// off the async executor — a render is real work.
pub(super) async fn handle_preview(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let _gate = state.preview.lock().await;
    let state2 = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || preview_site(&state2, &v))
        .await
        .unwrap_or_else(|e| Err(format!("preview task failed: {e}")));
    api_result(result)
}

/// The blocking half of [`handle_preview`]: build the selected site into
/// its target's output directory and answer with the `/api/preview/…` href
/// of its index page.
fn preview_site(state: &EditorState, v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(v, "entry")?;
    let entry_abs = state.ws.abs(entry)?;
    let site = v
        .get("site")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    let overlay = state.ws.overlay(v)?;

    // The merged all-views render (the content modal's Merged tab): every
    // block regardless of `@only` / `@except`, with per-block visibility
    // stamped on the edit-mode anchors.
    let merged = v.get("merged").and_then(serde_json::Value::as_bool) == Some(true);
    let skill = v.get("skill").and_then(serde_json::Value::as_bool) == Some(true);
    if merged && skill {
        return Err("merged has no meaning for a skill build".to_string());
    }
    // A merged build may additionally ask for a synthetic page for one unit
    // (`unit: {kind, id}`) — the fallback for units no view builds a page
    // for (they render embedded elsewhere). The page projects the unit's
    // `body`, so its blocks keep their real file/span anchors and stay
    // editable.
    let unit = v.get("unit").filter(|u| u.is_object());
    if unit.is_some() && !merged {
        return Err("unit previews are only available for merged builds".to_string());
    }
    let unit_id = unit
        .map(|u| crate::edit::str_field(u, "id").map(str::to_string))
        .transpose()?;

    let target = PreviewTarget {
        entry: entry.to_string(),
        site: site.clone(),
        merged,
        unit: unit_id.is_some(),
    };
    let slug = target.slug();
    let out = state.preview.root().join("sites").join(&slug);

    // A skill view builds the actual skill folder (the Markdown backend's
    // SKILL.md + references/ + assets) instead of an HTML site, and answers
    // with the file listing so the client can browse it.
    if skill {
        // A fresh dir per build: stale files from renamed pages would
        // otherwise linger in the listing. Any recorded session refers to
        // the wiped output, so it goes too.
        state.sessions.lock().remove(&slug);
        let _ = std::fs::remove_dir_all(&out);
        wcl_wdoc::skill(&entry_abs, &out, site.as_deref()).map_err(|e| e.render_plain())?;
        let _ = wcl_wdoc::take_render_warnings();
        let mut files: Vec<String> = Vec::new();
        collect_files(&out, &out, &mut files);
        files.sort();
        return Ok(serde_json::json!({
            "ok": true,
            "mode": "skill",
            "base": format!("/api/preview/sites/{slug}/"),
            "files": files,
        }));
    }

    // The client posts the page it wants on screen (its preview target) and
    // the files a design-mode commit touched; whether that can be served by
    // a targeted re-render is decided here and nowhere else.
    let request = BuildRequest {
        target,
        pages: v
            .get("pages")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|s: &HashSet<String>| !s.is_empty()),
        changed: v
            .get("changed")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str())
                    .filter_map(|p| state.ws.abs(p).ok())
                    .collect()
            })
            .unwrap_or_default(),
    };
    let plan = request.plan(state.sessions.lock().get(&slug));

    let mut overlay = overlay;
    if let Some(u) = unit {
        let kind = crate::edit::str_field(u, "kind")?;
        let id = unit_id.as_deref().unwrap_or_default();
        append_synthetic_unit_page(&entry_abs, &mut overlay, kind, id)?;
    }

    let built = run_preview_build(BuildJob {
        entry_abs: &entry_abs,
        out: &out,
        site: site.as_deref(),
        all_sites: merged,
        plan,
        overlay: overlay.clone(),
        changed: &request.changed,
    })?;
    let mode = match &built {
        PreviewBuild::Targeted(_) => "targeted",
        PreviewBuild::Full => "full",
    };

    // Record the session so navigating to a page this build left stale can
    // lazily re-render it with the same inputs. A POST is a new generation:
    // its inputs (buffers, saved files) supersede whatever came before.
    let mut sessions = state.sessions.lock();
    let session = sessions
        .entry(slug.clone())
        .or_insert_with(|| PreviewSession {
            entry_abs: entry_abs.clone(),
            site: site.clone(),
            all_sites: merged,
            overlay: HashMap::new(),
            generation: 0,
            full_gen: 0,
            page_gen: HashMap::new(),
        });
    session.entry_abs = entry_abs;
    session.site = site;
    session.all_sites = merged;
    session.overlay = overlay;
    session.generation += 1;
    session.note(&built);
    drop(sessions);

    let index = index_page(&out).ok_or("the site built but produced no HTML page")?;
    Ok(serde_json::json!({
        "ok": true,
        "mode": mode,
        "href": format!("/api/preview/sites/{slug}/{index}"),
    }))
}

/// One preview build pass — everything [`run_preview_build`] needs, so its
/// call sites state their intent instead of a positional parameter list.
struct BuildJob<'a> {
    entry_abs: &'a Path,
    out: &'a Path,
    site: Option<&'a str>,
    /// The merged all-views render: `@only` / `@except` visibility is
    /// bypassed and each block's anchor carries its visibility stamps for
    /// the client's per-view indicator gutter.
    all_sites: bool,
    plan: BuildPlan,
    overlay: HashMap<PathBuf, String>,
    /// Files a design-mode commit touched. Only consulted by a targeted
    /// pass with no explicit page filter.
    changed: &'a [PathBuf],
}

/// Execute one [`BuildPlan`] into `job.out`.
///
/// comment_mode stamps the `data-wcl-block` / `data-wcl-page-*` anchors the
/// preview pane's comment UI keys on; edit_mode adds the per-block
/// `data-wcl-span` / `data-wcl-file` anchors and the `edit_object` "Edit
/// this …" buttons the pane resolves via `/api/object/locate` — still no
/// injected scripts.
///
/// An explicit page filter that renders nothing (the viewed page isn't one
/// of this entry's site pages — a chooser index, an included sub-site's
/// page, a renamed page) means the targeted path can't refresh it, so it
/// falls back to a full build rather than reporting a no-op as success. The
/// incremental engine itself also self-falls-back on structural change
/// (page set, decks, new icons).
fn run_preview_build(job: BuildJob<'_>) -> Result<PreviewBuild, String> {
    let mut opts = wcl_wdoc::BuildOptions {
        overlay: Some(job.overlay),
        comment_mode: true,
        edit_mode: true,
        all_sites: job.all_sites,
        ..Default::default()
    };
    if let BuildPlan::Targeted { pages } = job.plan {
        let explicit = pages.is_some();
        opts.page_filter = pages;
        let outcome =
            wcl_wdoc::build_incremental(job.entry_abs, job.out, job.site, &opts, job.changed)
                .map_err(|e| e.render_plain())?;
        // Per-render warnings would otherwise pile up for whoever drains next.
        let _ = wcl_wdoc::take_render_warnings();
        match outcome {
            wcl_wdoc::RebuildOutcome::Targeted { pages: rendered } => {
                if !rendered.is_empty() || !explicit {
                    return Ok(PreviewBuild::Targeted(rendered));
                }
                // Explicit filter matched nothing — full-build below.
                opts.page_filter = None;
            }
            wcl_wdoc::RebuildOutcome::Full { .. } => return Ok(PreviewBuild::Full),
        }
    }
    wcl_wdoc::build_with_options(job.entry_abs, job.out, job.site, &opts)
        .map_err(|e| e.render_plain())?;
    let _ = wcl_wdoc::take_render_warnings();
    Ok(PreviewBuild::Full)
}

/// The page name synthetic unit previews build under (the content modal's
/// merged tab for units with no page of their own, and the systems view's
/// screen editor).
pub(crate) const UNIT_PREVIEW_PAGE: &str = "__wcl_unit_preview";

/// Append the merged preview's synthetic unit page to the entry's overlay:
/// a `page __wcl_unit_preview { project { from = <gather>.<id>.body } }`
/// that renders the unit's addressable `body` via a static label path — the
/// projected blocks are the unit file's own doc-view blocks, so their
/// edit-mode anchors carry the REAL file/span and every editing op works.
/// Errors when the kind has no document gather or the instance carries no
/// `body` (nothing to render — the client keeps its list fallback).
fn append_synthetic_unit_page(
    entry_abs: &Path,
    overlay: &mut HashMap<PathBuf, String>,
    kind: &str,
    id: &str,
) -> Result<(), String> {
    if id.is_empty()
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || id.starts_with(|c: char| c.is_ascii_digit())
    {
        return Err(format!("bad unit id `{id}`"));
    }
    let doc =
        wcl_wdoc::open_doc_for_edit_with_overlay(entry_abs, overlay.clone()).map_err(err_str)?;
    let gather = doc
        .gather_field(kind)
        .ok_or_else(|| format!("no document gather for kind `{kind}`"))?;
    let unit = doc
        .blocks()
        .find(|b| {
            b.kind() == kind
                && b.labels().ok().is_some_and(|ls| {
                    matches!(
                        ls.first(),
                        Some(
                            wcl_lang::Value::Utf8(s)
                                | wcl_lang::Value::Ascii(s)
                                | wcl_lang::Value::Identifier(s)
                        ) if s == id
                    )
                })
        })
        .ok_or_else(|| format!("no `{kind}` with id `{id}`"))?;
    if !unit.blocks().any(|c| c.kind() == "body") {
        return Err(format!(
            "`{kind} {id}` has no body — nothing to render standalone"
        ));
    }
    let base = match overlay.get(entry_abs) {
        Some(text) => text.clone(),
        None => std::fs::read_to_string(entry_abs)
            .map_err(|e| format!("read {}: {e}", entry_abs.display()))?,
    };
    let synthetic = format!(
        "\n\npage {UNIT_PREVIEW_PAGE} {{\n{}  title = \"{id}\"\n  project {{ from = {gather}.{id}.body }}\n}}\n",
        unit_page_sites(&doc)
    );
    overlay.insert(entry_abs.to_path_buf(), base + &synthetic);
    Ok(())
}

/// The `sites` line the synthetic unit page needs, or `""` when the
/// document declares at most one site.
///
/// A multi-site document rejects an untagged page, and this one joins
/// EVERY declared site: it is built under whichever site the caller
/// targets — all of them on a merged build — so naming one would be a
/// second answer that could disagree with the build target.
fn unit_page_sites(doc: &wcl_lang::Document) -> String {
    let declared: Vec<Option<String>> = doc
        .blocks()
        .filter(|b| b.kind() == "site")
        .map(|b| match b.labels().ok()?.into_iter().next()? {
            wcl_lang::Value::Identifier(s)
            | wcl_lang::Value::Utf8(s)
            | wcl_lang::Value::Symbol(s) => Some(s),
            _ => None,
        })
        .collect();
    if declared.len() < 2 {
        return String::new();
    }
    let names: Vec<String> = declared.iter().flatten().map(|n| format!(":{n}")).collect();
    format!("  sites = [{}]\n", names.join(", "))
}

/// Every file under `root`, as `/`-separated paths relative to it.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The page the preview iframe should open: `index.html` when the build
/// produced one, else the first `.html` directly under `out`.
fn index_page(out: &Path) -> Option<String> {
    if out.join("index.html").is_file() {
        return Some("index.html".to_string());
    }
    let mut pages: Vec<String> = std::fs::read_dir(out)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.ends_with(".html") && e.file_type().ok()?.is_file()).then_some(name)
        })
        .collect();
    pages.sort();
    pages.into_iter().next()
}

/// `GET /api/preview/{*path}` — serve a rendered file from the preview
/// scratch tree (the iframe target). No editor/reload scripts are involved:
/// the scratch tree holds a plain wdoc build.
///
/// A page left stale by a targeted rebuild is lazily re-rendered (with the
/// same inputs as the last `/api/preview` build) before it's served, so
/// navigating the preview always shows the last Rebuild's state without
/// paying for a full site build up front.
pub(super) async fn handle_preview_file(
    State(state): State<Arc<EditorState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    match preview_file(&state, &path).await {
        Ok((file, bytes)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type(&file))],
            bytes,
        )
            .into_response(),
        Err(e) if e.starts_with("bad ") => json_error(StatusCode::BAD_REQUEST, &e),
        Err(e) => json_error(StatusCode::NOT_FOUND, &e),
    }
}

/// Resolve a scratch-tree path, lazily re-render it if the last build left
/// it stale, and read it back — the whole of what serving a preview file
/// means, minus the response encoding.
pub(super) async fn preview_file(
    state: &Arc<EditorState>,
    path: &str,
) -> Result<(PathBuf, Vec<u8>), String> {
    let rel = Path::new(path);
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("bad preview path".to_string());
    }
    let file = state.preview.root().join(rel);
    lazy_page_rebuild(state, path, &file).await;
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|_| "no such preview file".to_string())?;
    Ok((file, bytes))
}

/// Lazily materialize a stale preview page before it's served. Only
/// `sites/<slug>/…/<stem>.html` requests with a recorded [`PreviewSession`]
/// participate; everything else (assets, skill files, unknown slugs) serves
/// as-is. Best-effort: a failing build logs and serves the stale file — a
/// broken document surfaces through the Rebuild button, not navigation.
async fn lazy_page_rebuild(state: &Arc<EditorState>, rel: &str, file: &Path) {
    let Some(slug) = rel
        .strip_prefix("sites/")
        .and_then(|r| r.split('/').next())
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    if !rel.ends_with(".html") {
        return;
    }
    let Some(stem) = file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
    else {
        return;
    };
    let slug = slug.to_string();

    // Cheap pre-check without the build gate: right after a full build
    // (the common case) everything is fresh.
    {
        let sessions = state.sessions.lock();
        let Some(s) = sessions.get(&slug) else { return };
        if s.maybe_fresh(&stem) {
            return;
        }
    }

    // Serialize behind the preview gate, re-check (another request may have
    // materialized the page while we waited), then build off the executor.
    let _gate = state.preview.lock().await;
    let state2 = Arc::clone(state);
    let file2 = file.to_path_buf();
    let rel2 = rel.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        // Map the html file back to its page through the sibling manifest a
        // full build wrote. No manifest / unknown stem (a chooser index, an
        // included sub-site's page the root entry's targeted path can't
        // reach) ⇒ `None` ⇒ full rebuild.
        let page = manifest_page(&file2, &stem);
        let (entry_abs, site, all_sites, overlay, plan) = {
            let sessions = state2.sessions.lock();
            let Some(s) = sessions.get(&slug) else { return };
            let Some(plan) = s.lazy_plan(page.as_deref()) else {
                return;
            };
            (
                s.entry_abs.clone(),
                s.site.clone(),
                s.all_sites,
                s.overlay.clone(),
                plan,
            )
        };
        let out = state2.preview.root().join("sites").join(&slug);
        match run_preview_build(BuildJob {
            entry_abs: &entry_abs,
            out: &out,
            site: site.as_deref(),
            all_sites,
            plan,
            overlay,
            changed: &[],
        }) {
            Ok(built) => {
                let mut sessions = state2.sessions.lock();
                if let Some(s) = sessions.get_mut(&slug) {
                    s.note(&built);
                }
            }
            Err(e) => eprintln!("preview: lazy rebuild for {rel2} failed: {e}"),
        }
    })
    .await;
}

/// The page name behind a built `<stem>.html`, read from the sibling
/// `_wdoc/pages.json` manifest the full build wrote: `index.html` maps to
/// the site's start page, any other stem must be listed. `None` when the
/// manifest is missing or the stem is unknown.
fn manifest_page(file: &Path, stem: &str) -> Option<String> {
    // The synthetic unit page maps to itself — its dir may have been built
    // cold-targeted (no manifest written), and falling back to a full book
    // build for the one page that never needs one would be exactly the
    // stall the targeted path exists to avoid.
    if stem == UNIT_PREVIEW_PAGE {
        return Some(UNIT_PREVIEW_PAGE.to_string());
    }
    let manifest = file.parent()?.join(wcl_wdoc::PAGES_MANIFEST_HREF);
    let text = std::fs::read_to_string(manifest).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if stem == "index" {
        return v.get("start")?.as_str().map(str::to_string);
    }
    v.get("pages")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|p| *p == stem)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::testsupport::{BODY_DOC, OBJECT_DOC, SITE_DOC, state_at};

    /// Two-page site for the targeted-rebuild / lazy-materialization tests:
    /// `alpha` is the start page (so `index.html` is its copy), `beta` the
    /// page a targeted alpha rebuild leaves stale.
    const TWO_PAGE_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage alpha {\n  start = true\n\n  h1 \"Alpha\"\n\n  p \"alpha original\"\n}\n\npage beta {\n  h1 \"Beta\"\n\n  p \"beta original\"\n}\n";

    /// Two-page site with per-site-hidden blocks for the merged all-views
    /// preview tests: both pages carry an `@except(sites = [:docs])` block
    /// a normal `docs` build drops.
    const MERGED_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage alpha {\n  start = true\n\n  h1 \"Alpha\"\n\n  p \"alpha original\"\n\n  @except(sites = [:docs]) p \"ALPHA_HIDDEN\"\n}\n\npage beta {\n  h1 \"Beta\"\n\n  p \"beta original\"\n\n  @except(sites = [:docs]) p \"BETA_HIDDEN\"\n}\n";

    /// A document with a gathered unit kind carrying an addressable `body`
    /// — but NO page for the unit — for the synthetic merged unit preview.
    const UNIT_BODY_DOC: &str = "import <wdoc.wcl>\n\n\
        @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
        @block(\"thing\")\ntype Thing {\n  @inline(0) id: identifier\n  @child(\"body\") body: wdoc.WdocAddressableBody?\n}\n\n\
        site docs {\n  title = \"The Docs\"\n  root = true\n}\n\n\
        thing alpha {\n  body {\n    p \"ALPHA_BODY_TEXT\"\n  }\n}\n\n\
        thing beta {\n}\n\n\
        page index {\n  start = true\n\n  h1 \"Home\"\n}\n";

    /// [`UNIT_BODY_DOC`] with a second site — where an untagged page is a
    /// build error, so the synthetic unit page has to name its sites.
    const UNIT_BODY_MULTISITE_DOC: &str = "import <wdoc.wcl>\n\n\
        @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
        @block(\"thing\")\ntype Thing {\n  @inline(0) id: identifier\n  @child(\"body\") body: wdoc.WdocAddressableBody?\n}\n\n\
        site docs {\n  title = \"The Docs\"\n  root = true\n}\n\n\
        site blog {\n  title = \"The Blog\"\n}\n\n\
        thing alpha {\n  body {\n    p \"ALPHA_BODY_TEXT\"\n  }\n}\n\n\
        page index {\n  sites = [:docs, :blog]\n  start = true\n\n  h1 \"Home\"\n}\n";

    fn write(dir: &Path, doc: &str) -> Arc<EditorState> {
        std::fs::write(dir.join("main.wcl"), doc).unwrap();
        state_at(dir)
    }

    /// The scratch-tree path behind a build's `href`.
    fn rel_of(href: &str) -> String {
        href.strip_prefix("/api/preview/").unwrap().to_string()
    }

    async fn page_text(state: &Arc<EditorState>, rel: &str) -> String {
        let (_, bytes) = preview_file(state, rel)
            .await
            .unwrap_or_else(|e| panic!("GET {rel}: {e}"));
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn builds_selected_site_with_overlay() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), SITE_DOC);

        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] }),
        )
        .expect("build");
        let href = v["href"].as_str().unwrap().to_string();
        assert!(href.starts_with("/api/preview/"), "{href}");
        page_text(&state, &rel_of(&href)).await;

        // Unsaved buffers overlay disk: the served HTML shows the edit.
        let edited = SITE_DOC.replace("Hello preview", "Overlaid text");
        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "site": "docs",
                "files": [{ "path": "main.wcl", "text": edited }],
            }),
        )
        .expect("rebuild");
        let html = page_text(&state, &rel_of(v["href"].as_str().unwrap())).await;
        assert!(html.contains("Overlaid text"), "overlay not applied");
        // Comment anchors for the preview pane's comment UI, plus the
        // edit-mode source anchors the edit_object jump keys on — but still
        // no injected scripts (the SPA drives the iframe from outside).
        assert!(html.contains("data-wcl-page-file"), "missing page anchor");
        assert!(html.contains("data-wcl-block"), "missing block anchors");
        assert!(
            html.contains("data-wcl-span"),
            "missing source span anchors"
        );
        assert!(!html.contains("<script"), "scripts injected into preview");
    }

    #[tokio::test]
    async fn targeted_rebuild_lazily_materializes_stale_pages() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), TWO_PAGE_DOC);

        // Cold dir: even with a page hint the first build is full.
        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "files": [], "pages": ["alpha"] }),
        )
        .expect("cold build");
        assert_eq!(v["mode"], "full");
        let base = rel_of(v["href"].as_str().unwrap())
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();

        // Warm dir + page hint: only alpha re-renders; beta.html goes stale.
        let edited = TWO_PAGE_DOC
            .replace("alpha original", "alpha edited")
            .replace("beta original", "beta edited");
        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "files": [{ "path": "main.wcl", "text": edited }],
                "pages": ["alpha"],
            }),
        )
        .expect("targeted build");
        assert_eq!(v["mode"], "targeted");
        let alpha = page_text(&state, &format!("{base}/alpha.html")).await;
        assert!(alpha.contains("alpha edited"), "targeted page not rebuilt");

        // Navigating to the stale page materializes it on request with the
        // same overlay the last build used.
        let beta = page_text(&state, &format!("{base}/beta.html")).await;
        assert!(
            beta.contains("beta edited"),
            "stale page not lazily rebuilt"
        );

        // `index.html` is a copy of the start page: a targeted beta rebuild
        // leaves it stale, and a lazy fetch resolves it to `alpha` through
        // the pages.json manifest (re-copying the landing file).
        let edited2 = edited.replace("alpha edited", "alpha edited again");
        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "files": [{ "path": "main.wcl", "text": edited2 }],
                "pages": ["beta"],
            }),
        )
        .expect("targeted beta");
        assert_eq!(v["mode"], "targeted");
        let index = page_text(&state, &format!("{base}/index.html")).await;
        assert!(
            index.contains("alpha edited again"),
            "stale start-page copy not lazily rebuilt"
        );

        // A page name that matches nothing in this entry's sites can't be
        // refreshed by the targeted path — automatic full fallback.
        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "files": [], "pages": ["no-such-page"] }),
        )
        .expect("fallback build");
        assert_eq!(v["mode"], "full");
    }

    #[tokio::test]
    async fn merged_renders_all_blocks_in_its_own_slug() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), MERGED_DOC);

        // Normal build: the `@except(:docs)` block is dropped.
        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] }),
        )
        .expect("normal build");
        let normal_base = rel_of(v["href"].as_str().unwrap())
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        assert!(!normal_base.contains("__merged"), "{normal_base}");
        let alpha = page_text(&state, &format!("{normal_base}/alpha.html")).await;
        assert!(!alpha.contains("ALPHA_HIDDEN"), "normal build filters");

        // Merged build: its own slug, every block rendered, visibility
        // stamped on the anchors.
        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true, "files": [],
            }),
        )
        .expect("merged build");
        let merged_base = rel_of(v["href"].as_str().unwrap())
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        assert!(merged_base.contains("__merged"), "{merged_base}");
        let alpha = page_text(&state, &format!("{merged_base}/alpha.html")).await;
        assert!(alpha.contains("ALPHA_HIDDEN"), "merged build shows all");
        assert!(
            alpha.contains("data-wcl-except=\"docs\""),
            "merged build stamps visibility: {alpha}"
        );

        // The normal slug's output is untouched by the merged build.
        let alpha = page_text(&state, &format!("{normal_base}/alpha.html")).await;
        assert!(!alpha.contains("ALPHA_HIDDEN"), "normal output poisoned");

        // A targeted merged rebuild of alpha leaves beta stale; the lazy
        // materialization must remember the merged flag (or beta would
        // silently re-render filtered).
        let edited = MERGED_DOC.replace("beta original", "beta edited");
        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl",
                "site": "docs",
                "merged": true,
                "files": [{ "path": "main.wcl", "text": edited }],
                "pages": ["alpha"],
            }),
        )
        .expect("targeted merged");
        assert_eq!(v["mode"], "targeted");
        let beta = page_text(&state, &format!("{merged_base}/beta.html")).await;
        assert!(
            beta.contains("beta edited"),
            "stale page not lazily rebuilt"
        );
        assert!(
            beta.contains("BETA_HIDDEN"),
            "lazy merged rebuild lost the all-sites flag"
        );

        // merged + skill is meaningless — an explicit error.
        let e = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true, "skill": true, "files": [],
            }),
        )
        .unwrap_err();
        assert!(e.contains("merged"), "{e}");
    }

    #[tokio::test]
    async fn merged_unit_builds_synthetic_body_page() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), UNIT_BODY_DOC);

        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true,
                "unit": { "kind": "thing", "id": "alpha" },
                "pages": [UNIT_PREVIEW_PAGE], "files": [],
            }),
        )
        .expect("unit build");
        let base = rel_of(v["href"].as_str().unwrap())
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        // ONE shared `__unit` slug for every unit of the (entry, site): the
        // synthetic page name is constant, so switching units is a warm
        // targeted rebuild instead of a cold full build per unit — and the
        // dir never collides with the plain merged output.
        assert!(base.contains("__unit"), "shared unit slug: {base}");
        let page = page_text(&state, &format!("{base}/{UNIT_PREVIEW_PAGE}.html")).await;
        assert!(page.contains("ALPHA_BODY_TEXT"), "body projected: {page}");
        // The projected blocks anchor to the REAL declaring file, so every
        // editing op (reorder, visibility, text) targets real source.
        assert!(
            page.contains("main.wcl\""),
            "anchors point at the unit's file: {page}"
        );

        // A unit without a body has nothing to render standalone.
        let e = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true,
                "unit": { "kind": "thing", "id": "beta" }, "files": [],
            }),
        )
        .unwrap_err();
        assert!(e.contains("no body"), "{e}");

        // `unit` without `merged` is an explicit error.
        assert!(
            preview_site(
                &state,
                &serde_json::json!({
                    "entry": "main.wcl", "site": "docs",
                    "unit": { "kind": "thing", "id": "alpha" }, "files": [],
                }),
            )
            .is_err()
        );
    }

    /// A multi-site entry rejects an untagged page, so the synthetic unit
    /// page joins every declared site — otherwise the content modal's
    /// merged tab and the screen editor would hard-fail on such an entry.
    #[tokio::test]
    async fn merged_unit_page_joins_every_site_in_a_multi_site_document() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), UNIT_BODY_MULTISITE_DOC);

        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "merged": true,
                "unit": { "kind": "thing", "id": "alpha" },
                "pages": [UNIT_PREVIEW_PAGE], "files": [],
            }),
        )
        .expect("unit build in a multi-site document");
        let base = rel_of(v["href"].as_str().unwrap())
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string();
        let page = page_text(&state, &format!("{base}/{UNIT_PREVIEW_PAGE}.html")).await;
        assert!(page.contains("ALPHA_BODY_TEXT"), "body projected: {page}");
    }

    #[tokio::test]
    async fn renders_edit_object_button() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), OBJECT_DOC);

        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] }),
        )
        .expect("build");
        let html = page_text(&state, &rel_of(v["href"].as_str().unwrap())).await;
        assert!(
            html.contains("data-wcl-edit-kind=\"thing\""),
            "missing edit_object button: {html}"
        );
        assert!(html.contains("data-wcl-edit-target=\"alpha\""));
    }

    /// A skill view previews as the built skill folder: the file listing +
    /// browsable contents, not an HTML site.
    #[tokio::test]
    async fn skill_builds_folder_listing() {
        let td = tempfile::tempdir().unwrap();
        let state = write(
            td.path(),
            "import <wdoc.wcl>\n\nsite skill {\n  default_template = :ai_skill\n  root = true\n\n  skill {\n    name = \"demo\"\n    description = \"A demo skill.\"\n  }\n}\n\npage index {\n  title = \"Demo skill\"\n  start = true\n\n  h1 \"Demo\"\n\n  p \"Start page prose.\"\n}\n\npage extra {\n  title = \"Extra\"\n\n  p \"Reference prose.\"\n}\n",
        );

        let v = preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "skill", "files": [], "skill": true,
            }),
        )
        .expect("skill build");
        assert_eq!(v["mode"], "skill");
        let files: Vec<&str> = v["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f.as_str())
            .collect();
        assert!(files.contains(&"SKILL.md"), "{files:?}");
        assert!(
            files.iter().any(|f| f.starts_with("references/")),
            "{files:?}"
        );
        // The listed files serve through the preview file path.
        let base = rel_of(v["base"].as_str().unwrap());
        let text = page_text(&state, &format!("{base}SKILL.md")).await;
        assert!(text.contains("Start page prose."), "{text}");
    }

    /// A targeted rebuild after a source edit picks the edit up: the client
    /// commits, then asks for just the page it is looking at.
    #[tokio::test]
    async fn targeted_rebuild_after_edit() {
        let td = tempfile::tempdir().unwrap();
        let state = write(td.path(), BODY_DOC);

        let v = preview_site(
            &state,
            &serde_json::json!({ "entry": "main.wcl", "site": "docs", "files": [] }),
        )
        .expect("cold build");
        assert_eq!(v["mode"], "full");
        let href = rel_of(v["href"].as_str().unwrap());

        // A commit lands on disk; the client then asks for just the page it
        // is looking at, naming the file that changed.
        let disk = std::fs::read_to_string(td.path().join("main.wcl")).unwrap();
        std::fs::write(
            td.path().join("main.wcl"),
            disk.replace("First paragraph", "Rebuilt text"),
        )
        .unwrap();
        preview_site(
            &state,
            &serde_json::json!({
                "entry": "main.wcl", "site": "docs", "files": [],
                "pages": ["index"], "changed": ["main.wcl"],
            }),
        )
        .expect("targeted rebuild");
        let html = page_text(&state, &href).await;
        assert!(
            html.contains("Rebuilt text"),
            "targeted rebuild missed the edit"
        );
    }

    #[tokio::test]
    async fn rejects_escaping_entry() {
        let td = tempfile::tempdir().unwrap();
        let state = state_at(td.path());
        let e = preview_site(
            &state,
            &serde_json::json!({ "entry": "../escape.wcl", "site": null, "files": [] }),
        )
        .unwrap_err();
        assert!(e.contains("outside"), "{e}");
    }

    fn target(entry: &str, site: Option<&str>) -> PreviewTarget {
        PreviewTarget {
            entry: entry.to_string(),
            site: site.map(str::to_string),
            merged: false,
            unit: false,
        }
    }

    fn session(generation: u64, full_gen: u64, pages: &[(&str, u64)]) -> PreviewSession {
        PreviewSession {
            entry_abs: PathBuf::from("/tmp/main.wcl"),
            site: None,
            all_sites: false,
            overlay: HashMap::new(),
            generation,
            full_gen,
            page_gen: pages.iter().map(|(p, g)| ((*p).to_string(), *g)).collect(),
        }
    }

    fn request(t: PreviewTarget, pages: &[&str], changed: &[&str]) -> BuildRequest {
        BuildRequest {
            target: t,
            pages: (!pages.is_empty()).then(|| pages.iter().map(|p| (*p).to_string()).collect()),
            changed: changed.iter().map(PathBuf::from).collect(),
        }
    }

    fn targeted_pages(plan: &BuildPlan) -> Option<Vec<String>> {
        match plan {
            BuildPlan::Full => None,
            BuildPlan::Targeted { pages } => Some({
                let mut v: Vec<String> = pages.clone().unwrap_or_default().into_iter().collect();
                v.sort();
                v
            }),
        }
    }

    #[test]
    fn slug_is_stable_for_the_same_target() {
        assert_eq!(
            target("docs/main.wcl", Some("book")).slug(),
            target("docs/main.wcl", Some("book")).slug()
        );
    }

    #[test]
    fn slug_separates_entries_that_sanitize_alike() {
        let a = target("docs/main.wcl", Some("book"));
        let b = target("docs_main.wcl", Some("book"));
        assert_ne!(a.slug(), b.slug());
        // …and a site name can't imitate the entry/site boundary either.
        assert_ne!(
            target("docs", Some("a_b")).slug(),
            target("docs", Some("a/b")).slug(),
        );
        assert_ne!(target("docs", None).slug(), target("docs", Some("")).slug());
    }

    #[test]
    fn slug_separates_the_render_variants() {
        let plain = target("docs/main.wcl", Some("book"));
        let merged = PreviewTarget {
            merged: true,
            ..plain.clone()
        };
        let unit = PreviewTarget {
            unit: true,
            ..merged.clone()
        };
        let slugs = [plain.slug(), merged.slug(), unit.slug()];
        assert_eq!(
            slugs.iter().collect::<std::collections::HashSet<_>>().len(),
            3
        );
        // The readable prefix still says which is which.
        assert!(slugs[1].contains("__merged"), "{}", slugs[1]);
        assert!(slugs[2].contains("__merged__unit"), "{}", slugs[2]);
    }

    #[test]
    fn slug_is_a_safe_directory_name() {
        let s = target("a/b/../c.wcl", Some("x y")).slug();
        assert!(
            s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{s}"
        );
    }

    #[test]
    fn plan_build_table() {
        let t = target("docs/main.wcl", Some("book"));
        let warm = session(1, 1, &[]);

        // Cold output dir: nothing to render into, whatever the hints say.
        assert_eq!(
            request(t.clone(), &["alpha"], &[]).plan(None),
            BuildPlan::Full
        );
        assert_eq!(
            request(t.clone(), &[], &["a.wcl"]).plan(None),
            BuildPlan::Full
        );

        // Warm with no hint at all: the whole site.
        assert_eq!(
            request(t.clone(), &[], &[]).plan(Some(&warm)),
            BuildPlan::Full
        );

        // Warm with an explicit page: just that page.
        let plan = request(t.clone(), &["alpha"], &[]).plan(Some(&warm));
        assert_eq!(
            targeted_pages(&plan).as_deref(),
            Some(&["alpha".to_string()][..])
        );

        // Warm with only changed files: targeted, filter left to the engine.
        let plan = request(t.clone(), &[], &["a.wcl"]).plan(Some(&warm));
        assert_eq!(targeted_pages(&plan), Some(vec![]));

        // A unit preview is warm on a cold dir — its slug only ever serves
        // the one synthetic page.
        let unit = PreviewTarget {
            merged: true,
            unit: true,
            ..t
        };
        let plan = request(unit, &[UNIT_PREVIEW_PAGE], &[]).plan(None);
        assert_eq!(
            targeted_pages(&plan).as_deref(),
            Some(&[UNIT_PREVIEW_PAGE.to_string()][..]),
        );
    }

    #[test]
    fn lazy_plan_table() {
        // Right after a full build everything is fresh.
        let full = session(3, 3, &[]);
        assert_eq!(full.lazy_plan(Some("alpha")), None);
        assert_eq!(full.lazy_plan(None), None);
        assert!(full.maybe_fresh("index"));

        // After a targeted rebuild only the rendered page is.
        let targeted = session(4, 3, &[("alpha", 4)]);
        assert_eq!(targeted.lazy_plan(Some("alpha")), None);
        assert!(targeted.maybe_fresh("alpha"));
        assert_eq!(
            targeted_pages(&targeted.lazy_plan(Some("beta")).unwrap()).as_deref(),
            Some(&["beta".to_string()][..]),
        );
        // An unresolvable stem can only be answered by a full build.
        assert_eq!(targeted.lazy_plan(None), Some(BuildPlan::Full));
        // index.html is a copy of the start page: its freshness needs the
        // manifest, so the cheap pre-check never claims it.
        assert!(!targeted.maybe_fresh("index"));
    }
}
