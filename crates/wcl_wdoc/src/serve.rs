use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tempfile::TempDir;

use crate::build::build;

pub async fn serve(
    file: PathBuf,
    out: Option<PathBuf>,
    addr: SocketAddr,
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
    match build(&file, &out_dir) {
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
    tokio::spawn(async move {
        // Keep the watcher alive for the lifetime of this task; dropping
        // it would silently stop notifications.
        let _watcher = watcher;
        while let Some(event) = rx.recv().await {
            if !is_relevant(&event) {
                continue;
            }
            match build(&bg_file, &bg_out) {
                Ok(n) => eprintln!("rebuilt: {n} page{}", if n == 1 { "" } else { "s" }),
                Err(err) => {
                    eprintln!("rebuild failed:");
                    err.report();
                }
            }
        }
    });

    let shared_out: Arc<PathBuf> = Arc::new(out_dir.clone());
    let app = Router::new()
        .route("/", get(handle_index))
        // Bundled terminal assets (fonts + replay player) live under
        // `_wdoc/`; serve them as static files so `@font-face` and the
        // player `<script src>` resolve.
        .route(
            &format!("/{}/{{file}}", crate::terminal::ASSET_DIR),
            get(handle_asset),
        )
        .route("/{name}", get(handle_named))
        .with_state(shared_out)
        .layer(middleware::from_fn(log_requests));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );

    // Race the server against Ctrl-C so the TempDir guard at function
    // scope gets dropped on SIGINT. SIGTERM still kills the process
    // before drops can run.
    tokio::select! {
        res = axum::serve(listener, app).into_future() => res?,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nshutting down");
        }
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

async fn handle_index(State(out): State<Arc<PathBuf>>) -> Response {
    serve_page(&out, "index").await
}

async fn handle_named(
    State(out): State<Arc<PathBuf>>,
    AxumPath(name): AxumPath<String>,
) -> Response {
    let trimmed = name.strip_suffix(".html").unwrap_or(&name);
    serve_page(&out, trimmed).await
}

/// Serve a bundled static asset out of `<out>/_wdoc/`. Single path
/// segment only — anything with a separator or `..` is rejected so the
/// dev server can't be walked outside the asset directory.
async fn handle_asset(
    State(out): State<Arc<PathBuf>>,
    AxumPath(file): AxumPath<String>,
) -> Response {
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = out.join(crate::terminal::ASSET_DIR).join(&file);
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, asset_content_type(&file))],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Map a bundled asset's extension to a content type.
fn asset_content_type(file: &str) -> &'static str {
    match file.rsplit('.').next() {
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

async fn serve_page(out_dir: &Path, name: &str) -> Response {
    let path = out_dir.join(format!("{name}.html"));
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
        Err(e) => match e.kind() {
            ErrorKind::NotFound => (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                format!(
                    "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Not found</title></head>\
                     <body><h1>404</h1><p>No page named <code>{name}</code>.</p></body></html>"
                ),
            )
                .into_response(),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!("read {}: {e}", path.display()),
            )
                .into_response(),
        },
    }
}

async fn log_requests(req: axum::http::Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    println!("{} {} {}", method, path, response.status().as_u16());
    response
}
