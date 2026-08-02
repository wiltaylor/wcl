use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedReceiver;
use wcl_wdoc::{BuildOptions, RebuildOutcome, build_incremental, build_with_options};

/// How long the watch loop waits for the event stream to go quiet
/// before rebuilding — one editor save fires several notify events,
/// which should coalesce into a single build.
const QUIET_WINDOW: Duration = Duration::from_millis(150);

/// How long a live-reload long-poll request parks before answering
/// with the unchanged generation. Short enough that intermediaries
/// don't kill the connection; the client just re-polls.
pub(crate) const POLL_TIMEOUT: Duration = Duration::from_secs(30);

/// The address `serve` binds to when neither `--addr` nor `auto` shifts
/// it elsewhere. `auto` scans upward from this port.
pub(crate) const DEFAULT_BIND: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 8080);

/// How the dev server chooses its bind address.
///
/// `--addr auto` picks the first free port near [`DEFAULT_BIND`]; any other
/// value is parsed as an explicit `SocketAddr` and bound as-is (hard error if
/// the port is busy).
#[derive(Debug, Clone, Copy)]
pub(crate) enum BindSpec {
    /// Scan upward from [`DEFAULT_BIND`] for a free port.
    Auto,
    /// Bind exactly this address.
    Fixed(SocketAddr),
}

impl std::str::FromStr for BindSpec {
    type Err = std::net::AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(BindSpec::Auto)
        } else {
            s.parse().map(BindSpec::Fixed)
        }
    }
}

/// Bind a listener by scanning a fixed window of ports upward from `base`,
/// returning the first that's free. Keeping the successfully bound listener
/// avoids a check-then-bind race.
pub(crate) async fn bind_auto(base: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    const RANGE: u16 = 100; // scan base..base+100
    let mut last_err = None;
    for offset in 0..RANGE {
        let Some(port) = base.port().checked_add(offset) else {
            break;
        };
        let cand = SocketAddr::new(base.ip(), port);
        match tokio::net::TcpListener::bind(cand).await {
            Ok(l) => return Ok(l),
            Err(e) if e.kind() == ErrorKind::AddrInUse => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(ErrorKind::AddrInUse, "no free port found near default")
    }))
}

/// Injected into every served HTML page (and the error/404 pages):
/// long-polls `/__wdoc_reload` and reloads when the build generation
/// changes, so the browser tracks rebuilds — including the flip
/// between a working site and the build-failure page.
const RELOAD_SCRIPT: &str = "<script>(async()=>{const u='/__wdoc_reload';let g=null;\
for(;;){try{const r=await fetch(g===null?u:u+'?gen='+encodeURIComponent(g));\
const t=(await r.text()).trim();if(g!==null&&t!==g){location.reload();return}g=t}\
catch(e){await new Promise(r=>setTimeout(r,1000))}}})();</script>";

/// Shared between the rebuild loop and the request handlers.
struct ServeState {
    out: PathBuf,
    /// Plain-text rendering of the most recent failed build; `None`
    /// when the last build succeeded. While set, HTML requests get a
    /// build-failure page instead of the stale previous build.
    error: RwLock<Option<String>>,
    /// Bumped after every build attempt — success *and* failure — so
    /// a browser parked on the error page reloads when the fix lands.
    generation: tokio::sync::watch::Sender<u64>,
    /// Watched source root; `/__wdoc_rebuild` page paths are sandboxed to it.
    watch_root: PathBuf,
    /// The `training.wcl` course answers are recorded into — resolved once at
    /// startup to the owning wskill (the served entry is usually below it).
    training_sidecar: PathBuf,
    /// Source `.wcl` files changed since the last rebuild. The watcher
    /// accumulates here instead of rebuilding; a rebuild is triggered manually
    /// (Enter in the console, or `POST /__wdoc_rebuild`) and drains this.
    pending: Mutex<Vec<PathBuf>>,
    /// Send a [`RebuildReq`] to request a rebuild. The console (stdin Enter)
    /// and the `/__wdoc_rebuild` endpoint both use it; the rebuild worker runs
    /// one build per request and (when asked) reports completion back.
    rebuild_tx: tokio::sync::mpsc::UnboundedSender<RebuildReq>,
}

