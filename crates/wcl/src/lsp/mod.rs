pub mod analysis;
pub mod ast_utils;
pub mod completion;
pub mod convert;
pub mod definition;
pub mod diagnostics;
pub mod fmt_impl;
pub mod formatting;
pub mod hover;
pub mod references;
pub mod rename;
pub mod semantic_tokens;
pub mod server;
pub mod signature_help;
pub mod state;
pub mod symbols;

use tower_lsp::{LspService, Server};

/// Start the LSP server over stdio.
pub async fn start_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(server::WclLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Start the LSP server over TCP at the given address.
pub async fn start_tcp(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("WCL LSP listening on {}", addr);

    let (stream, _) = listener.accept().await?;
    let (read, write) = tokio::io::split(stream);

    let (service, socket) = LspService::new(server::WclLanguageServer::new);
    Server::new(read, write, socket).serve(service).await;
    Ok(())
}
