//! The `wcl wdoc serve` dev server: build once, serve the output over HTTP
//! with a live-reload script, and rebuild on request.
//!
//! A `notify` watcher notes which `.wcl` files changed but does not rebuild;
//! a rebuild runs when asked (Enter on the console, or `POST
//! /__wdoc_rebuild`), incrementally over the noted files when there are any.
//! Builds run on the blocking pool, so the server keeps answering requests
//! while one is in progress.
//!
//! Compiled only with the `serve` cargo feature, which brings in axum, tokio
//! and notify.

use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::{BuildOptions, RebuildOutcome, build_incremental, build_with_options};
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

/// How long the watch loop waits for the event stream to go quiet
/// before rebuilding — one editor save fires several notify events,
/// which should coalesce into a single build.
/// Write one line of the server's console log to stdout. See [`log_line`].
macro_rules! log_out {
    ($($arg:tt)*) => {
        log_line(std::io::stdout().lock(), format_args!($($arg)*))
    };
}

/// Write one line of the server's console log to stderr. See [`log_line`].
macro_rules! log_err {
    ($($arg:tt)*) => {
        log_line(std::io::stderr().lock(), format_args!($($arg)*))
    };
}

/// Write `line` and a newline to `stream`, dropping any write error. The
/// server outlives the terminal it was started from: a closed pipe
/// (`wcl wdoc serve | head`) or a vanished console must not stop it from
/// serving, and there is nowhere left to report the failure anyway.
fn log_line(mut stream: impl std::io::Write, line: std::fmt::Arguments<'_>) {
    let _ = writeln!(stream, "{line}");
}

/// The [`BuildOptions::progress`] hook the server builds with: each
/// progress line goes to stderr, but only when stderr is a terminal, so a
/// piped or captured log stays free of it.
fn log_progress(line: std::fmt::Arguments<'_>) {
    use std::io::IsTerminal as _;
    if std::io::stderr().is_terminal() {
        log_err!("{line}");
    }
}

/// The options every server build uses: [`log_progress`] as the hook.
fn build_options() -> BuildOptions {
    BuildOptions {
        progress: Some(log_progress),
        ..BuildOptions::default()
    }
}

const QUIET_WINDOW: Duration = Duration::from_millis(150);

/// How long a live-reload long-poll request parks before answering
/// with the unchanged generation. Short enough that intermediaries
/// don't kill the connection; the client just re-polls.
const POLL_TIMEOUT: Duration = Duration::from_secs(30);

/// The address `serve` binds to when neither `--addr` nor `auto` shifts
/// it elsewhere. `auto` scans upward from this port.
pub const DEFAULT_BIND: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 8080);