/// A rebuild request handed to the rebuild worker.
struct RebuildReq {
    /// The page the request came from (`POST /__wdoc_rebuild`), used to
    /// scope the rebuild to that page's included sub-site. `None` for a console
    /// (Enter) rebuild, which rebuilds the whole served site.
    page_file: Option<PathBuf>,
    /// Resolved when the build finishes, so the HTTP handler can report the
    /// result. `None` for the console path.
    done: Option<tokio::sync::oneshot::Sender<RebuildReport>>,
}

/// What a rebuild did, returned to the `/__wdoc_rebuild` caller.
struct RebuildReport {
    ok: bool,
    /// What was rebuilt — `"site"` or the sub-site's output subdir.
    scope: String,
    /// Human summary (page count, or the first line of the error).
    summary: String,
}

/// Print any non-fatal edge warnings left by the most recent build (edges
/// whose endpoint matched no shape id) to stderr.
fn print_edge_warnings() {
    for w in wcl_wdoc::take_render_warnings() {
        eprintln!("warning: {w}");
    }
}

/// Run one build, report to stderr, record the outcome in `state`, and
/// bump the live-reload generation.
fn run_build(file: &Path, out: &Path, site: Option<&str>, state: &ServeState, rebuild: bool) {
    let opts = BuildOptions::default();
    match build_with_options(file, out, site, &opts).map(|(n, _)| n) {
        Ok(n) => {
            print_edge_warnings();
            let plural = if n == 1 { "" } else { "s" };
            if rebuild {
                eprintln!("rebuilt: {n} page{plural}");
            } else {
                eprintln!("rendered {n} page{plural}");
            }
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
        }
        Err(err) => {
            eprintln!(
                "{} failed:",
                if rebuild { "rebuild" } else { "initial build" }
            );
            err.report();
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(err.render_plain());
        }
    }
    state.generation.send_modify(|g| *g += 1);
}

/// Handle one [`RebuildReq`]. When the request names a page that belongs to an
/// included sub-site (e.g. a wskill), rebuild **only** that sub-site into its
/// output subdir, draining just the pending changes under it — so a rebuild
/// requested from a sub-site page is fast and scoped. Otherwise (console Enter,
/// or a root page) drain all pending and rebuild the top-level site. Records
/// the outcome in `state`, bumps the live-reload generation, and returns a
/// report.
fn run_rebuild_request(
    file: &Path,
    out: &Path,
    site: Option<&str>,
    state: &ServeState,
    page_file: Option<PathBuf>,
) -> RebuildReport {
    let opts = BuildOptions::default();

    // Scope to the page's sub-site when the request names one.
    if let Some(pf) = page_file.as_deref()
        && let Some(sub) = wcl_wdoc::subsite_for_page(file, pf)
    {
        let changed = drain_pending_under(state, Some(&sub.src_root));
        let sub_out = out.join(&sub.out_subdir);
        let scope = sub.out_subdir.display().to_string();
        let result = build_incremental(&sub.entry, &sub_out, sub.site.as_deref(), &opts, &changed);
        return finish_rebuild(state, scope, result.map(rebuild_summary));
    }

    // Whole-site rebuild: drain everything pending; full when nothing's pending.
    let changed = drain_pending_under(state, None);
    let result = if changed.is_empty() {
        build_with_options(file, out, site, &opts).map(|(n, _)| format!("{} (full)", page_count(n)))
    } else {
        build_incremental(file, out, site, &opts, &changed).map(rebuild_summary)
    };
    finish_rebuild(state, "site".to_string(), result)
}

