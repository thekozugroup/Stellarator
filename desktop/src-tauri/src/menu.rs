//! The macOS menu bar: the standard app, Edit, View, Window and Help menus. The app menu has
//! Check for Updates… when the build has the updater (updater.rs).
//!
//! The Edit menu is required, not decoration: WKWebView text fields get Cmd+C, Cmd+V, Cmd+Z and
//! Cmd+A only through these menu items.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Url};

use crate::{open_in_browser, updater, MAIN, REPO_URL};

const CHECK_FOR_UPDATES: &str = "check-for-updates";
const QUIT: &str = "quit";
const RELOAD: &str = "reload";
const GITHUB: &str = "github";

/// Build the menu bar and make it the app's menu.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let name = "Stellarator";
    let about = AboutMetadata {
        name: Some(name.into()),
        version: Some(app.package_info().version.to_string()),
        ..Default::default()
    };
    let sep = || PredefinedMenuItem::separator(app);

    let app_menu = Submenu::with_items(
        app,
        name,
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Stellarator"), Some(about))?,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some("Hide Stellarator"))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &sep()?,
            // Not the predefined Quit item: that one ends the process through AppKit's
            // terminate: without RunEvent::ExitRequested, and quitting stops the server first.
            &MenuItem::with_id(app, QUIT, "Quit Stellarator", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    // Right below About, where Mac apps put it. Only in builds with the updater.
    if updater::enabled(app.config()) {
        let check = MenuItem::with_id(
            app,
            CHECK_FOR_UPDATES,
            "Check for Updates…",
            true,
            None::<&str>,
        )?;
        app_menu.insert(&check, 1)?;
    }
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, RELOAD, "Reload", true, Some("CmdOrCtrl+R"))?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, Some("Toggle Full Screen"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        "Help",
        true,
        &[&MenuItem::with_id(
            app,
            GITHUB,
            "Stellarator on GitHub",
            true,
            None::<&str>,
        )?],
    )?;

    let menu = Menu::with_items(app, &[&app_menu, &edit, &view, &window, &help])?;
    app.set_menu(menu)?;
    // After set_menu: AppKit adds the window list to the Window menu and the search field to Help.
    #[cfg(target_os = "macos")]
    {
        window.set_as_windows_menu_for_nsapp()?;
        help.set_as_help_menu_for_nsapp()?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, help);
    Ok(())
}

pub fn on_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        // Goes through RunEvent::ExitRequested, which stops the server before exiting.
        QUIT => app.exit(0),
        CHECK_FOR_UPDATES => updater::check_now(app),
        RELOAD => {
            if let Some(window) = app.get_webview_window(MAIN) {
                let _ = window.reload();
            }
        }
        GITHUB => {
            if let Ok(url) = Url::parse(REPO_URL) {
                open_in_browser(&url);
            }
        }
        _ => {}
    }
}
