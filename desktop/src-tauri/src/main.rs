//! Stellarator desktop app: one native process holding Tauri's window and the Stellarator server.
//!
//! The server runs in-process on 127.0.0.1 and serves the same web UI and API as the Docker
//! image. It answers only requests that carry the cookie set from a per-launch token (see
//! crates/server/src/guard.rs), so other programs and web pages on the machine cannot drive it.
//! The window opens `http://127.0.0.1:<port>/?launch=<token>`; the server trades the token for
//! the cookie and redirects to `/`.
//!
//! Quitting (closing the window, Cmd+Q, Dock > Quit) stops the server before the process ends,
//! so runs are paused cleanly and resume on the next launch.

// Release builds on Windows: no console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod downloads;
// macOS only; also compiled by tests elsewhere so CI on Linux type-checks it.
#[cfg(any(target_os = "macos", test))]
mod menu;

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::Context;
use stellarator_core::config::Mode;
use stellarator_core::Config;
use stellarator_server::{Parts, RunningServer};
use tauri::async_runtime::JoinHandle;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Manager, RunEvent, Theme, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_window_state::StateFlags;

/// The label of the app's one window.
const MAIN: &str = "main";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const REPO_URL: &str = "https://github.com/thekozugroup/Stellarator";

/// The port tried first. A fixed port keeps the page's origin, and with it what the web UI keeps
/// in localStorage (theme, the Rendered/Raw switch), the same from one launch to the next. When
/// it is taken, any free port is used. `STELLARATOR_DESKTOP_PORT` overrides it; 0 means any port.
const PREFERRED_PORT: u16 = 47823;

/// How long quitting waits for the server to stop (requests finish, runs are paused).
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

/// Show the window this long after creating it even if the page has not reported that it loaded.
const SHOW_FALLBACK: Duration = Duration::from_secs(3);

/// The page ground, `--background` in frontend/src/index.css: oklch(0.99 0 0) and
/// oklch(0.165 0 0) in sRGB. The window shows it before the page paints and while resizing.
const GROUND_LIGHT: Color = Color(252, 252, 252, 255);
const GROUND_DARK: Color = Color(14, 14, 14, 255);

/// The in-process server, from start to stop.
enum Server {
    /// Not started (or failed to start), or already stopped.
    Idle,
    Running(RunningServer),
    /// Quitting: this task is stopping the server and exits the app when done.
    Stopping(JoinHandle<()>),
}

struct ServerState {
    server: Mutex<Server>,
    /// Set when `RunEvent::Exit` arrives: the process ends as soon as that handler returns.
    exiting: AtomicBool,
}

