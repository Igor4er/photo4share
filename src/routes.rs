use crate::{
    auth::verify_cookie_key,
    file_utils::{error_response, should_include_file, validate_path},
    models::{AppState, ListTemplate, LoginForm, LoginTemplate},
    zip_utils::{calculate_directory_hash, serve_zip_file},
};
use askama::Template;
use async_zip::{Compression, ZipEntryBuilder, base::write::ZipFileWriter};
use axum::{
    body::Body,
    extract::{Form, Path as AxumPath, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use rand::{Rng, rng};
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use tower_cookies::{Cookie as TowerCookie, Cookies};

pub async fn show_login_form(cookies: Cookies) -> impl IntoResponse {
    let token = generate_csrf_token();
    let mut csrf_cookie = TowerCookie::new("csrf_token", token.clone());
    csrf_cookie.set_http_only(true);
    csrf_cookie.set_secure(true);
    csrf_cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
    cookies.add(csrf_cookie);

    let template = LoginTemplate {
        error: "".to_string(),
        csrf_token: token,
    };
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Template error"),
    }
}

fn generate_csrf_token() -> String {
    let mut rng = rng();
    let random_bytes: [u8; 32] = rng.random();

    // Add timestamp to prevent token reuse
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut combined = Vec::with_capacity(40);
    combined.extend_from_slice(&random_bytes);
    combined.extend_from_slice(&timestamp.to_be_bytes());

    URL_SAFE.encode(combined)
}

pub async fn process_login(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<LoginForm>,
) -> Response {
    // Verify CSRF token
    let stored_token = cookies.get("csrf_token").map(|c| c.value().to_string());
    match stored_token {
        Some(token) if token == form.csrf_token => {
            // CSRF token is valid, proceed with login
            if crate::auth::verify_user_sent_key(&form.key, &state.share_key) {
                // Clear CSRF token after successful verification
                cookies.remove(TowerCookie::new("csrf_token", ""));

                let mut cookie = TowerCookie::new("share_key", form.key);
                cookie.set_http_only(true);
                cookie.set_secure(true);
                cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
                cookies.add(cookie);

                Redirect::to("/").into_response()
            } else {
                let new_token = generate_csrf_token();
                let mut csrf_cookie = TowerCookie::new("csrf_token", new_token.clone());
                csrf_cookie.set_http_only(true);
                csrf_cookie.set_secure(true);
                csrf_cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
                cookies.add(csrf_cookie);

                let template = LoginTemplate {
                    error: "Хибний ключ доступу. Впевніться що скопіювали його повністю без жодних додаткових символів та пробілів".to_string(),
                    csrf_token: new_token,
                };
                match template.render() {
                    Ok(html) => Html(html).into_response(),
                    Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Template error"),
                }
            }
        }
        _ => {
            // CSRF token is invalid
            let new_token = generate_csrf_token();
            let mut csrf_cookie = TowerCookie::new("csrf_token", new_token.clone());
            csrf_cookie.set_http_only(true);
            csrf_cookie.set_secure(true);
            csrf_cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
            cookies.add(csrf_cookie);

            let template = LoginTemplate {
                error: "Помилка безпеки: недійсний маркер CSRF. Спробуйте знову.".to_string(),
                csrf_token: new_token,
            };
            match template.render() {
                Ok(html) => Html(html).into_response(),
                Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Template error"),
            }
        }
    }
}

pub async fn download_file(
    State(state): State<AppState>,
    cookies: Cookies,
    AxumPath(filename): AxumPath<String>,
) -> Response {
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

pub async fn index(State(state): State<AppState>, cookies: Cookies) -> Response {
    if !verify_cookie_key(&cookies, &state.share_key) {
        return axum::response::Redirect::to("/login").into_response();
    }

    let mut entries = match fs::read_dir(&state.share_dir).await {
        Ok(e) => e,
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Can't read directory"),
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

    let template = ListTemplate { files };
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "Template error"),
    }
}
