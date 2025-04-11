mod auth;
mod file_utils;
mod models;
mod routes;
mod zip_utils;

use crate::models::AppState;
use axum::{
    Router,
    routing::{get, post},
};
use dotenvy::dotenv;
use std::env;
use tower_cookies::CookieManagerLayer;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let state = AppState {
        share_dir: env::var("SHARE_DIR").expect("SHARE_DIR not set").into(),
        share_key: env::var("SHARE_KEY").expect("SHARE_KEY not set"),
    };

    let app = Router::new()
        .route("/", get(routes::index))
        .route("/login", get(routes::show_login_form))
        .route("/login", post(routes::process_login))
        .route("/download-zip", get(routes::download_zip))
        .route("/download/{filename}", get(routes::download_file))
        .with_state(state)
        .layer(CookieManagerLayer::new());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