impl ServerState {
    fn lock(&self) -> MutexGuard<'_, Server> {
        self.server.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn main() {
    stellarator_server::init_tracing();

    let builder = tauri::Builder::default()
        // First: a second launch hands over to the running app before anything else starts, so
        // two servers never share one database.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app)
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(ServerState {
            server: Mutex::new(Server::Idle),
            exiting: AtomicBool::new(false),
        })
        .manage(downloads::Downloads::default())
        // The macOS menu is installed in `setup`. Windows and Linux get no menu bar: the web UI
        // has its own navigation, and their webviews handle the editing shortcuts themselves.
        .enable_macos_default_menu(false)
        .setup(|app| {
            setup(app.handle());
            Ok(())
        });
    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(menu::on_event);

    match builder.build(tauri::generate_context!()) {
        Ok(app) => app.run(on_run_event),
        Err(e) => {
            tracing::error!(error = %e, "could not start the app");
            std::process::exit(1);
        }
    }
}

fn setup(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Err(e) = menu::install(app) {
        tracing::warn!(error = %e, "could not install the menu bar");
    }

    match tauri::async_runtime::block_on(start_server(app)) {
        Ok((server, token)) => {
            let port = server.addr.port();
            *app.state::<ServerState>().lock() = Server::Running(server);
            if let Err(e) = open_main_window(app, port, &token) {
                fail(app, e.context("could not open the window"));
            }
        }
        Err(e) => fail(app, e),
    }
}

/// Open the database and start the server on 127.0.0.1. Returns the server and its launch token.
async fn start_server(app: &AppHandle) -> anyhow::Result<(RunningServer, String)> {
    let token = launch_token()?;
    let mut config = Config::new(data_dir(app)?, Mode::Desktop);
    config.launch_token = Some(token.clone());
    tracing::info!(data_dir = %config.data_dir.display(), "starting the Stellarator server");

    let any_port = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server = match preferred_addr() {
        Some(addr) => {
            match stellarator_server::start(build_parts(config.clone()).await?, addr).await {
                Ok(server) => server,
                // Taken between the probe and the bind: use any free port.
                Err(e) if is_addr_in_use(&e) => {
                    stellarator_server::start(build_parts(config).await?, any_port).await?
                }
                Err(e) => return Err(e),
            }
        }
        None => stellarator_server::start(build_parts(config).await?, any_port).await?,
    };
    Ok((server, token))
}

/// The server's parts: database, routes and background work.
// TODO(lead): switch to `stellarator_server::full_parts` (engine, imports and exports wired in,
// as crates/server/src/main.rs does) once those crates are settled. Until then the app serves
// only the server crate's own routes.
async fn build_parts(config: Config) -> anyhow::Result<Parts> {
    stellarator_server::full_parts(config).await
}

/// `STELLARATOR_DATA_DIR`, or the app's data folder: ~/Library/Application Support/<identifier>
/// on macOS, %LOCALAPPDATA%\<identifier> on Windows, ~/.local/share/<identifier> on Linux.
fn data_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = match std::env::var_os("STELLARATOR_DATA_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => app
            .path()
            .app_local_data_dir()
            .context("could not find the app data folder")?,
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    Ok(dir)
}

/// 32 random bytes as hex, new for every launch.
fn launch_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("no random source for the launch token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The preferred address, if it is free right now. The probe keeps a busy port from costing a
/// second database open; `start` binds again right after.
fn preferred_addr() -> Option<SocketAddr> {
    let port = match std::env::var("STELLARATOR_DESKTOP_PORT") {
        Ok(v) => v.trim().parse().unwrap_or_else(|_| {
            tracing::warn!(value = %v, "STELLARATOR_DESKTOP_PORT is not a port number; ignoring it");
            PREFERRED_PORT
        }),
        Err(_) => PREFERRED_PORT,
    };
    if port == 0 {
        return None;
    }
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpListener::bind(addr).ok().map(|_| addr)
}

fn is_addr_in_use(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<std::io::Error>())
        .any(|e| e.kind() == std::io::ErrorKind::AddrInUse)
}

fn open_main_window(app: &AppHandle, port: u16, token: &str) -> anyhow::Result<()> {
    let home = Url::parse(&format!("http://127.0.0.1:{port}/"))?;
    let mut start = home.clone();
    start.set_query(Some(&format!("launch={token}")));
    let origin = home.origin();
    let popup_origin = origin.clone();
    let popup_app = app.clone();

    let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::External(start))
        .title("Stellarator")
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .center()
        // Shown once the page has loaded, so the first frame is the UI, not an empty window.
        .visible(false)
        .background_color(GROUND_LIGHT)
        // Only the local server loads in the window; every other link opens in the browser.
        .on_navigation(move |url| {
            if url.origin() == origin || url.scheme() == "about" {
                return true;
            }
            open_in_browser(url);
            false
        })
        // target="_blank" links and window.open: the browser, or this window for the app's own
        // pages (a browser tab would not have the session cookie).
        .on_new_window(move |url, _features| {
            if url.origin() == popup_origin {
                navigate_main(&popup_app, url);
            } else {
                open_in_browser(&url);
            }
            NewWindowResponse::Deny
        })
        .on_download(downloads::handle)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished) {
                reveal(&window);
            }
        })
        .build()?;

    let _ = window.set_background_color(Some(ground(window.theme().unwrap_or(Theme::Light))));
    let late = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_FALLBACK).await;
        reveal(&late);
    });
    Ok(())
}

