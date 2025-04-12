use crate::auth::verify_cookie_key;
use crate::file_utils::error_response;
use crate::file_utils::should_include_file;
use crate::file_utils::validate_path;
use crate::models::AppState;
use crate::zip_utils::calculate_directory_hash;
use crate::zip_utils::serve_zip_file;
use async_zip::Compression;
use async_zip::ZipEntryBuilder;
use async_zip::base::write::ZipFileWriter;
use axum::body::Body;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use std::io::Cursor;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use tower_cookies::Cookies;

pub async fn download_file(
    State(state): State<AppState>,
    cookies: Cookies,
    AxumPath(filename): AxumPath<String>,
) -> Response {
    info!("File download requested: {}", filename);
    if !verify_cookie_key(&cookies, &state.share_key) {
        return Redirect::to("/login").into_response();
    }

    let filepath = match validate_path(&state.share_dir, &filename).await {
        Ok(Some(path)) => path,
        Ok(None) => return error_response(StatusCode::BAD_REQUEST, "Invalid file requested"),
        Err(_) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "File system error");
        }
    };

    match File::open(&filepath).await {
        Ok(file) => {
            let stream = ReaderStream::new(file);
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/octet-stream")
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
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file"),
    }
}

pub async fn download_zip(State(state): State<AppState>, cookies: Cookies) -> Response {
    if !verify_cookie_key(&cookies, &state.share_key) {
        return Redirect::to("/login").into_response();
    }

    // Setup zip cache directory
    let zip_dir = state.share_dir.join(".zipcache");
    let _ = fs::create_dir_all(&zip_dir).await;

    // Get directory hash for cache filename
    let hash = match calculate_directory_hash(&state.share_dir).await {
        Ok(h) => h,
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Hashing failed"),
    };

    let cached_zip = zip_dir.join(format!("{}.zip", hash));

    // Return cached zip if it exists
    if cached_zip.exists() {
        if let Ok(file) = File::open(&cached_zip).await {
            return serve_zip_file(file);
        }
    }

    // Create a new zip file
    let temp_path = zip_dir.join(format!("{}.tmp", hash));

    // Get and sort the files
    let mut entries = match fs::read_dir(&state.share_dir).await {
        Ok(e) => e,
        Err(_) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to read dir");
        }
    };

    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if let Ok(true) = should_include_file(&state.share_dir, &path).await {
            files.push(path);
        }
    }

    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    // Create an in-memory buffer for the ZIP
    let mut buffer = Vec::new();
    let cursor = Cursor::new(&mut buffer);
    let mut zip = ZipFileWriter::with_tokio(cursor);

    // Add files to the zip
    for path in files {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        let mut file = match File::open(&path).await {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mut contents = Vec::new();
        if file.read_to_end(&mut contents).await.is_err() {
            continue;
        }

        // Create entry for the file
        let entry = ZipEntryBuilder::new(filename.into(), Compression::Stored);

        if let Err(_) = zip.write_entry_whole(entry, &contents).await {
            continue;
        }
    }

    // Finalize the zip
    if let Err(_) = zip.close().await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create ZIP");
    }

    // Write the buffer to a file
    if let Err(_) = fs::write(&temp_path, buffer).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to write ZIP");
    }

    // Rename the temporary file to the final cached ZIP
    if let Err(_) = fs::rename(&temp_path, &cached_zip).await {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save ZIP cache",
        );
    }

    // Serve the ZIP file
    match File::open(&cached_zip).await {
        Ok(file) => serve_zip_file(file),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "ZIP read error"),
    }
}
