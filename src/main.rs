mod auth;
mod file_utils;
mod models;
mod routes;
mod zip_utils;

use crate::models::AppState;
use axum::Router;
use axum::routing::get;
use axum::routing::post;
use dotenvy::dotenv;
use std::env;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let state = AppState {
        share_dir: env::var("SHARE_DIR").expect("SHARE_DIR not set").into(),
        share_key: env::var("SHARE_KEY").expect("SHARE_KEY not set"),
        greet: env::var("GREET").expect("GREET not set"),
    };

    let login_router = Router::new()
        .route("/login", get(routes::show_login_form))
        .route("/login", post(routes::process_login));

    let downloads_router = Router::new()
        .route("/download-zip", get(routes::download_zip))
        .route("/download/{filename}", get(routes::download_file));

    let app = Router::new()
        .route("/", get(routes::index))
        .merge(login_router)
        .merge(downloads_router)
        .nest_service("/static", ServeDir::new("static"))
        .fallback(routes::handle_404)
        .with_state(state)
        .layer(CookieManagerLayer::new());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
