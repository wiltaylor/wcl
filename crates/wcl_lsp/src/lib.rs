pub mod analysis;
pub mod ast_utils;
pub mod completion;
pub mod convert;
pub mod definition;
pub mod diagnostics;
pub mod formatting;
pub mod hover;
pub mod references;
pub mod rename;
pub mod semantic_tokens;
pub mod server;
pub mod signature_help;
pub mod state;
pub mod symbols;

use async_lsp::client_monitor::ClientProcessMonitorLayer;
use async_lsp::concurrency::ConcurrencyLayer;
use async_lsp::panic::CatchUnwindLayer;
use async_lsp::server::LifecycleLayer;
use async_lsp::tracing::TracingLayer;
use tower::ServiceBuilder;

/// Start the LSP server over stdio.
pub async fn start_stdio() {
    start_stdio_with_options(wcl_lang::ParseOptions::default()).await
}

/// Start the LSP server over stdio with custom parse options.
pub async fn start_stdio_with_options(options: wcl_lang::ParseOptions) {
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let options = options.clone();
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(server::WclLanguageServer::new_router_with_options(
                client, options,
            ))
    });

    #[cfg(unix)]
    let (stdin, stdout) = (
        async_lsp::stdio::PipeStdin::lock_tokio().unwrap(),
        async_lsp::stdio::PipeStdout::lock_tokio().unwrap(),
    );
    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    server.run_buffered(stdin, stdout).await.unwrap();
}

/// Start the LSP server over TCP at the given address.
pub async fn start_tcp(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    start_tcp_with_options(addr, wcl_lang::ParseOptions::default()).await
}

/// Start the LSP server over TCP with custom parse options.
pub async fn start_tcp_with_options(
    addr: &str,
    options: wcl_lang::ParseOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("WCL LSP listening on {}", addr);

    let (stream, _) = listener.accept().await?;
    let (read, write) = tokio::io::split(stream);

    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        let options = options.clone();
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(server::WclLanguageServer::new_router_with_options(
                client, options,
            ))
    });

    let read = tokio_util::compat::TokioAsyncReadCompatExt::compat(read);
    let write = tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(write);

    server.run_buffered(read, write).await?;
    Ok(())
}
