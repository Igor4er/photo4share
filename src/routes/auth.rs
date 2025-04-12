use crate::auth::verify_cookie_key;
use crate::file_utils::error_response;
use crate::models::AppState;
use crate::models::LoginForm;
use crate::models::LoginTemplate;
use askama::Template;
use axum::extract::Form;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use rand::Rng;
use rand::rng;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tower_cookies::Cookie as TowerCookie;
use tower_cookies::Cookies;

pub async fn show_login_form(State(state): State<AppState>, cookies: Cookies) -> impl IntoResponse {
    if verify_cookie_key(&cookies, &state.share_key) {
        return Redirect::to("/").into_response();
    }

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
    if verify_cookie_key(&cookies, &state.share_key) {
        return Redirect::to("/").into_response();
    }

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
