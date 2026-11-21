//! Serves the React build (web/dist) embedded into the binary.

use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref().to_string())],
            file.data,
        )
            .into_response();
    }
    // SPA fallback
    match Assets::get("index.html") {
        Some(file) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string())],
            file.data,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "Web-UI nicht gebaut. Bitte zuerst `npm run build` im Ordner web/ ausführen.",
        )
            .into_response(),
    }
}
