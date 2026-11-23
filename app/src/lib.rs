//! Stellarator's app on nsh (Native Shell): the whole UI in Rust, rendered with each platform's
//! own UI (AppKit on macOS, Win32 on Windows, GTK 4 on Linux, the DOM in a browser).
//!
//! On the desktops the Stellarator server runs inside the app's process ([`backend`]) and the UI
//! calls its router directly ([`api`]); in a browser the same screens reach the server that serves
//! the page. The design is DESIGN.md's: the tokens in theme.json, the type scale and radii in
//! [`theme`], the components in [`ui`], the screens in [`screens`].
//!
//! nsh.json names [`App`] as the root: `nsh run` and `nsh package` generate a runner that mounts
//! it; `cargo run` starts it from src/main.rs.

pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod backend;
mod format;
mod platform;
mod screens;
mod theme;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

use nsh_core::desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes};
use nsh_core::element::desktop::{on_app_quit, on_quit_request, quit, window as app_window};
use nsh_core::platform::Os;
use nsh_core::prelude::*;

use crate::theme::metrics::TOOLBAR_HEIGHT;
use crate::theme::shape::WindowShape;

/// The UI set of the app's crate (nsh.json `"rust"`): its root and the files it ships (fonts,
/// icons). Its setup starts the in-process server, before the first render, in every runner.
#[native_module("stellarator/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
    registry.assets(assets!());
    #[cfg(not(target_arch = "wasm32"))]
    registry.setup(|| {
        if let Err(e) = backend::start() {
            log::error!("the Stellarator server could not start: {e}");
        }
    });
}

/// The API the screens call: the in-process server on the desktops, the page's server on the web.
pub(crate) fn api() -> Result<api::Api, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        backend::start().map(|b| b.api().clone())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(api::Api::same_origin())
    }
}

/// Stellarator: one window with the sidebar and the open screen.
#[component]
pub fn App() -> impl IntoElement {
    global::<WindowShape>().set(WindowShape::expected(Some(os()), os_version()));
    platform::setup_window();
    // A single-window tool: closing the window or quitting stops the server first (requests
    // finish, runs are paused), then quits, as in the Tauri app.
    on_quit_request(quit_cleanly);
    on_app_quit(|| async {
        #[cfg(not(target_arch = "wasm32"))]
        backend::stop().await;
    });
    app_window()
        .id("main")
        .title("Stellarator")
        .default_size(size(px(1280.0), px(820.0)))
        .attributes(main_window_attributes())
        .on_close_press(|_| quit_cleanly())
        .content(|| ui::shell::shell())
}

/// The main window's attributes: on macOS the title bar hidden, with the traffic lights centered
/// in the 52 px band; elsewhere the system's frame. Closing it goes through [`quit_cleanly`].
fn main_window_attributes() -> WindowAttributes {
    let mut attributes = WindowAttributes::default();
    attributes.min_size = Some(Some(size(px(900.0), px(600.0))));
    attributes.close_on_press = Some(false);
    if os() == Os::MacOs {
        let mut bar = TitleBarOptions::default();
        bar.style = TitleBarStyle::Hidden;
        bar.height = Some(px(TOOLBAR_HEIGHT));
        attributes.title_bar = Some(bar);
    }
    attributes
}

/// Stops the server (in-flight requests finish, runs are paused), then quits.
fn quit_cleanly() {
    spawn(async {
        #[cfg(not(target_arch = "wasm32"))]
        backend::stop().await;
        quit();
    });
}
