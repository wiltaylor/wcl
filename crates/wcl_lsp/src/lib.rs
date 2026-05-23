//! WCL language server.
//!
//! Exposes a [`tower_lsp`]-based language server that wraps the
//! `wcl_lang` library. The CLI's `wcl lsp` subcommand drives this via
//! [`start_stdio`] (or [`start_tcp`] when launched against a debug
//! client); library consumers can also construct a [`Backend`]
//! directly for in-process testing.

mod completion;
mod convert;
mod diagnostics;
mod hover;
mod navigation;
mod resolve;
mod semtokens;
mod server;
mod symbols;
mod walk;

pub use server::Backend;

use std::net::SocketAddr;
use std::path::Path;

use tower_lsp::{LspService, Server};

/// Run the language server on stdio. Blocks until the client closes
/// the connection. Intended to be called from the `wcl lsp` CLI
/// handler inside a tokio runtime.
pub async fn start_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Listen on `addr` for inbound TCP connections and serve each one
/// as an independent LSP session. Intended for debug clients (e.g.
/// an editor's LSP inspector) where stdio isn't convenient. Each
/// accepted connection runs on its own tokio task with a fresh
/// [`Backend`], so connections don't share document state. Returns
/// only if the listener errors — kill the process to stop accepting.
pub async fn start_tcp(addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "wcl-lsp listening for TCP connections");
    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::info!(%peer, "wcl-lsp client connected");
        tokio::spawn(async move {
            let (read, write) = tokio::io::split(stream);
            let (service, socket) = LspService::new(Backend::new);
            Server::new(read, write, socket).serve(service).await;
            tracing::info!(%peer, "wcl-lsp client disconnected");
        });
    }
}

/// Initialise a `tracing` subscriber that writes plain-text log
/// lines to `path`. Designed for the `wcl lsp --log <path>` flag —
/// stderr-bound logging would corrupt the LSP stdio stream, so a
/// file sink is the safe default. Call this once, before
/// [`start_stdio`] / [`start_tcp`]; subsequent calls are no-ops.
pub fn install_file_logger(path: &Path) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let _ = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .try_init();
    Ok(())
}