/// How the dev server chooses its bind address.
///
/// `--addr auto` picks the first free port near [`DEFAULT_BIND`]; any other
/// value is parsed as an explicit `SocketAddr` and bound as-is (hard error if
/// the port is busy).
#[derive(Debug, Clone, Copy)]
pub enum BindSpec {
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
async fn bind_auto(base: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
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
    /// The output directory, canonicalised once at startup. Every file the
    /// static handler serves must canonicalise to a path under it.
    out: PathBuf,
    /// Plain-text rendering of the most recent failed build; `None`
    /// when the last build succeeded. While set, HTML requests get a
    /// build-failure page instead of the stale previous build.
    error: RwLock<Option<String>>,
    /// Bumped after every build attempt — success *and* failure — so
    /// a browser parked on the error page reloads when the fix lands.
    generation: tokio::sync::watch::Sender<u64>,
    /// Source `.wcl` files changed since the last rebuild. The watcher
    /// accumulates here instead of rebuilding; a rebuild is triggered manually
    /// (Enter in the console, or `POST /__wdoc_rebuild`) and drains this.
    pending: Mutex<Vec<PathBuf>>,
    /// Send a [`RebuildReq`] to request a rebuild. The console (stdin Enter)
    /// and the `/__wdoc_rebuild` endpoint both use it; the rebuild worker runs
    /// one build per request and (when asked) reports completion back.
    rebuild_tx: tokio::sync::mpsc::UnboundedSender<RebuildReq>,
}

impl ServeState {
    /// Fresh state serving `out`, which must already exist (it is
    /// canonicalised here, before the first request).
    fn new(
        out: &Path,
        rebuild_tx: tokio::sync::mpsc::UnboundedSender<RebuildReq>,
    ) -> std::io::Result<Self> {
        Ok(ServeState {
            out: std::fs::canonicalize(out)?,
            error: RwLock::new(None),
            generation: tokio::sync::watch::Sender::new(0),
            pending: Mutex::new(Vec::new()),
            rebuild_tx,
        })
    }
}

/// A rebuild request handed to the rebuild worker.
struct RebuildReq {
    /// Resolved when the build finishes, so the HTTP handler can report the
    /// result. `None` for the console path.
    done: Option<tokio::sync::oneshot::Sender<RebuildReport>>,
}

/// What a rebuild did, returned to the `/__wdoc_rebuild` caller.
struct RebuildReport {
    ok: bool,
    /// Human summary (page count, or the first line of the error).
    summary: String,
}

/// Print the non-fatal warnings a build returned (edges whose endpoint
/// matched no shape id, …) to stderr.
fn print_render_warnings(warnings: &[String]) {
    for w in warnings {
        log_err!("warning: {w}");
    }
}

/// Run one build, report to stderr, record the outcome in `state`, and
/// bump the live-reload generation.
fn run_build(file: &Path, out: &Path, site: Option<&str>, state: &ServeState, rebuild: bool) {
    let opts = build_options();
    match build_with_options(file, out, site, &opts) {
        Ok(report) => {
            print_render_warnings(&report.warnings);
            let n = report.count;
            let plural = if n == 1 { "" } else { "s" };
            if rebuild {
                log_err!("rebuilt: {n} page{plural}");
            } else {
                log_err!("rendered {n} page{plural}");
            }
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
        }
        Err(err) => {
            log_err!(
                "{} failed:",
                if rebuild { "rebuild" } else { "initial build" }
            );
            log_err!("{}", err.render());
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(err.render_plain());
        }
    }
    state.generation.send_modify(|g| *g += 1);
}

/// Handle one [`RebuildReq`]: drain every pending change and rebuild the
/// served site. A drained set goes through [`build_incremental`], which scopes
/// the work to the pages those files declare; nothing pending means a full
/// build. Records the outcome in `state`, bumps the live-reload generation,
/// and returns a report.
fn run_rebuild_request(
    file: &Path,
    out: &Path,
    site: Option<&str>,
    state: &ServeState,
) -> RebuildReport {
    let opts = build_options();
    let changed = drain_pending(state);
    let result = if changed.is_empty() {
        build_with_options(file, out, site, &opts)
            .map(|r| (format!("{} (full)", page_count(r.count)), r.warnings))
    } else {
        build_incremental(file, out, site, &opts, &changed)
            .map(|r| (rebuild_summary(r.outcome), r.warnings))
    };
    finish_rebuild(state, result)
}

/// Take every pending changed path, sorted and deduplicated.
fn drain_pending(state: &ServeState) -> Vec<PathBuf> {
    let mut g = state.pending.lock().unwrap_or_else(|e| e.into_inner());
    let mut taken = std::mem::take(&mut *g);
    taken.sort();
    taken.dedup();
    taken
}

/// Record a build outcome in `state`, bump the live-reload generation, and
/// build the [`RebuildReport`]. `result` is `Ok((summary, warnings))` or the
/// build error.
fn finish_rebuild(
    state: &ServeState,
    result: Result<(String, Vec<String>), crate::BuildError>,
) -> RebuildReport {
    let report = match result {
        Ok((summary, warnings)) => {
            print_render_warnings(&warnings);
            log_err!("rebuilt: {summary}");
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
            RebuildReport { ok: true, summary }
        }
        Err(err) => {
            log_err!("rebuild failed:");
            log_err!("{}", err.render());
            let plain = err.render_plain();
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(plain.clone());
            RebuildReport {
                ok: false,
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

/// Assemble the dev server's routes over `state`.
///
/// One generic static handler resolves any request path against the output
/// tree, so it serves both the flat single-site layout and the nested
/// multi-site one (`/<site>/…`, `/<site>/_wdoc/…`) plus the generated chooser
/// at `/`, with no per-route knowledge. The reload and rebuild endpoints sit
/// outside the output tree's namespace.
///
/// A function rather than an expression inside [`serve`] so the tests can drive
/// the real router in process, without a listener or a `notify` watcher.
fn router(state: Arc<ServeState>) -> Router {
    Router::new()
        .route("/__wdoc_reload", get(handle_reload))
        .route("/__wdoc_rebuild", axum::routing::post(handle_rebuild))
        .fallback(get(handle_static))
        .with_state(state)
        .layer(middleware::from_fn(log_requests))
}

/// One rebuild of the served site, run synchronously on the blocking pool.
/// A value rather than a call so the tests can wrap it (to hold a build
/// open while they send requests).
type RebuildJob = Arc<dyn Fn() -> RebuildReport + Send + Sync>;

/// The [`RebuildJob`] the server runs: [`run_rebuild_request`] over `file`,
/// which prints the build's warnings and records its outcome in `state`.
fn rebuild_job(
    file: PathBuf,
    out: PathBuf,
    site: Option<String>,
    state: Arc<ServeState>,
) -> RebuildJob {
    Arc::new(move || run_rebuild_request(&file, &out, site.as_deref(), &state))
}

/// Rebuild worker: one build per request, until the request channel closes.
/// Every request rebuilds the served site, incrementally when files are
/// pending. Reports completion back when asked.
///
/// A build is synchronous and can take seconds, so it runs on the blocking
/// pool. Run inline, it would stall the task it shares with the listener in
/// [`serve`]'s `select!`, and no request would be answered until it finished.
async fn rebuild_worker(
    job: RebuildJob,
    state: Arc<ServeState>,
    mut requests: tokio::sync::mpsc::UnboundedReceiver<RebuildReq>,
) {
    while let Some(req) = requests.recv().await {
        let run = Arc::clone(&job);
        let report = match tokio::task::spawn_blocking(move || run()).await {
            Ok(report) => report,
            // The build panicked. Report it like a failed build so the
            // browser shows the failure page rather than stale content.
            Err(e) => {
                let summary = format!("rebuild panicked: {e}");
                log_err!("{summary}");
                *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(summary.clone());
                state.generation.send_modify(|g| *g += 1);
                RebuildReport { ok: false, summary }
            }
        };
        if let Some(done) = req.done {
            let _ = done.send(report);
        }
    }
}

/// Run the dev server for `file` until Ctrl-C, which exits the process.
///
/// Builds into `out` (a temp dir removed on Ctrl-C when `None`), binds
/// `addr`, and serves the output with the live-reload script injected into
/// every HTML page. With `site`, only that site is built and served at `/`.
/// A failed initial build is not fatal: HTML requests get the build-failure
/// page until a rebuild succeeds. Returns an error only when the server
/// cannot start (the output dir, the watcher, or the bind).
pub async fn serve(
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
    // observed even mid-build: a build is a synchronous, non-cancellable call
    // (the initial one below runs on this task, rebuilds on the blocking
    // pool), so a shutdown branch could not interrupt it anyway. `process::exit` tears down every thread at
    // once (the inotify watcher, axum connections, parked reload long-polls,
    // and any in-flight build) and, by ending the process, blocks all further
    // rebuilds. It skips the TempDir guard's `Drop`, so clean the temp output
    // dir by hand first.
    let temp_cleanup = _tempdir_guard.as_ref().map(|td| td.path().to_path_buf());
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log_err!("\nshutting down");
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
    let (rebuild_tx, rebuild_rx) = tokio::sync::mpsc::unbounded_channel::<RebuildReq>();

    let state = Arc::new(ServeState::new(&out_dir, rebuild_tx.clone())?);

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
                        if trigger.send(RebuildReq { done: None }).is_err() {
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
            log_err!(
                "{n} file change{} pending — press Enter to rebuild",
                if n == 1 { "" } else { "s" }
            );
        }
    };

    let rebuild_loop = rebuild_worker(
        rebuild_job(
            file.clone(),
            out_dir.clone(),
            site.clone(),
            Arc::clone(&state),
        ),
        Arc::clone(&state),
        rebuild_rx,
    );

    let app = router(Arc::clone(&state));

    let listener = match addr {
        BindSpec::Auto => bind_auto(DEFAULT_BIND).await?,
        BindSpec::Fixed(a) => tokio::net::TcpListener::bind(a).await?,
    };
    let bound = listener.local_addr()?;
    log_out!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );
    log_out!("auto-rebuild is off — press Enter here to rebuild after edits");

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

/// A path the watcher must react to: every `.wcl` file under the watch root
/// is document source, so the extension is the whole test.
fn is_source_wcl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "wcl")
}

/// The document `.wcl` paths an event touched (the granularity
/// `build_incremental` maps onto pages).
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

/// Request a rebuild over HTTP. Rebuilds the served site and **waits** for the
/// build to finish, so the caller can show a running/done indication; the
/// reload long-poll then reloads the page when the generation bumps. The
/// request body is ignored.
async fn handle_rebuild(State(state): State<Arc<ServeState>>) -> Response {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .rebuild_tx
        .send(RebuildReq { done: Some(tx) })
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild worker is gone");
    }
    match rx.await {
        Ok(report) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "ok": report.ok,
                "summary": report.summary,
            }),
        ),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild was cancelled"),
    }
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        value.to_string(),
    )
        .into_response()
}

