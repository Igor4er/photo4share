use crate::models::ErrorTemplate;
use askama::Template;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use path_clean::PathClean;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::io;

pub async fn validate_path(base_dir: &Path, filename: &str) -> io::Result<Option<PathBuf>> {
    // Basic filename safety check
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Ok(None);
    }

    // Clean the path to handle any path traversal attempts
    let filepath = base_dir.join(filename).clean();

    // Verify canonical path is within share directory to prevent path traversal
    let canonical_path = match fs::canonicalize(&filepath).await {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let canonical_base = match fs::canonicalize(base_dir).await {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    if !canonical_path.starts_with(&canonical_base) {
        return Ok(None);
    }

    // Check if it's a regular file and not a symlink
    let metadata = match fs::symlink_metadata(&filepath).await {
        Ok(m) => m,
        Err(e) => return Err(e),
    };

    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(None);
    }

    Ok(Some(filepath))
}

pub async fn should_include_file(base_dir: &Path, path: &Path) -> io::Result<bool> {
    // Get filename, skip hidden files
    let filename = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) if !name.starts_with(".") => name,
        _ => return Ok(false),
    };

    // Use the consolidated validation logic
    match validate_path(base_dir, filename).await? {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

pub fn error_response(status: StatusCode, message: &str) -> Response {
    let template = ErrorTemplate {
        error_code: status.as_u16().to_string(),
        error_message: message.to_string(),
    };

    match template.render() {
        Ok(html) => Response::builder()
            .status(status)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(axum::body::Body::from(html))
            .unwrap_or_else(|_| status.into_response()),
        Err(_) => {
            // Fallback if template rendering fails
            status.into_response()
        }
    }
}
