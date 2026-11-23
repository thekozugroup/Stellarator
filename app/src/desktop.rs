//! The app as a desktop app: what happens around the window rather than in it.
//!
//! - **One instance.** nsh.json `"singleInstance": true`: on Windows and Linux a second launch
//!   hands its command line to this process and exits; the window comes forward ([`install`]
//!   listens and focuses it). macOS's Launch Services does the same for an app bundle. Across
//!   apps (this one and the Tauri app, or a server started from a terminal) the data folder's lock
//!   keeps a second process away from the database (crates/core/src/datalock.rs): it shows why in
//!   [`startup_failure`] and quits.
//! - **The window's place.** nsh.json `"window": { "restoreBounds": true }`: nsh keeps the main
//!   window's size, position and maximized or full-screen mode and opens it there next time. On
//!   macOS (nsh 0.6.1) the window comes back 32 pt shorter each launch, its bottom edge in place,
//!   seemingly as AppKit keeps the content size when the hidden title bar (full-size content)
//!   applies after nsh placed the window. [`show_window`] puts the kept place back before the
//!   window shows; with that fixed in nsh it changes nothing.
//! - **No flash at launch.** nsh.json `"defaultVisible": false`: the window opens hidden;
//!   [`show_window`] shows it once it is ready (on macOS after the toolbar that gives its corners,
//!   `platform::setup_window`), so it never shows with the plain title bar's corners first.
//! - **Menus.** The macOS menu bar: the app menu (About, Check for Updates… when the build has
//!   the updater, Settings…, Hide, Quit), Edit, View (Back, full screen), Window and Help. Windows
//!   and Linux have no menu bar, as the Tauri app: their text fields handle the editing keys
//!   themselves, and the app's keys are in `shortcuts`.

use std::cell::Cell;
use std::time::Duration;

use nsh_core::desktop::{MenuRole, WindowMode, WindowPlacement};
use nsh_core::element::desktop::{MenuItem, main_window, set_application_menu};
use nsh_core::platform::Os;
use nsh_core::prelude::*;

use crate::nav::{self, Destination};

/// The repository, for Help.
pub const REPO_URL: &str = "https://github.com/thekozugroup/Stellarator";

/// How long the window may stay hidden at launch, whatever happens: past it, it shows.
const SHOW_AT_THE_LATEST: Duration = Duration::from_millis(1500);

/// Where nsh keeps the main window's place (nsh-core's `placement_key("main")`).
const KEPT_PLACEMENT_KEY: &str = "nsh.window:main";

thread_local! {
    /// macOS: the main window's place from the last run, read before this run changes it; put
    /// back once, when the window first shows.
    static KEPT: Cell<Option<WindowPlacement>> = const { Cell::new(None) };
}

/// Sets up the desktop side. Called once from the root component.
pub fn install() {
    if !os().is_desktop() {
        return;
    }
    if os() == Os::MacOs {
        let kept = nsh_core::storage::Storage::get_item(KEPT_PLACEMENT_KEY)
            .and_then(|json| serde_json::from_str::<WindowPlacement>(&json).ok())
            .filter(|p| p.mode == WindowMode::Normal);
        KEPT.with(|k| k.set(kept));
        set_application_menu(mac_menu());
    } else {
        // No toolbar to wait for: show the window with its first frame.
        show_window();
    }
    // Whatever the platform did, the window shows.
    let _ = set_timeout(SHOW_AT_THE_LATEST, show_window);
    // A second launch (Windows, Linux): bring this window forward, out of the Dock or taskbar.
    let _ = spawn(async {
        loop {
            let _ = next_second_instance().await;
            let window = main_window();
            window.restore();
            window.focus();
        }
    });
    startup_failure();
    #[cfg(not(target_arch = "wasm32"))]
    crate::updater::start();
}

/// Shows the main window (it opens hidden: nsh.json `defaultVisible`). Showing a shown window
/// changes nothing.
pub fn show_window() {
    if os().is_desktop() {
        let window = main_window();
        if let Some(kept) = KEPT.with(Cell::take) {
            window.set_placement(kept);
        }
        window.show();
    }
}

/// When the server could not start (another process holds the data folder, the database cannot
/// be opened), the reason in a dialog, then quit: the UI is no use without it.
fn startup_failure() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(error) = crate::backend::start_error() else { return };
        let busy = error.contains("Another Stellarator is already using the data folder");
        let title = if busy { "Stellarator is already open" } else { "Stellarator could not start" };
        show_window();
        // To the front, the app active: a copy started from a terminal is not, and its dialog
        // would open behind the other windows.
        main_window().focus();
        let _ = spawn(async move {
            let _ = nsh_base::alert_dialog().title(title).description(error).action("quit", "Quit").show().await;
            nsh_core::element::desktop::quit();
        });
    }
}

/// The macOS menu bar.
fn mac_menu() -> Vec<MenuItem> {
    let mut app_menu = vec![MenuItem::role(MenuRole::About).label("About Stellarator"), MenuItem::separator()];
    #[cfg(not(target_arch = "wasm32"))]
    if crate::updater::available_here() {
        app_menu.push(MenuItem::new("Check for Updates…").on_select(|| crate::updater::check_now()));
        app_menu.push(MenuItem::separator());
    }
    app_menu.extend([
        MenuItem::new("Settings…").shortcut(crate::shortcuts::SETTINGS).on_select(|| nav::open(Destination::Settings)),
        MenuItem::separator(),
        MenuItem::role(MenuRole::Hide).label("Hide Stellarator"),
        MenuItem::role(MenuRole::HideOthers),
        MenuItem::role(MenuRole::ShowAll),
        MenuItem::separator(),
        MenuItem::role(MenuRole::Quit).label("Quit Stellarator"),
    ]);
    vec![
        MenuItem::submenu("Stellarator", app_menu),
        MenuItem::submenu(
            "Edit",
            [
                MenuItem::role(MenuRole::Undo),
                MenuItem::role(MenuRole::Redo),
                MenuItem::separator(),
                MenuItem::role(MenuRole::Cut),
                MenuItem::role(MenuRole::Copy),
                MenuItem::role(MenuRole::Paste),
                MenuItem::role(MenuRole::SelectAll),
            ],
        ),
        // No Enter Full Screen item here: AppKit adds its own to the View menu. nsh takes the first
        // submenu with a window role (full screen among them) for the Window menu, the one macOS
        // lists the open windows and the tiling items in, and that has to be Window.
        MenuItem::submenu(
            "View",
            [MenuItem::new("Back").shortcut(crate::shortcuts::BACK).on_select(|| {
                nav::back();
            })],
        ),
        MenuItem::submenu(
            "Window",
            [MenuItem::role(MenuRole::Minimize), MenuItem::role(MenuRole::Zoom), MenuItem::separator(), MenuItem::role(MenuRole::Close)],
        ),
        MenuItem::submenu(
            "Help",
            [MenuItem::new("Stellarator on GitHub").on_select(|| {
                let _ = open_url(REPO_URL);
            })],
        ),
    ]
}
