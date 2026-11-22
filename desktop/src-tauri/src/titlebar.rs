//! The macOS title bar: an overlay on the page, as in macOS 26 apps. The page toolbar runs across
//! the top of the content column, and the window controls (traffic lights) sit at the top left of
//! the sidebar panel, in the same band.
//!
//! - The window uses `TitleBarStyle::Overlay` with the title hidden, and [`use_toolbar_window`]
//!   gives it an empty unified toolbar. That is what macOS 26 apps with a sidebar have, and it
//!   changes two things (measured on macOS 26.6 with the same window style and a WKWebView):
//!   - the window's corners are 26 pt instead of 16 pt, so the floating sidebar (8 pt inside the
//!     window, 18 pt corners) is concentric with them;
//!   - macOS centers the traffic lights at (26, 26) pt, 19 pt from the left edge: in the middle of
//!     the 52 px band the page keeps free for them. Nothing moves them by hand.
//!   Clicks anywhere in that band still reach the page (the web view is the hit-tested view).
//! - An initialization script sets `window.__STELLARATOR_DESKTOP__` before the page's own scripts
//!   run: `titleBar: "overlay"` on macOS, and on every platform `windowRadius`, the window's corner
//!   radius in points ([`window_radius`]). The web UI (frontend/src/lib/desktop-shell.ts) reads
//!   it: the sidebar keeps the top `--titlebar-height` (52 px, frontend/src/index.css) free for the
//!   traffic lights, that band moves the window (Tauri's `data-tauri-drag-region`; a double click
//!   zooms), and every nested corner derives from `--window-radius`. The web UI served by Docker
//!   never gets the global and uses its design values.
//! - In full screen macOS hides the traffic lights; [`sync_fullscreen`] sets `data-fullscreen` on
//!   `<html>` and the page drops the reserved space.
//! - Dragging and zooming call two window commands over Tauri's IPC, from the page on 127.0.0.1.
//!   `capabilities/title-bar.json` allows exactly those two, for the main window only.
//!
//! Windows and Linux keep their native frames; only `windowRadius` reaches the page there.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, WebviewWindow};

/// The main window's corner radius in points, as macOS, Windows or Linux draws it. The page makes
/// its sidebar and every nested corner concentric with it, and below 20 it lays the sidebar flush
/// against the window's edge instead of floating it.
pub fn window_radius() -> u32 {
    #[cfg(target_os = "macos")]
    {
        // macOS 26: a window with a unified toolbar (use_toolbar_window). macOS 11 to 15: 10.
        if macos_major_version() >= 26 { 26 } else { 10 }
    }
    #[cfg(target_os = "windows")]
    {
        8
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        0
    }
}

#[cfg(target_os = "macos")]
fn macos_major_version() -> isize {
    objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion().majorVersion
}

/// Gives the window an empty unified toolbar: macOS 26's toolbar-window corners and traffic-light
/// placement (see the module docs). Runs on the main thread, where AppKit must be called.
#[cfg(target_os = "macos")]
pub fn use_toolbar_window(window: &WebviewWindow) {
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        use objc2::{MainThreadMarker, MainThreadOnly};
        use objc2_app_kit::{NSToolbar, NSWindow, NSWindowToolbarStyle};
        use objc2_foundation::NSString;

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(ptr) = target.ns_window() else {
            return;
        };
        // SAFETY: Tauri returns this window's NSWindow, which lives as long as the window, and we
        // are on the main thread.
        let ns_window: &NSWindow = unsafe { &*ptr.cast::<NSWindow>() };
        let toolbar = NSToolbar::initWithIdentifier(
            NSToolbar::alloc(mtm),
            &NSString::from_str("stellarator.main-window"),
        );
        ns_window.setToolbar(Some(&toolbar));
        ns_window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    });
}

/// Runs before the page's scripts on every load of `origin` (the local server) in the window.
/// Other origins never load in the window (main.rs blocks them), and the check keeps it that way.
pub fn init_script(origin: &str, overlay_title_bar: bool, radius: u32) -> String {
    let title_bar = if overlay_title_bar { "titleBar: \"overlay\", " } else { "" };
    format!(
        "if (window.location.origin === \"{origin}\") {{\n  \
           window.__STELLARATOR_DESKTOP__ = Object.freeze({{ {title_bar}windowRadius: {radius} }})\n\
         }}\n"
    )
}

/// Whether the main window was last seen in full screen. Changes are sent to the page.
#[derive(Default)]
pub struct Fullscreen(AtomicBool);

/// Tell the page whether the window is in full screen when that changed since the last call, or
/// when `page_loaded` and it is (a page loaded in full screen starts without the attribute).
pub fn sync_fullscreen(window: &WebviewWindow, page_loaded: bool) {
    let Ok(now) = window.is_fullscreen() else {
        return;
    };
    let before = window.state::<Fullscreen>().0.swap(now, Ordering::SeqCst);
    if before != now || (page_loaded && now) {
        let _ = window.eval(fullscreen_script(now));
    }
}

fn fullscreen_script(fullscreen: bool) -> String {
    format!("document.documentElement.toggleAttribute(\"data-fullscreen\", {fullscreen})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_script_sets_the_global_only_for_the_local_server() {
        let script = init_script("http://127.0.0.1:47823", true, 26);
        assert!(script.starts_with("if (window.location.origin === \"http://127.0.0.1:47823\") {"));
        assert!(script.contains(
            "window.__STELLARATOR_DESKTOP__ = Object.freeze({ titleBar: \"overlay\", windowRadius: 26 })"
        ));
    }

    #[test]
    fn init_script_without_the_overlay_only_gives_the_radius() {
        let script = init_script("http://127.0.0.1:47823", false, 8);
        assert!(script.contains("window.__STELLARATOR_DESKTOP__ = Object.freeze({ windowRadius: 8 })"));
    }

    #[test]
    fn fullscreen_script_toggles_the_attribute() {
        assert_eq!(
            fullscreen_script(true),
            "document.documentElement.toggleAttribute(\"data-fullscreen\", true)"
        );
        assert_eq!(
            fullscreen_script(false),
            "document.documentElement.toggleAttribute(\"data-fullscreen\", false)"
        );
    }
}