fn json_error(status: StatusCode, msg: &str) -> Response {
    json_response(status, &serde_json::json!({ "error": msg }))
}

/// Resolve any request path to a file under the output tree and serve
/// it. Handles `/` and directory paths (→ `index.html`), extension-less
/// page names (→ `<name>.html`), and explicit files (`.html`, and the
/// `_wdoc/` assets at any depth). Nothing outside the output directory is
/// ever served: the request path must pass [`is_plain_request_path`], and the
/// file it resolves to must canonicalise to a path under the output
/// directory (which also stops a symlink inside it pointing out). Either
/// check failing is a bare 404. While the most recent build failed, HTML
/// requests get the build-failure page instead of stale content; non-HTML
/// assets keep serving the previous build so unrelated tabs don't lose their
/// styling.
async fn handle_static(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if !is_plain_request_path(rel) {
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
    let read = match confine(&state.out, &path).await {
        Ok(Some(real)) => tokio::fs::read(&real).await,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => Err(e),
    };
    match read {
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

/// Whether `rel` — a request path with its leading `/`s stripped, exactly as
/// the client sent it (no percent-decoding, so `%2f` is three literal
/// characters of a file name) — names only plain entries under the output
/// directory.
///
/// Every `/`-separated segment must be a single [`Component::Normal`], so
/// `..` and `.` are refused. A backslash or a colon is refused on every
/// platform: on Windows `\` is a separator, `C:` is a drive prefix and
/// `\\server\share` a UNC root, and joining any of them onto the output
/// directory would replace it rather than extend it. Refusing them
/// everywhere keeps the rule the same on the platform that can be tested
/// here and the one where it matters. Empty segments (`//`, a trailing `/`)
/// are allowed, as a path join collapses them.
fn is_plain_request_path(rel: &str) -> bool {
    rel.split('/').filter(|seg| !seg.is_empty()).all(|seg| {
        if seg.contains(['\\', ':']) {
            return false;
        }
        let mut parts = Path::new(seg).components();
        matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(_)), None)
        )
    })
}

