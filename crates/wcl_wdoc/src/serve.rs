use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::watch;
use tower_http::services::ServeDir;

use crate::model::WdocDocument;

#[derive(Clone)]
struct ServeState {
    reload_rx: watch::Receiver<u64>,
}

/// Start a dev server with live reload.
///
/// `build_fn` is called to produce the document (re-called on file changes).
/// `watch_paths` are the source files/directories to watch.
pub async fn serve(
    build_fn: impl Fn() -> Result<WdocDocument, String> + Send + Sync + 'static,
    watch_paths: Vec<PathBuf>,
    output_dir: PathBuf,
    port: u16,
    open_browser: bool,
) -> Result<(), String> {
    let build_fn = Arc::new(build_fn);

    // Initial build
    let doc = build_fn().map_err(|e| format!("initial build failed: {e}"))?;
    crate::render::render_document(&doc, &output_dir)?;
    eprintln!("wdoc: built to {}", output_dir.display());

    // Reload signal
    let (reload_tx, reload_rx) = watch::channel(0u64);
    let state = ServeState { reload_rx };

    // File watcher
    let build_fn_watch = Arc::clone(&build_fn);
    let output_dir_watch = output_dir.clone();
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<()>(1);

    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if res.is_ok() {
                let _ = notify_tx.blocking_send(());
            }
        })
        .map_err(|e| format!("failed to create file watcher: {e}"))?;

    for path in &watch_paths {
        watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| format!("failed to watch {}: {e}", path.display()))?;
    }

    // Rebuild task
    let reload_tx = Arc::new(reload_tx);
    tokio::spawn(async move {
        let mut generation: u64 = 0;
        while notify_rx.recv().await.is_some() {
            // Debounce
            tokio::time::sleep(Duration::from_millis(200)).await;
            while notify_rx.try_recv().is_ok() {}

            eprintln!("wdoc: rebuilding...");
            match build_fn_watch() {
                Ok(doc) => {
                    if let Err(e) = crate::render::render_document(&doc, &output_dir_watch) {
                        eprintln!("wdoc: render error: {e}");
                        continue;
                    }
                    generation += 1;
                    let _ = reload_tx.send(generation);
                    eprintln!("wdoc: rebuilt successfully");
                }
                Err(e) => eprintln!("wdoc: build error: {e}"),
            }
        }
        // Keep watcher alive
        drop(watcher);
    });

    // HTTP server
    let app = Router::new()
        .route("/_wdoc/reload", get(sse_handler))
        .fallback_service(ServeDir::new(&output_dir).append_index_html_on_directories(true))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("failed to bind to {addr}: {e}"))?;

    eprintln!("wdoc: serving at http://{addr}");

    if open_browser {
        let _ = open_url(&format!("http://{addr}"));
    }

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {e}"))?;

    Ok(())
}

async fn sse_handler(
    State(state): State<ServeState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.reload_rx;
    let stream = async_stream::stream! {
        let mut rx = rx;
        while rx.changed().await.is_ok() {
            let gen = *rx.borrow();
            yield Ok(Event::default().data(format!("{gen}")));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