fn ground(theme: Theme) -> Color {
    match theme {
        Theme::Dark => GROUND_DARK,
        _ => GROUND_LIGHT,
    }
}

/// Show the window the first time; later page loads (Reload) leave it as it is.
fn reveal(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn navigate_main(app: &AppHandle, url: Url) {
    // Not from inside the webview's callback: hand it to the event loop.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(window) = app.get_webview_window(MAIN) {
            let _ = window.navigate(url);
        }
    });
}

/// Open a link in the default browser (or mail app). Other schemes are not opened.
fn open_in_browser(url: &Url) {
    match url.scheme() {
        "http" | "https" | "mailto" => {
            if let Err(e) = tauri_plugin_opener::open_url(url.as_str(), None::<&str>) {
                tracing::warn!(%url, error = %e, "could not open the link in the browser");
            }
        }
        scheme => tracing::warn!(scheme, "blocked a link to a scheme the app does not open"),
    }
}

/// Startup failed: say why in a dialog, then quit.
fn fail(app: &AppHandle, error: anyhow::Error) {
    tracing::error!(error = %format!("{error:#}"), "Stellarator could not start");
    let app_ = app.clone();
    app.dialog()
        .message(format!("Stellarator could not start: {error:#}"))
        .title("Stellarator")
        .kind(MessageDialogKind::Error)
        .show(move |_| app_.exit(1));
}

fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        // The last window closed, Cmd+Q (menu.rs), or `app.exit`: stop the server first, with
        // the window already gone, then exit for real.
        RunEvent::ExitRequested { code, api, .. } => {
            let state = app.state::<ServerState>();
            let mut server = state.lock();
            match std::mem::replace(&mut *server, Server::Idle) {
                Server::Running(running) => {
                    api.prevent_exit();
                    hide_windows(app);
                    let app = app.clone();
                    *server = Server::Stopping(tauri::async_runtime::spawn(async move {
                        stop(running).await;
                        let state = app.state::<ServerState>();
                        *state.lock() = Server::Idle;
                        if !state.exiting.load(Ordering::SeqCst) {
                            app.exit(code.unwrap_or(0));
                        }
                    }));
                }
                Server::Stopping(task) => {
                    api.prevent_exit();
                    *server = Server::Stopping(task);
                }
                Server::Idle => {}
            }
        }
        // macOS can end the app without asking first (Dock > Quit, logging out). The process ends
        // when this returns, so stop the server here, blocking.
        RunEvent::Exit => {
            let state = app.state::<ServerState>();
            state.exiting.store(true, Ordering::SeqCst);
            let pending = std::mem::replace(&mut *state.lock(), Server::Idle);
            match pending {
                Server::Running(running) => {
                    hide_windows(app);
                    tauri::async_runtime::block_on(stop(running));
                }
                Server::Stopping(task) => {
                    let _ = tauri::async_runtime::block_on(task);
                }
                Server::Idle => {}
            }
        }
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::ThemeChanged(theme),
            ..
        } => {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.set_background_color(Some(ground(theme)));
            }
        }
        _ => {}
    }
}

fn hide_windows(app: &AppHandle) {
    for window in app.webview_windows().values() {
        let _ = window.hide();
    }
}

async fn stop(server: RunningServer) {
    tracing::info!("stopping the Stellarator server");
    match tokio::time::timeout(SHUTDOWN_TIMEOUT, server.shutdown()).await {
        Ok(()) => tracing::info!("the Stellarator server stopped"),
        Err(_) => tracing::warn!(
            seconds = SHUTDOWN_TIMEOUT.as_secs(),
            "the Stellarator server did not stop in time; quitting anyway"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_tokens_are_64_hex_digits_and_differ() {
        let a = launch_token().unwrap();
        let b = launch_token().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn addr_in_use_is_found_through_context() {
        let e = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::AddrInUse))
            .context("bind");
        assert!(is_addr_in_use(&e));
        assert!(!is_addr_in_use(&anyhow::anyhow!("other")));
    }
}
