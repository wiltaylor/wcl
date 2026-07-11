//! The embedded editor SPA (`editor-ui/dist`, built by `build.rs`).
//!
//! Release builds embed the vite output into the binary; debug builds read
//! the folder from disk at runtime, so frontend iteration doesn't rebuild
//! `wcl`. The fallback handler mirrors a standard SPA server: exact asset
//! match, `index.html` for extension-less routes, immutable caching for
//! vite's content-hashed `/assets/*` files.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../editor-ui/dist"]
struct Assets;

/// Router fallback: serve the SPA. Anything that isn't a known asset and
/// has no file extension gets `index.html` (client-side routes).
pub(crate) async fn spa_fallback(uri: Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if rel.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = if rel.is_empty() { "index.html" } else { rel };
    if let Some(file) = Assets::get(path) {
        return asset_response(path, file);
    }
    if !path.contains('.')
        && let Some(index) = Assets::get("index.html")
    {
        return asset_response("index.html", index);
    }
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("nothing at /{rel}"),
    )
        .into_response()
}

fn asset_response(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    // Vite's hashed output under assets/ never changes content for a given
    // name; everything else (index.html) must revalidate.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                crate::serve::content_type(std::path::Path::new(path)),
            ),
            (header::CACHE_CONTROL, cache),
        ],
        file.data.into_owned(),
    )
        .into_response()
}