/// Canonicalise `path` and keep it only if it lies under `out` (itself
/// canonical). `Ok(None)` is a path that escapes, through a symlink or
/// anything [`is_plain_request_path`] let past; an error is the
/// canonicalisation's own, `NotFound` for a missing file.
async fn confine(out: &Path, path: &Path) -> std::io::Result<Option<PathBuf>> {
    let real = tokio::fs::canonicalize(path).await?;
    Ok(real.starts_with(out).then_some(real))
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
fn content_type(path: &Path) -> &'static str {
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
    log_out!("{} {} {}", method, path, response.status().as_u16());
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

    // ── The rebuild round trip, end to end ───────────────────────────────
    //
    // The three tests above compare strings. What they cannot see is the
    // server's one moving part: a change lands in `pending`, a rebuild
    // request drains it, `build_incremental` re-renders just the pages those
    // files declare, and the next `GET` serves the rewritten page. Every step
    // of that chain fails *silently* when it breaks — a stale page in the dev
    // server, not a compile error — so it is driven here for real.
    //
    // In process, over the real `Router`, rather than by spawning the binary:
    // seeding `pending` by hand is what reaches the incremental branch at all,
    // and a spawned server only gets there when the `notify` watcher fires.

    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Everything [`serve`] wires together except the listener and the
    /// watcher: a small book on disk, its build output, the shared state, a
    /// running rebuild worker, and the router.
    struct Harness {
        src: TempDir,
        out: TempDir,
        state: Arc<ServeState>,
        app: Router,
    }

    impl Harness {
        /// Lay out a two-page book (`main.wcl` + a page file each), run the
        /// initial full build, and start the rebuild worker.
        fn start() -> Self {
            Self::start_with(|job| job)
        }

        /// [`Harness::start`], with the rebuild worker running `wrap(job)`
        /// in place of the real rebuild job.
        fn start_with(wrap: impl FnOnce(RebuildJob) -> RebuildJob) -> Self {
            let src = TempDir::new().expect("mkdir src");
            let out = TempDir::new().expect("mkdir out");
            let main = src.path().join("main.wcl");
            std::fs::write(
                &main,
                "import <wdoc.wcl>\n\
                 site docs {\n  \
                   default_template = :book\n  \
                   title = \"Docs\"\n  \
                   toc {\n    \
                     chapter \"A\" { page = \"a\" }\n    \
                     chapter \"B\" { page = \"b\" }\n  \
                   }\n\
                 }\n\
                 import \"./a.wcl\"\n\
                 import \"./b.wcl\"\n",
            )
            .expect("write main.wcl");
            write_page(src.path(), "a", "Original A.");
            write_page(src.path(), "b", "Original B.");

            let (rebuild_tx, rebuild_rx) = tokio::sync::mpsc::unbounded_channel::<RebuildReq>();
            let state = Arc::new(ServeState::new(out.path(), rebuild_tx).expect("state"));

            run_build(&main, out.path(), None, &state, false);
            assert!(
                state.error.read().unwrap().is_none(),
                "the initial build must succeed"
            );

            let job = rebuild_job(
                main.clone(),
                out.path().to_path_buf(),
                None,
                Arc::clone(&state),
            );
            tokio::spawn(rebuild_worker(wrap(job), Arc::clone(&state), rebuild_rx));

            let app = router(Arc::clone(&state));
            Harness {
                src,
                out,
                state,
                app,
            }
        }

        /// Drive one request through the real router, returning the status and
        /// the body as text.
        async fn send(&self, method: &str, path: &str) -> (StatusCode, String) {
            send(self.app.clone(), method, path).await
        }

        /// `GET path`, returning the status and the body as text.
        async fn get(&self, path: &str) -> (StatusCode, String) {
            self.send("GET", path).await
        }

        /// `POST /__wdoc_rebuild`, returning the decoded JSON report.
        async fn rebuild(&self) -> serde_json::Value {
            let (status, body) = self.send("POST", "/__wdoc_rebuild").await;
            assert_eq!(status, StatusCode::OK);
            serde_json::from_str(&body).expect("rebuild report is JSON")
        }

        /// Rewrite a page file and note it as pending, exactly as the watcher
        /// would have after an editor save.
        fn edit_page(&self, name: &str, body: &str) {
            let path = write_page(self.src.path(), name, body);
            self.state.pending.lock().unwrap().push(path);
        }
    }

    /// Drive one request through `app`, returning the status and the body as
    /// text. Owns its router, so a test can spawn it and let it wait.
    async fn send(app: Router, method: &str, path: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX)
            .await
            .expect("read body");
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Write `<name>.wcl` under `dir` as a one-paragraph page of the `docs`
    /// site. The page files don't re-import the wdoc schema — it resolves
    /// document-wide through `main.wcl`, exactly like the real docs.
    fn write_page(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(format!("{name}.wcl"));
        std::fs::write(
            &path,
            format!(
                "page {name} {{\n  sites = [:docs]\n  h1 \"Page {name}\"\n  p \"{body}\"\n}}\n"
            ),
        )
        .expect("write page file");
        path
    }

    #[tokio::test]
    async fn a_pending_change_rebuilds_just_that_page_and_the_next_get_serves_it() {
        let h = Harness::start();

        let (status, before) = h.get("/a").await;
        assert_eq!(status, StatusCode::OK);
        assert!(before.contains("Original A."), "{before}");
        // Every served page carries the live-reload script, appended after
        // the document — that is how the browser learns a rebuild happened.
        assert!(before.contains("/__wdoc_reload"), "{before}");
        let b = h.out.path().join("b.html");
        let b_written_before = std::fs::metadata(&b).expect("stat b.html").modified().ok();

        h.edit_page("a", "Edited A!");
        let report = h.rebuild().await;

        assert_eq!(report["ok"], serde_json::json!(true), "{report}");
        // The *targeted* branch: one named page, not "N pages (full)". A
        // mis-drained `pending` would fall back to a full build and this
        // summary is the only place that shows.
        assert_eq!(
            report["summary"],
            serde_json::json!("1 page (a)"),
            "{report}"
        );

        let (status, after) = h.get("/a").await;
        assert_eq!(status, StatusCode::OK);
        assert!(after.contains("Edited A!"), "{after}");
        assert!(
            !after.contains("Original A."),
            "stale page served:\n{after}"
        );
        // Targeted means targeted: B's file was never written again. Its bytes
        // would be identical either way (the build is deterministic), so the
        // write timestamp is what actually separates the two branches.
        let b_written_after = std::fs::metadata(&b).expect("stat b.html").modified().ok();
        assert_eq!(
            b_written_before, b_written_after,
            "page B must not be re-rendered"
        );
    }

    #[tokio::test]
    async fn a_rebuild_with_nothing_pending_rebuilds_the_whole_site() {
        let h = Harness::start();

        let report = h.rebuild().await;
        assert_eq!(report["ok"], serde_json::json!(true), "{report}");
        assert_eq!(
            report["summary"],
            serde_json::json!("2 pages (full)"),
            "an empty `pending` means a full build:\n{report}"
        );
        // The drain is a take, so a second request has nothing left either.
        assert!(h.state.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_rebuild_reports_and_serves_the_build_failure_page() {
        let h = Harness::start();

        // Break page A, and note it exactly as the watcher would.
        let a = h.src.path().join("a.wcl");
        std::fs::write(&a, "page a {\n  sites = [:docs]\n  p \n").expect("break a.wcl");
        h.state.pending.lock().unwrap().push(a);

        let report = h.rebuild().await;
        assert_eq!(report["ok"], serde_json::json!(false), "{report}");

        // While the last build is broken, every HTML request gets the failure
        // page rather than stale content.
        let (status, page) = h.get("/a").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(page.contains("Build failed"), "{page}");
        assert!(!page.contains("Original A."), "stale page served:\n{page}");
        // The reload long-poll answers with the bumped generation, so the
        // parked browser replaces the failure page when a build succeeds.
        let (status, generation) = h.get("/__wdoc_reload").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(generation.trim(), "2", "a failed build still bumps");

        // The fix lands, and the page comes back.
        h.edit_page("a", "Fixed A.");
        let report = h.rebuild().await;
        assert_eq!(report["ok"], serde_json::json!(true), "{report}");
        let (status, page) = h.get("/a").await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("Fixed A."), "{page}");
    }

    // ── Serving while a rebuild runs ─────────────────────────────────────

    #[tokio::test]
    async fn a_request_during_a_rebuild_is_still_served() {
        // The rebuild job is held open until the test releases it, so the
        // `GET` below provably lands mid-build. `#[tokio::test]` runs on one
        // thread: a build run inline on the worker task would block that
        // thread, and the `GET` would only be answered after the build.
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let gate = (Mutex::new(started_tx), Mutex::new(release_rx));
        let done = Arc::clone(&finished);
        let h = Harness::start_with(move |job| {
            Arc::new(move || {
                let _ = gate.0.lock().unwrap().send(());
                // Bounded, so a regression fails the assertions below
                // instead of hanging the test run.
                let _ = gate.1.lock().unwrap().recv_timeout(Duration::from_secs(10));
                let report = job();
                done.store(true, std::sync::atomic::Ordering::SeqCst);
                report
            })
        });

        let rebuild = tokio::spawn(send(h.app.clone(), "POST", "/__wdoc_rebuild"));
        // Wait (without blocking the runtime thread) for the build to start.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while started_rx.try_recv().is_err() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "rebuild never started"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let (status, page) = h.get("/a").await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("Original A."), "{page}");
        assert!(
            !finished.load(std::sync::atomic::Ordering::SeqCst),
            "the page was only served after the rebuild finished"
        );

        release_tx.send(()).expect("release the rebuild");
        let (status, body) = rebuild.await.expect("rebuild request task");
        assert_eq!(status, StatusCode::OK);
        let report: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
        assert_eq!(report["ok"], serde_json::json!(true), "{report}");
    }

    // ── Nothing outside the output directory is served ───────────────────

    #[test]
    fn plain_request_paths_are_accepted() {
        for rel in [
            "",
            "index.html",
            "a",
            "docs/a",
            "_wdoc/app.css",
            "docs/",
            // `//etc/passwd` after the handler strips leading slashes: a
            // relative path under the output dir, not the system file.
            "etc/passwd",
            // Not percent-decoded: three literal characters of a name.
            "..%2fsecret",
        ] {
            assert!(is_plain_request_path(rel), "{rel:?} should be accepted");
        }
    }

    #[test]
    fn escaping_request_paths_are_refused() {
        for rel in [
            "..",
            "../secret",
            "docs/../../secret",
            ".",
            "./a",
            // Windows drive paths, relative and absolute, and a stream name.
            "C:/Windows/win.ini",
            "C:\\Windows\\win.ini",
            "C:",
            "C:x",
            "a.html:stream",
            // A UNC root, and backslash traversal.
            "\\\\server\\share\\x",
            "\\etc\\passwd",
            "docs\\..\\..\\secret",
        ] {
            assert!(!is_plain_request_path(rel), "{rel:?} should be refused");
        }
    }

    /// On Windows each of these, joined onto the output dir the way
    /// [`resolve_path`] joins, *replaces* the output dir: the reason the
    /// rule exists. Only provable on Windows, where `Path` parses prefixes;
    /// CI's windows-latest leg runs it.
    #[cfg(windows)]
    #[test]
    fn windows_paths_would_replace_the_output_dir_if_let_through() {
        let out = Path::new(r"D:\site\out");
        for rel in [
            r"C:/Windows/win.ini",
            r"C:\Windows\win.ini",
            r"\\server\share\x",
        ] {
            assert!(!out.join(rel).starts_with(out), "{rel}");
            assert!(!is_plain_request_path(rel), "{rel}");
        }
    }

    /// An output dir `out/` with one servable file, next to a `secret.txt`
    /// that must never be served, behind a router with no rebuild worker.
    struct StaticHarness {
        root: TempDir,
        app: Router,
    }

    impl StaticHarness {
        fn start() -> Self {
            let root = TempDir::new().expect("mkdir root");
            let out = root.path().join("out");
            std::fs::create_dir(&out).expect("mkdir out");
            std::fs::write(out.join("inside.txt"), "inside").expect("write inside.txt");
            std::fs::write(root.path().join("secret.txt"), "secret").expect("write secret.txt");
            let (rebuild_tx, _) = tokio::sync::mpsc::unbounded_channel::<RebuildReq>();
            let state = Arc::new(ServeState::new(&out, rebuild_tx).expect("state"));
            StaticHarness {
                root,
                app: router(state),
            }
        }

        async fn get(&self, path: &str) -> (StatusCode, String) {
            send(self.app.clone(), "GET", path).await
        }
    }

    #[tokio::test]
    async fn requests_that_leave_the_output_dir_get_404() {
        let h = StaticHarness::start();
        let (status, body) = h.get("/inside.txt").await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, "inside"));

        for path in [
            "/../secret.txt",
            "/..%2fsecret.txt",
            "//secret.txt",
            "/C:/secret.txt",
            "/C:%5Csecret.txt",
            "/%5C%5Cserver%5Cshare",
        ] {
            let (status, body) = h.get(path).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
            assert!(!body.contains("\"secret\""), "{path} leaked: {body}");
            assert_ne!(body, "secret", "{path} leaked the file");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlink_out_of_the_output_dir_is_not_followed() {
        let h = StaticHarness::start();
        std::os::unix::fs::symlink(
            h.root.path().join("secret.txt"),
            h.root.path().join("out").join("link.txt"),
        )
        .expect("symlink");
        let (status, body) = h.get("/link.txt").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_ne!(body, "secret", "the symlink was followed");
    }
}
