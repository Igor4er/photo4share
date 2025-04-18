use crate::file_utils::error_response;
use crate::file_utils::should_include_file;
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use std::path::Path;
use tokio::fs;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub async fn calculate_directory_hash(dir: &Path) -> std::io::Result<String> {
    let mut entries = fs::read_dir(dir).await?;
    let mut files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Ok(true) = should_include_file(dir, &path).await {
            files.push(path);
        }
    }

    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut hasher = blake3::Hasher::new();
    for path in files {
        let file_name = path.file_name().unwrap();
        let meta = fs::metadata(&path).await?;
        let mtime = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        hasher.update(file_name.to_string_lossy().as_bytes());
        hasher.update(&mtime.to_le_bytes());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn serve_zip_file(file: File) -> Response {
    let stream = ReaderStream::new(file);

    let today = chrono::Local::now();
    let formatted_date = today.format("%d.%m.%y").to_string();
    let filename = format!("{}_files.zip", formatted_date);

    match Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/zip")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from_stream(stream))
    {
        Ok(response) => response,
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Response error"),
    }
}
