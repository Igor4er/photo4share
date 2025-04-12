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
use tracing::{Level, info};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stdout))
        .with(
            EnvFilter::from_default_env()
                .add_directive(Level::INFO.into())
                .add_directive("photo4share=debug".parse().unwrap()),
        )
        .init();

    info!("Starting photo4share application");

    let share_dir = env::var("SHARE_DIR").expect("SHARE_DIR not set");
    let share_key = env::var("SHARE_KEY").expect("SHARE_KEY not set");
    let greet = env::var("GREET").expect("GREET not set");

    info!("Configuration loaded, share directory: {}", share_dir);

    let state = AppState {
        share_dir: share_dir.into(),
        share_key,
        greet,
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

    info!("Router configured, starting server on 0.0.0.0:3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    info!("Server shutdown");
}