/// Drain pending changed paths: those under `scope` (a sub-site source root),
/// leaving the rest queued; or *all* of them when `scope` is `None`.
fn drain_pending_under(state: &ServeState, scope: Option<&Path>) -> Vec<PathBuf> {
    let mut g = state.pending.lock().unwrap_or_else(|e| e.into_inner());
    let mut taken = match scope {
        None => std::mem::take(&mut *g),
        Some(root) => {
            let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            let (under, rest): (Vec<_>, Vec<_>) =
                std::mem::take(&mut *g).into_iter().partition(|p| {
                    std::fs::canonicalize(p)
                        .map(|c| c.starts_with(&root))
                        .unwrap_or(false)
                });
            *g = rest;
            under
        }
    };
    taken.sort();
    taken.dedup();
    taken
}

/// Record a build outcome in `state`, bump the live-reload generation, and
/// build the [`RebuildReport`]. `result` is `Ok(summary)` or the build error.
fn finish_rebuild(
    state: &ServeState,
    scope: String,
    result: Result<String, wcl_wdoc::BuildError>,
) -> RebuildReport {
    let report = match result {
        Ok(summary) => {
            print_edge_warnings();
            eprintln!("rebuilt {scope}: {summary}");
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
            RebuildReport {
                ok: true,
                scope,
                summary,
            }
        }
        Err(err) => {
            eprintln!("rebuild failed ({scope}):");
            err.report();
            let plain = err.render_plain();
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(plain.clone());
            RebuildReport {
                ok: false,
                scope,
                summary: plain.lines().next().unwrap_or("build failed").to_string(),
            }
        }
    };
    state.generation.send_modify(|g| *g += 1);
    report
}

/// A one-line summary of an incremental rebuild outcome.
fn rebuild_summary(outcome: RebuildOutcome) -> String {
    match outcome {
        RebuildOutcome::Targeted { pages } => {
            format!("{} ({})", page_count(pages.len()), pages.join(", "))
        }
        RebuildOutcome::Full { pages } => format!("{} (full)", page_count(pages)),
    }
}

/// `"1 page"` / `"3 pages"`.
fn page_count(n: usize) -> String {
    format!("{n} page{}", if n == 1 { "" } else { "s" })
}

