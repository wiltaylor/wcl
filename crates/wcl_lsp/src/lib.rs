//! WCL language server.
//!
//! Exposes a [`tower_lsp`]-based language server that wraps the
//! `wcl_lang` library. The CLI's `wcl lsp` subcommand drives this via
//! [`start_stdio`]; library consumers can also construct a [`Backend`]
//! directly for in-process testing.
//!
//! First slice features:
//!   - publish diagnostics on open/change (parse errors + schema errors)
//!   - document formatting (full-document edit, backed by `format::to_source`)
//!   - document symbols (outline view, backed by `SymbolIndex`)

mod completion;
mod convert;
mod diagnostics;
mod hover;
mod navigation;
mod resolve;
mod semtokens;
mod server;
mod symbols;

pub use server::Backend;

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
