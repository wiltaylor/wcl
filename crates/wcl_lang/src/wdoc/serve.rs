use std::collections::HashSet;
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

use crate::wdoc::model::WdocDocument;

/// Result of a wdoc serve build, including the document and any extra source
/// paths discovered during parsing that should be watched for rebuilds.
pub struct ServeBuild {
    pub document: WdocDocument,
    pub watch_paths: Vec<PathBuf>,
}

#[derive(Clone)]
struct ServeState {
    reload_rx: watch::Receiver<u64>,
}

/// Start a dev server with live reload.
///
/// `build_fn` is called to produce the document (re-called on file changes).
/// `watch_paths` are the root source files/directories to watch. Additional
/// paths may be returned from each build.
pub async fn serve(
    build_fn: impl Fn() -> Result<ServeBuild, String> + Send + Sync + 'static,
    watch_paths: Vec<PathBuf>,
    asset_dirs: Vec<PathBuf>,
    output_dir: PathBuf,
    port: u16,
    open_browser: bool,
) -> Result<(), String> {
    let build_fn = Arc::new(build_fn);

    // Initial build
    let initial = build_fn().map_err(|e| format!("initial build failed: {e}"))?;
    let initial_asset_dirs = combined_asset_dirs(&asset_dirs, &initial.watch_paths);
    let asset_dir_refs: Vec<&std::path::Path> =
        initial_asset_dirs.iter().map(|p| p.as_path()).collect();
    crate::wdoc::render::render_document(&initial.document, &output_dir, &asset_dir_refs)?;
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
            if let Ok(event) = res {
                // Only trigger on data writes, not access/metadata
                use notify::event::{CreateKind, ModifyKind, RemoveKind};
                use notify::EventKind;
                match event.kind {
                    EventKind::Create(CreateKind::File)
                    | EventKind::Modify(ModifyKind::Data(_))
                    | EventKind::Remove(RemoveKind::File) => {
                        let _ = notify_tx.blocking_send(());
                    }
                    _ => {}
                }
            }
        })
        .map_err(|e| format!("failed to create file watcher: {e}"))?;

    let mut watched_paths = HashSet::new();
    let desired_paths = combined_watch_paths(&watch_paths, &initial.watch_paths);
    sync_watch_paths(&mut watcher, &mut watched_paths, &desired_paths)?;

    // Rebuild task
    let reload_tx = Arc::new(reload_tx);
    tokio::spawn(async move {
        let mut generation: u64 = 0;
        while notify_rx.recv().await.is_some() {
            wait_for_quiet(&mut notify_rx, Duration::from_secs(1)).await;

            eprintln!("wdoc: rebuilding...");
            match build_fn_watch() {
                Ok(build) => {
                    let desired_paths = combined_watch_paths(&watch_paths, &build.watch_paths);
                    if let Err(e) =
                        sync_watch_paths(&mut watcher, &mut watched_paths, &desired_paths)
                    {
                        eprintln!("wdoc: watch error: {e}");
                    }

                    let render_asset_dirs = combined_asset_dirs(&asset_dirs, &build.watch_paths);
                    let arefs: Vec<&std::path::Path> =
                        render_asset_dirs.iter().map(|p| p.as_path()).collect();
                    if let Err(e) = crate::wdoc::render::render_document(
                        &build.document,
                        &output_dir_watch,
                        &arefs,
                    ) {
                        eprintln!("wdoc: render error: {e}");
                        cooldown_after_build(&mut notify_rx).await;
                        continue;
                    }
                    generation += 1;
                    let _ = reload_tx.send(generation);
                    eprintln!("wdoc: rebuilt successfully");
                }
                Err(e) => eprintln!("wdoc: build error: {e}"),
            }
            cooldown_after_build(&mut notify_rx).await;
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

fn combined_watch_paths(root_paths: &[PathBuf], build_paths: &[PathBuf]) -> HashSet<PathBuf> {
    root_paths
        .iter()
        .chain(build_paths.iter())
        .filter(|p| !p.as_os_str().is_empty())
        .cloned()
        .collect()
}

fn combined_asset_dirs(root_dirs: &[PathBuf], build_paths: &[PathBuf]) -> Vec<PathBuf> {
    root_dirs
        .iter()
        .cloned()
        .chain(
            build_paths
                .iter()
                .filter_map(|path| path.parent().map(PathBuf::from)),
        )
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn sync_watch_paths(
    watcher: &mut RecommendedWatcher,
    watched_paths: &mut HashSet<PathBuf>,
    desired_paths: &HashSet<PathBuf>,
) -> Result<(), String> {
    let stale_paths: Vec<PathBuf> = watched_paths.difference(desired_paths).cloned().collect();
    for path in stale_paths {
        watcher
            .unwatch(&path)
            .map_err(|e| format!("failed to unwatch {}: {e}", path.display()))?;
        watched_paths.remove(&path);
    }

    let new_paths: Vec<PathBuf> = desired_paths.difference(watched_paths).cloned().collect();
    for path in new_paths {
        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("failed to watch {}: {e}", path.display()))?;
        watched_paths.insert(path);
    }

    Ok(())
}

async fn wait_for_quiet(notify_rx: &mut tokio::sync::mpsc::Receiver<()>, quiet: Duration) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(quiet) => break,
            event = notify_rx.recv() => {
                if event.is_none() {
                    break;
                }
            }
        }
    }
    drain_events(notify_rx);
}

async fn cooldown_after_build(notify_rx: &mut tokio::sync::mpsc::Receiver<()>) {
    tokio::time::sleep(Duration::from_millis(200)).await;
    drain_events(notify_rx);
}

fn drain_events(notify_rx: &mut tokio::sync::mpsc::Receiver<()>) {
    while notify_rx.try_recv().is_ok() {}
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
