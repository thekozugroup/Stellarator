//! The web build's side: the API is the server that serves the page, at the page's origin, and
//! the address bar shows the screen as the web UI's routes do ([`follow_address`]).

use nsh_core::prelude::*;
use nsh_core::url_scheme::HistoryBehavior;

use crate::nav::{self, Route};

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
    let page = app_url().await.unwrap_or_default();
    origin_of(&page)
}

/// Keeps the address bar and the screen together, as the web UI's router does:
///
/// - The page's address at start opens its screen (a reload, a pasted link); `/` and paths the web
///   UI has no page for become `/runs` in place.
/// - Each screen the app moves to is a history entry of its own with the screen's path
///   ([`Route::path`]), so the browser's back and forward step through the app.
/// - The browser's back and forward show the screen they name ([`nav::show`]) without a new
///   entry, the entries of an earlier load (kept by a reload) too.
pub fn follow_address() {
    // The path the address bar shows, as the app last wrote or read it.
    let shown = signal(None::<String>);
    // Nothing is written until the page's own address has been read: it would replace the link.
    let started = signal(false);
    effect(move || {
        let path = nav::current().path();
        if !started.get() || shown.with_untracked(|s| s.as_deref() == Some(path.as_str())) {
            return;
        }
        shown.set(Some(path.clone()));
        let _ = set_app_url_with(path, HistoryBehavior::Push);
    });
    let _ = spawn(async move {
        let page = app_url().await.unwrap_or_default();
        let route = Route::from_path(&page).unwrap_or(Route::Runs);
        let path = route.path();
        if nav::path_of_address(&page) != path {
            let _ = set_app_url(path.clone());
        }
        shown.set(Some(path));
        nav::show(route);
        started.set(true);
        // An entry of an earlier load of the page (a reload keeps them) comes as a link that opens
        // the app at that address.
        let _ = spawn(async move {
            loop {
                let url = next_url_scheme_open().await;
                go_to(&url, shown);
            }
        });
        loop {
            let change = next_app_url_change().await;
            go_to(&change.url, shown);
        }
    });
}

/// Shows the screen of `url`, an address the browser moved to, without a new history entry.
fn go_to(url: &str, shown: Signal<Option<String>>) {
    if let Some(route) = Route::from_path(url) {
        shown.set(Some(route.path()));
        nav::show(route);
    }
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
