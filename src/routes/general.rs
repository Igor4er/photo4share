use crate::auth::verify_cookie_key;
use crate::file_utils::error_response;
use crate::file_utils::should_include_file;
use crate::models::AppState;
use crate::models::ErrorTemplate;
use crate::models::ListTemplate;
use askama::Template;
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Response;
use mime_guess;
use rust_embed::RustEmbed;
use tokio::fs;
use tower_cookies::Cookies;

pub async fn index(State(state): State<AppState>, cookies: Cookies) -> Response {
    if !verify_cookie_key(&cookies, &state.share_key) {
        return axum::response::Redirect::to("/login").into_response();
    }

    let mut entries = match fs::read_dir(&state.share_dir).await {
        Ok(o) => o,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Can't read directory: {}", e.to_string()).as_str(),
            );
        }
    };

    let mut files = vec![];
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if let Ok(true) = should_include_file(&state.share_dir, &path).await {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                files.push(name.to_string());
            }
        }
    }

    let template = ListTemplate {
        files,
        greet: state.greet,
    };
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Template error"),
    }
}

pub async fn handle_404() -> impl IntoResponse {
    let template = ErrorTemplate {
        error_code: "404".to_string(),
        error_message: "Page not found".to_string(),
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticAssets;

pub async fn static_handler(Path(path): Path<String>) -> Response {
    match StaticAssets::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();

            match Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(axum::body::Body::from(content.data))
            {
                Ok(response) => response,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to build response",
                ),
            }
        }
        None => error_response(
            StatusCode::NOT_FOUND,
            &format!("Static file not found: {}", path),
        ),
    }
}
