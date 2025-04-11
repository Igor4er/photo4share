use askama::Template;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub share_dir: PathBuf,
    pub share_key: String,
    pub greet: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: String,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub key: String,
    pub csrf_token: String,
}

#[derive(Template)]
#[template(path = "list.html")]
pub struct ListTemplate {
    pub files: Vec<String>,
    pub greet: String,
}

#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorTemplate {
    pub error_code: String,
    pub error_message: String,
}