pub(crate) async fn serve(
    file: PathBuf,
    out: Option<PathBuf>,
    addr: BindSpec,
    site: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the output directory. If `--out` wasn't given, create a
    // TempDir and hold it for the lifetime of `serve` so cleanup runs
    // when the future is dropped (Ctrl-C / shutdown).
    let (out_dir, _tempdir_guard): (PathBuf, Option<TempDir>) = match out {
        Some(p) => {
            std::fs::create_dir_all(&p)?;
            (p, None)
        }
        None => {
            let td = tempfile::Builder::new().prefix("wdoc-").tempdir()?;
            (td.path().to_path_buf(), Some(td))
        }
    };

    // Hard stop on Ctrl-C. A *dedicated* task owns the kill so the signal is
    // observed even while the watch loop is mid-rebuild: `run_build` is a
    // synchronous, non-cancellable call that blocks its worker thread, so a
    // shutdown branch sharing the `select!` task below would never get polled
    // until the build finished. `process::exit` tears down every thread at
    // once (the inotify watcher, axum connections, parked reload long-polls,
    // and any in-flight build) and, by ending the process, blocks all further
    // rebuilds. It skips the TempDir guard's `Drop`, so clean the temp output
    // dir by hand first.
    let temp_cleanup = _tempdir_guard.as_ref().map(|td| td.path().to_path_buf());
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down");
        if let Some(p) = temp_cleanup {
            let _ = std::fs::remove_dir_all(&p);
        }
        std::process::exit(0);
    });

    let watch_root = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Rebuilds are triggered manually (not on every file change). Both the
    // console (stdin Enter) and the `/__wdoc_rebuild` endpoint send on this.
    let (rebuild_tx, mut rebuild_rx) = tokio::sync::mpsc::unbounded_channel::<RebuildReq>();

    let state = Arc::new(ServeState {
        out: out_dir.clone(),
        error: RwLock::new(None),
        generation: tokio::sync::watch::Sender::new(0),
        watch_root: watch_root.clone(),
        training_sidecar: wcl_wdoc::training::sidecar_for(&watch_root, wcl_wskill::ROOT_MARKER),
        pending: Mutex::new(Vec::new()),
        rebuild_tx: rebuild_tx.clone(),
    });

    // Initial build. Failure is non-fatal — HTML requests serve the
    // build-failure page until the next (manual) rebuild succeeds.
    run_build(&file, &out_dir, site.as_deref(), &state, false);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    // Console driver: pressing Enter requests a rebuild. A blocking thread (not
    // an async stdin reader, which can stall process exit) feeds the trigger
    // channel; EOF (no interactive console — piped/closed stdin) just ends the
    // thread so non-interactive runs don't spin.
    {
        let trigger = rebuild_tx.clone();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                match stdin.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        // A console Enter rebuilds the whole served site.
                        if trigger
                            .send(RebuildReq {
                                page_file: None,
                                done: None,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
    }

    let bg_state = Arc::clone(&state);
    // The watcher no longer rebuilds — it accumulates the changed `.wcl` paths
    // into `state.pending` and notifies the console. A *local* future (not a
    // detached task) so it drops with `serve`, stopping the inotify thread.
    let watch_loop = async move {
        let _watcher = watcher; // keep the watcher alive for this future's lifetime
        while let Some(event) = rx.recv().await {
            if !is_relevant(&event) {
                continue;
            }
            // Coalesce the notify event storm one save fires into a single note.
            let mut changed: Vec<PathBuf> = wcl_paths(&event);
            drain_quiet(&mut rx, &mut changed).await;
            let n = changed.len();
            bg_state
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(changed);
            eprintln!(
                "{n} file change{} pending — press Enter to rebuild",
                if n == 1 { "" } else { "s" }
            );
        }
    };

    let rb_file = file.clone();
    let rb_out = out_dir.clone();
    let rb_site = site.clone();
    let rb_state = Arc::clone(&state);
    // Rebuild worker: one build per request. Scopes to the request's sub-site
    // when given (`POST /__wdoc_rebuild` with a `page_file`), else rebuilds
    // the whole site (console Enter). Reports completion back when asked.
    let rebuild_loop = async move {
        while let Some(req) = rebuild_rx.recv().await {
            let report = run_rebuild_request(
                &rb_file,
                &rb_out,
                rb_site.as_deref(),
                &rb_state,
                req.page_file,
            );
            if let Some(done) = req.done {
                let _ = done.send(report);
            }
        }
    };

    // One generic static handler resolves any request path against the
    // output tree, so it serves both the flat single-site layout and the
    // nested multi-site one (`/<site>/…`, `/<site>/_wdoc/…`) plus the
    // generated chooser at `/`, with no per-route knowledge. The reload
    // endpoint sits outside the output tree's namespace.
    let app = Router::new()
        .route("/__wdoc_reload", get(handle_reload))
        .route("/__wdoc_rebuild", axum::routing::post(handle_rebuild))
        .route(
            "/__wdoc_training/answer",
            axum::routing::post(handle_training_answer),
        )
        .route("/__wdoc_training/state", get(handle_training_state))
        .fallback(get(handle_static))
        .with_state(Arc::clone(&state))
        .layer(middleware::from_fn(log_requests));

    let listener = match addr {
        BindSpec::Auto => bind_auto(DEFAULT_BIND).await?,
        BindSpec::Fixed(a) => tokio::net::TcpListener::bind(a).await?,
    };
    let bound = listener.local_addr()?;
    println!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );
    println!("auto-rebuild is off — press Enter here to rebuild after edits");

    // Run the server, the watcher, and the rebuild worker concurrently. None
    // completes in normal operation — they run until the Ctrl-C task above
    // hard-exits the process. No graceful shutdown: a parked reload long-poll
    // (up to `POLL_TIMEOUT`) must not delay teardown.
    tokio::select! {
        res = axum::serve(listener, app).into_future() => res?,
        _ = watch_loop => {}
        _ = rebuild_loop => {}
    }
    Ok(())
}

