#[cfg(feature = "frontend")]
mod embedded {
    use axum::response::IntoResponse;
    use http::StatusCode;
    use rust_embed::Embed;

    #[derive(Embed)]
    #[folder = "../../frontend/oprc-next/out"]
    struct FrontendAssets;

    pub async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };

        match FrontendAssets::get(path) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                (
                    [(http::header::CONTENT_TYPE, mime.as_ref().to_string())],
                    content.data.to_vec(),
                )
                    .into_response()
            }
            // SPA fallback: serve index.html for client-side routes
            None => match FrontendAssets::get("index.html") {
                Some(content) => (
                    [(http::header::CONTENT_TYPE, "text/html".to_string())],
                    content.data.to_vec(),
                )
                    .into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            },
        }
    }
}

#[cfg(feature = "frontend")]
pub use embedded::serve_frontend;
