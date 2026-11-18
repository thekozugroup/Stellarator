//! The macOS title bar: an overlay on the page, so the sidebar runs to the top of the window with
//! the window controls (traffic lights) on it, as in Notes or Mail.
//!
//! - The window uses `TitleBarStyle::Overlay` with the title hidden, and the traffic lights are
//!   moved to [`TRAFFIC_LIGHTS`] (see `open_main_window` in main.rs).
//! - An initialization script sets `window.__STELLARATOR_DESKTOP__ = { titleBar: "overlay" }`
//!   before the page's own scripts run. The web UI (frontend/src/lib/desktop-shell.ts) reads it
//!   and sets `data-titlebar="overlay"` on `<html>`: the sidebar keeps the top
//!   `--titlebar-height` (40 px, frontend/src/index.css) free for the traffic lights, and a strip
//!   that tall across the top of the window moves it (Tauri's `data-tauri-drag-region`; a double
//!   click zooms). The web UI served by Docker never gets the global and is unchanged.
//! - In full screen macOS hides the traffic lights; [`sync_fullscreen`] sets `data-fullscreen` on
//!   `<html>` and the page drops the reserved space.
//! - Dragging and zooming call two window commands over Tauri's IPC, from the page on 127.0.0.1.
//!   `capabilities/title-bar.json` allows exactly those two, for the main window only.
//!
//! Windows and Linux keep their native frames, and nothing here runs there.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{LogicalPosition, Manager, WebviewWindow};

/// The close button's position from the window's top-left corner, in points. With the 40 px
/// `--titlebar-height` strip in the web UI, the buttons sit centered in it and line up with the
/// sidebar's logo (16 px from the left edge). Change both together.
pub const TRAFFIC_LIGHTS: LogicalPosition<f64> = LogicalPosition { x: 16.0, y: 18.0 };

/// Whether the main window was last seen in full screen. Changes are sent to the page.
#[derive(Default)]
pub struct Fullscreen(AtomicBool);

/// Runs before the page's scripts on every load of `origin` (the local server) in the window.
/// Other origins never load in the window (main.rs blocks them), and the check keeps it that way.
pub fn init_script(origin: &str) -> String {
    format!(
        "if (window.location.origin === \"{origin}\") {{\n  \
           window.__STELLARATOR_DESKTOP__ = Object.freeze({{ titleBar: \"overlay\" }})\n\
         }}\n"
    )
}

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
        let script = init_script("http://127.0.0.1:47823");
        assert!(script.starts_with("if (window.location.origin === \"http://127.0.0.1:47823\") {"));
        assert!(script
            .contains("window.__STELLARATOR_DESKTOP__ = Object.freeze({ titleBar: \"overlay\" })"));
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