/// Keep draining the event channel until `QUIET_WINDOW` passes with no
/// further *relevant* event. Irrelevant events (e.g. the build's own
/// output writes when `--out` sits inside the watched tree) are
/// swallowed without extending the window.
async fn drain_quiet(rx: &mut UnboundedReceiver<Event>, changed: &mut Vec<PathBuf>) {
    let mut deadline = tokio::time::Instant::now() + QUIET_WINDOW;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) if is_relevant(&ev) => {
                changed.extend(wcl_paths(&ev));
                deadline = tokio::time::Instant::now() + QUIET_WINDOW;
            }
            Ok(Some(_)) => {}
            // Window elapsed quiet, or the channel closed (the outer
            // loop's recv will observe the closure).
            Err(_) | Ok(None) => break,
        }
    }
}

fn is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| is_source_wcl(p))
}

/// A `.wcl` path that is part of the document — i.e. not one of the sidecars
/// ([`SIDECARS`]), whose writes must never trigger a rebuild (they render
/// client-side, so the build doesn't read them).
fn is_source_wcl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "wcl")
        && !p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| SIDECARS.contains(&n))
}

/// Sidecar file names: data *about* a document, written beside it. A rebuild
/// must never be triggered by one — a review comment or a course answer is
/// read by the client, not by the build.
const SIDECARS: [&str; 2] = ["comments.wcl", "training.wcl"];

/// The document `.wcl` paths an event touched (the granularity
/// `build_incremental` maps onto pages); `comments.wcl` sidecars are excluded.
fn wcl_paths(event: &Event) -> Vec<PathBuf> {
    event
        .paths
        .iter()
        .filter(|p| is_source_wcl(p))
        .cloned()
        .collect()
}

/// Live-reload long-poll. Without `?gen=`, answers immediately with the
/// current build generation. With `?gen=N` matching the current value,
/// parks until the next build attempt (or `POLL_TIMEOUT`) and answers
/// with the then-current generation; the client reloads on a change.
async fn handle_reload(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let asked: Option<u64> = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("gen=")))
        .and_then(|v| v.parse().ok());
    if asked == Some(*state.generation.borrow()) {
        let mut rx = state.generation.subscribe();
        let _ = tokio::time::timeout(POLL_TIMEOUT, rx.changed()).await;
    }
    let current = *state.generation.borrow();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        current.to_string(),
    )
        .into_response()
}

/// Request a rebuild over HTTP. Scopes to the current page's sub-site (from
/// the optional `page_file` body field) and **waits** for the build to finish,
/// so the caller can show a running/done indication; the reload long-poll then
/// reloads the page when the generation bumps. A missing `page_file` (or a
/// root page) rebuilds the whole site.
async fn handle_rebuild(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let page_file = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("page_file")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty())
        .and_then(|f| sandboxed(&state.watch_root, Path::new(&f)));

    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .rebuild_tx
        .send(RebuildReq {
            page_file,
            done: Some(tx),
        })
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild worker is gone");
    }
    match rx.await {
        Ok(report) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "ok": report.ok,
                "scope": report.scope,
                "summary": report.summary,
            }),
        ),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild was cancelled"),
    }
}

/// `POST /__wdoc_training/answer` — record one course answer into the
/// `training.wcl` sidecar beside the owning wskill.
///
/// The training site is static-first: it keeps progress in localStorage and
/// only calls this when a server happens to be running, so a failure here is
/// never fatal to the learner (the page falls back to self-check). Recording
/// an answer writes only the sidecar, which the watcher ignores — no rebuild.
///
/// Body: `{ course, lesson, check, response, status }`. `status` is `pending`
/// for a free-text answer awaiting an agent, or an already-decided
/// `correct` / `wrong` for multiple choice.
async fn handle_training_answer(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        return json_error(StatusCode::BAD_REQUEST, "bad JSON body");
    };
    let field = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (check, response) = (field("check"), field("response"));
    if check.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "missing `check`");
    }
    let (course, lesson, status) = (field("course"), field("lesson"), field("status"));
    let status = if status.is_empty() {
        "pending".to_string()
    } else {
        status
    };

    // The sidecar was resolved at startup (beside the owning wskill), so the
    // client never supplies a path it could forge.
    let file = state.training_sidecar.clone();
    match tokio::task::spawn_blocking(move || {
        wcl_wdoc::training::record(&file, &course, &lesson, &check, &response, &status)
    })
    .await
    {
        Ok(Ok(id)) => json_response(StatusCode::OK, &serde_json::json!({ "ok": true, "id": id })),
        Ok(Err(e)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "record task failed"),
    }
}

