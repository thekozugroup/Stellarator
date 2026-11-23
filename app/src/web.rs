//! The web build's side: the API is the server that serves the page, at the page's origin.

/// The origin (`http://127.0.0.1:8000`) of the page address `url`; empty when it has none.
pub fn origin_of(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    format!("{scheme}://{host}")
}

/// The page's origin, read from nsh's app address (the page's URL on the web).
pub async fn page_origin() -> String {
    let page = nsh_core::prelude::app_url().await.unwrap_or_default();
    origin_of(&page)
}

#[cfg(test)]
mod tests {
    use super::origin_of;

    #[test]
    fn the_origin_is_scheme_and_host() {
        assert_eq!(origin_of("http://127.0.0.1:8000/runs?x=1#a"), "http://127.0.0.1:8000");
        assert_eq!(origin_of("https://stellarator.example"), "https://stellarator.example");
        assert_eq!(origin_of("about:blank"), "");
    }
}
