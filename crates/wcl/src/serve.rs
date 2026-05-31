use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tempfile::TempDir;
use wcl_wdoc::build;

pub(crate) async fn serve(
    file: PathBuf,
    out: Option<PathBuf>,
    addr: SocketAddr,
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

    // Initial build. Failure is non-fatal — the watcher will retry,
    // and requests will 404 in the meantime.
    match build(&file, &out_dir, site.as_deref()) {
        Ok(n) => eprintln!("rendered {n} page{}", if n == 1 { "" } else { "s" }),
        Err(err) => {
            eprintln!("initial build failed:");
            err.report();
        }
    }

    let watch_root = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    let bg_file = file.clone();
    let bg_out = out_dir.clone();
    let bg_site = site.clone();
    // The rebuild loop is a *local* future (not a detached `tokio::spawn`)
    // owned by the `select!` below, so when the server shuts down it is
    // dropped here — which drops the `notify` watcher and stops its inotify
    // thread. A detached task would outlive `serve` and hang process exit.
    let watch_loop = async move {
        let _watcher = watcher; // keep the watcher alive for this future's lifetime
        while let Some(event) = rx.recv().await {
            if !is_relevant(&event) {
                continue;
            }
            match build(&bg_file, &bg_out, bg_site.as_deref()) {
                Ok(n) => eprintln!("rebuilt: {n} page{}", if n == 1 { "" } else { "s" }),
                Err(err) => {
                    eprintln!("rebuild failed:");
                    err.report();
                }
            }
        }
    };

    // One generic static handler resolves any request path against the
    // output tree, so it serves both the flat single-site layout and the
    // nested multi-site one (`/<site>/…`, `/<site>/_wdoc/…`) plus the
    // generated chooser at `/`, with no per-route knowledge.
    let shared_out: Arc<PathBuf> = Arc::new(out_dir.clone());
    let app = Router::new()
        .fallback(get(handle_static))
        .with_state(shared_out)
        .layer(middleware::from_fn(log_requests));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );

    // Ctrl-C drives axum's graceful shutdown: it stops accepting and lets
    // its connection tasks finish, then the serve future resolves and the
    // `select!` returns — dropping `watch_loop` (and the watcher with it).
    // Nothing detached survives, so the process can exit promptly.
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down");
    };
    tokio::select! {
        res = axum::serve(listener, app).with_graceful_shutdown(shutdown).into_future() => res?,
        // Only ends if the watch channel closes; otherwise it runs until the
        // server branch wins and this future is dropped.
        _ = watch_loop => {}
    }
    Ok(())
}

fn is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event
        .paths
        .iter()
        .any(|p| p.extension().is_some_and(|e| e == "wcl"))
}

/// Resolve any request path to a file under the output tree and serve
/// it. Handles `/` and directory paths (→ `index.html`), extension-less
/// page names (→ `<name>.html`), and explicit files (`.html`, and the
/// `_wdoc/` assets at any depth). Rejects `..` / backslash components so
/// the dev server can't be walked outside the output directory.
async fn handle_static(State(out): State<Arc<PathBuf>>, uri: axum::http::Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if rel.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = resolve_path(&out, rel);
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type(&path))],
            bytes,
        )
            .into_response(),
        Err(e) if e.kind() == ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            format!(
                "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Not found</title></head>\
                 <body><h1>404</h1><p>Nothing at <code>/{rel}</code>.</p></body></html>"
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
    println!("{} {} {}", method, path, response.status().as_u16());
    response
}