/// `GET /__wdoc_training/state?check=…&course=…` — the current record for one
/// check, so the page can show a grader's verdict.
///
/// Long-polls: while the answer is still `pending` the request parks until the
/// sidecar changes (or [`POLL_TIMEOUT`] elapses and the client asks again), so
/// a verdict written by `wcl wdoc training grade` appears without a reload.
async fn handle_training_state(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let Some(check) = query_param(&uri, "check") else {
        return json_error(StatusCode::BAD_REQUEST, "missing `check`");
    };
    let course = query_param(&uri, "course").unwrap_or_default();
    let file = state.training_sidecar.clone();

    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    loop {
        let (file2, course2, check2) = (file.clone(), course.clone(), check.clone());
        let found = tokio::task::spawn_blocking(move || {
            wcl_wdoc::training::find_in(&file2, &course2, &check2)
        })
        .await;
        match found {
            Ok(Some(rec)) if rec.status != "pending" => {
                return json_response(
                    StatusCode::OK,
                    &serde_json::json!({
                        "ok": true,
                        "status": "graded",
                        "verdict": rec.verdict,
                        "score": rec.score,
                        "pass": rec.passed(),
                    }),
                );
            }
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "lookup task failed"),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return json_response(
                StatusCode::OK,
                &serde_json::json!({ "ok": true, "status": "pending" }),
            );
        }
        // No file watch on the sidecar (the watcher deliberately ignores it),
        // so poll it on a slow tick — a grader's verdict is a human-scale event.
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}

/// Read the `kind` query parameter (`?kind=...`), URL-decoding `%xx` / `+`.
pub(crate) fn query_param(uri: &Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| url_decode(v))
    })
}

/// Minimal `application/x-www-form-urlencoded` decode for query values.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse a JSON request body, or an error message on malformed JSON.
pub(crate) fn parse_json_body(body: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(body).map_err(|e| format!("bad json: {e}"))
}

/// Canonicalize `file` and confirm it sits inside `root`, so a comment / edit
/// write can't escape the served source tree. Returns the canonical path to
/// edit.
pub(crate) fn sandboxed(root: &Path, file: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(root).ok()?;
    let file = std::fs::canonicalize(file).ok()?;
    file.starts_with(&root).then_some(file)
}

/// Like [`sandboxed`], but for a target that may not exist yet (a new-file
/// save): canonicalize the nearest existing ancestor, confirm containment,
/// then re-append the non-existing remainder — which must be plain path
/// segments (no `..`, no roots) so the remainder can't climb back out.
pub(crate) fn sandboxed_create(root: &Path, file: &Path) -> Option<PathBuf> {
    if let Some(existing) = sandboxed(root, file) {
        return Some(existing);
    }
    let root = std::fs::canonicalize(root).ok()?;
    let mut ancestor = file.parent()?;
    let mut remainder = vec![file.file_name()?.to_os_string()];
    let canon_ancestor = loop {
        match std::fs::canonicalize(ancestor) {
            Ok(c) => break c,
            Err(_) => {
                remainder.push(ancestor.file_name()?.to_os_string());
                ancestor = ancestor.parent()?;
            }
        }
    };
    if !canon_ancestor.starts_with(&root) {
        return None;
    }
    if remainder.iter().any(|seg| {
        seg.to_str()
            .is_none_or(|s| s == ".." || s == "." || s.contains('\\') || s.contains('/'))
    }) {
        return None;
    }
    let mut out = canon_ancestor;
    for seg in remainder.into_iter().rev() {
        out.push(seg);
    }
    Some(out)
}

