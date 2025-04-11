use subtle::ConstantTimeEq;
use tower_cookies::Cookies;

pub fn verify_user_sent_key(provided: &str, expected: &str) -> bool {
    verify_key(provided, expected)
}

pub fn verify_cookie_key(cookies: &Cookies, expected: &str) -> bool {
    if let Some(cookie) = cookies.get("share_key") {
        verify_key(cookie.value(), expected)
    } else {
        false
    }
}

// Constant-time
fn verify_key(provided: &str, expected: &str) -> bool {
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}