pub(crate) fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        value.to_string(),
    )
        .into_response()
}

pub(crate) fn json_error(status: StatusCode, msg: &str) -> Response {
    json_response(status, &serde_json::json!({ "error": msg }))
}

/// Resolve any request path to a file under the output tree and serve
/// it. Handles `/` and directory paths (→ `index.html`), extension-less
/// page names (→ `<name>.html`), and explicit files (`.html`, and the
/// `_wdoc/` assets at any depth). Rejects `..` / backslash components so
/// the dev server can't be walked outside the output directory. While
/// the most recent build failed, HTML requests get the build-failure
/// page instead of stale content; non-HTML assets keep serving the
/// previous build so unrelated tabs don't lose their styling.
async fn handle_static(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if rel.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = resolve_path(&state.out, rel);
    let is_html = content_type(&path).starts_with("text/html");
    if is_html {
        let failed = state
            .error
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(err) = failed {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                error_page(&err),
            )
                .into_response();
        }
    }
    match tokio::fs::read(&path).await {
        Ok(mut bytes) => {
            if is_html {
                // Appending after `</html>` is valid enough for a dev
                // server and avoids parsing the page.
                bytes.extend_from_slice(RELOAD_SCRIPT.as_bytes());
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type(&path))],
                bytes,
            )
                .into_response()
        }
        Err(e) if e.kind() == ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            format!(
                "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Not found</title></head>\
                 <body><h1>404</h1><p>Nothing at <code>/{rel}</code>.</p>{RELOAD_SCRIPT}</body></html>",
                rel = html_escape(rel)
            ),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("read {}: {e}", path.display()),
        )
            .into_response(),
    }
}

/// Self-contained build-failure page: inline styles only, no assets
/// from the (failed) output tree, plus the reload script so the page
/// replaces itself as soon as a build succeeds.
fn error_page(err: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Build failed</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2rem;background:#1c1c1c;color:#ddd}}\
         h1{{color:#f66}}pre{{background:#111;border-radius:6px;padding:1rem;overflow:auto;\
         white-space:pre-wrap;line-height:1.4}}</style></head>\
         <body><h1>Build failed</h1>\
         <p>The most recent rebuild failed; the page reloads itself once a build succeeds.</p>\
         <pre>{}</pre>{RELOAD_SCRIPT}</body></html>",
        html_escape(err)
    )
}

/// Minimal HTML escaping for text dropped into the error/404 pages.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Map a request-relative path to a file in the output tree: `/` and
/// directories resolve to their `index.html`, an extension-less name to
/// `<name>.html` (else a directory index), and an explicit file as-is.
fn resolve_path(out: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        return out.join("index.html");
    }
    let candidate = out.join(rel);
    if candidate.is_dir() {
        return candidate.join("index.html");
    }
    if candidate.extension().is_some() {
        return candidate;
    }
    let as_html = out.join(format!("{rel}.html"));
    if as_html.exists() {
        return as_html;
    }
    let dir_index = candidate.join("index.html");
    if dir_index.exists() {
        return dir_index;
    }
    as_html
}

/// Map an output file's extension to a content type.
pub(crate) fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

async fn log_requests(req: axum::http::Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    println!("{} {} {}", method, path, response.status().as_u16());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_escapes_markup() {
        assert_eq!(
            html_escape("a < b && c > d"),
            "a &lt; b &amp;&amp; c &gt; d"
        );
        assert_eq!(html_escape("plain"), "plain");
    }

    #[test]
    fn error_page_embeds_escaped_error_and_reload_script() {
        let page = error_page("expected `<value>`");
        assert!(page.contains("expected `&lt;value&gt;`"));
        assert!(!page.contains("expected `<value>`"));
        assert!(page.contains("__wdoc_reload"));
    }

    #[test]
    fn reload_script_targets_the_reload_route() {
        assert!(RELOAD_SCRIPT.contains("/__wdoc_reload"));
        assert!(RELOAD_SCRIPT.contains("location.reload()"));
    }
}
