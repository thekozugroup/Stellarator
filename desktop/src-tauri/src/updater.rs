//! In-app updates from GitHub Releases (tauri-plugin-updater).
//!
//! On only in builds whose config has `plugins.updater`. The desktop workflow adds it, with the
//! public key and the endpoint (`latest.json` of the latest release), when the signing secrets
//! are set; see docs/desktop.md. Without it the plugin is not registered, nothing here runs and
//! no update request is made.
//!
//! - The first check runs a few seconds after start, then one every 6 hours while the app runs.
//! - A newer version opens a dialog: Install and Restart, or Later. Later stops the automatic
//!   prompts for that version until the app is restarted.
//! - Install and Restart downloads the update (progress on the Dock or taskbar icon), stops the
//!   server the way quitting does (requests finish, runs are paused), installs the update and
//!   restarts the app. Runs are resumed by hand afterwards, as after any quit.
//! - Automatic checks fail quietly; errors go to the log. Check for Updates… in the macOS app menu
//!   reports the result or the error in a dialog, and so does a failed download or install.
//! - Linux: only the AppImage updates itself. A .deb install is updated by installing the new
//!   .deb with the package manager.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Config, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

use crate::MAIN;

/// The first check, counted from the start of the app.
const FIRST_CHECK: Duration = Duration::from_secs(5);
/// Between automatic checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Whether this build turns the updater on: its config has `plugins.updater`.
pub fn enabled(config: &Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .is_some_and(|v| !v.is_null())
}

/// Whether this install can replace itself. On Linux only an AppImage can; a .deb is updated
/// through the package manager.
fn supported(app: &AppHandle) -> bool {
    #[cfg(target_os = "linux")]
    {
        app.env().appimage.is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        true
    }
}

/// What the updater is doing, and the versions the user answered Later to in this session.
#[derive(Default)]
pub struct Updates(Mutex<State>);

#[derive(Default)]
struct State {
    phase: Phase,
    later: HashSet<String>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    #[default]
    Idle,
    /// Checking, or the Update available dialog is open.
    Checking,
    /// Downloading or installing; the app restarts when it is done.
    Installing,
}

impl Updates {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Move from Idle to Checking. Returns the phase found instead when it was not Idle.
    fn begin_check(&self) -> Result<(), Phase> {
        let mut state = self.lock();
        match state.phase {
            Phase::Idle => {
                state.phase = Phase::Checking;
                Ok(())
            }
            other => Err(other),
        }
    }

    fn set(&self, phase: Phase) {
        self.lock().phase = phase;
    }

    fn later(&self, version: &str) {
        let mut state = self.lock();
        state.later.insert(version.to_string());
        state.phase = Phase::Idle;
    }

    fn answered_later(&self, version: &str) -> bool {
        self.lock().later.contains(version)
    }
}

/// Start the automatic checks. Does nothing when the updater is off or this install cannot
/// update itself.
pub fn start(app: &AppHandle) {
    if !enabled(app.config()) {
        return;
    }
    if !supported(app) {
        tracing::info!("updates: only the AppImage updates itself; install a new .deb to update");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            check(app.clone(), false).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Check for Updates… in the menu: like an automatic check, but the result is always reported,
/// and a version answered with Later is offered again.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn check_now(app: &AppHandle) {
    if !enabled(app.config()) {
        return;
    }
    if !supported(app) {
        tell(
            app,
            MessageDialogKind::Info,
            "Updates come from the package manager",
            "Install the new .deb from the latest release to update Stellarator.",
        );
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(check(app, true));
}

async fn check(app: AppHandle, asked: bool) {
    let updates = app.state::<Updates>();
    match updates.begin_check() {
        Ok(()) => {}
        Err(Phase::Installing) if asked => {
            tell(
                &app,
                MessageDialogKind::Info,
                "Downloading the update",
                "Stellarator restarts when the update is installed.",
            );
            return;
        }
        // A check is running or its dialog is open.
        Err(_) => return,
    }

    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(update)) if asked || !updates.answered_later(&update.version) => {
            tracing::info!(version = %update.version, current = %update.current_version, "an update is available");
            // The phase stays Checking until the dialog is answered.
            offer(&app, update);
            return;
        }
        Ok(Some(update)) => {
            tracing::debug!(version = %update.version, "an update is available; the user chose Later");
        }
        Ok(None) => {
            tracing::info!("no update available");
            if asked {
                tell(
                    &app,
                    MessageDialogKind::Info,
                    "Stellarator is up to date",
                    &format!(
                        "Version {} is the latest release.",
                        app.package_info().version
                    ),
                );
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not check for updates");
            if asked {
                tell(
                    &app,
                    MessageDialogKind::Error,
                    "Could not check for updates",
                    &describe(&e),
                );
            }
        }
    }
    updates.set(Phase::Idle);
}

/// The Update available dialog.
fn offer(app: &AppHandle, update: Update) {
    let message = format!(
        "Stellarator {} is available. You have {}.\n\nInstalling it restarts Stellarator. Runs in \
         progress are paused and can be resumed after the restart.",
        update.version, update.current_version
    );
    let mut dialog = app
        .dialog()
        .message(message)
        .title("Update available")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install and Restart".into(),
            "Later".into(),
        ));
    if let Some(window) = visible_window(app) {
        dialog = dialog.parent(&window);
    }
    let app = app.clone();
    dialog.show(move |install| {
        if install {
            tauri::async_runtime::spawn(install_and_restart(app, update));
        } else {
            tracing::info!(version = %update.version, "update postponed for this session");
            app.state::<Updates>().later(&update.version);
        }
    });
}

/// Download, stop the server like quitting does, install, restart.
async fn install_and_restart(app: AppHandle, update: Update) {
    let updates = app.state::<Updates>();
    updates.set(Phase::Installing);
    tracing::info!(version = %update.version, url = %update.download_url, "downloading the update");

    let mut progress = DockProgress::new(app.get_webview_window(MAIN));
    let downloaded = update
        .download(|chunk, total| progress.add(chunk, total), || {})
        .await;
    progress.clear();
    let bytes = match downloaded {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(error = %e, "could not download the update");
            tell(
                &app,
                MessageDialogKind::Error,
                "Could not download the update",
                &describe(&e),
            );
            updates.set(Phase::Idle);
            return;
        }
    };

    // Runs are paused and the database is closed before the app is replaced. On Windows the
    // installer ends the process itself, so this must come first everywhere.
    if !crate::stop_server(&app).await {
        tracing::info!("the app is quitting; the update was not installed");
        return;
    }

    tracing::info!(version = %update.version, "installing the update");
    let installed = tauri::async_runtime::spawn_blocking({
        let update = update.clone();
        move || update.install(bytes)
    })
    .await;
    match installed {
        Ok(Ok(())) => {
            tracing::info!("update installed; restarting");
            app.restart();
        }
        Ok(Err(e)) => failed_install(&app, &update, &describe(&e)),
        Err(e) => failed_install(&app, &update, &e.to_string()),
    }
}

/// The server is already stopped, so the app restarts into the version it has.
fn failed_install(app: &AppHandle, update: &Update, error: &str) {
    tracing::error!(
        error,
        "could not install the update; restarting the current version"
    );
    let app_ = app.clone();
    app.dialog()
        .message(format!(
            "{error}\n\nStellarator restarts with version {}.",
            update.current_version
        ))
        .title("Could not install the update")
        .kind(MessageDialogKind::Error)
        .show(move |_| app_.restart());
}

/// Plain words for the errors a person can act on; the rest as the updater words them.
fn describe(e: &Error) -> String {
    match e {
        Error::ReleaseNotFound => "The latest release has no update information.".into(),
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "The latest release has no update for this computer.".into()
        }
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => {
            "The update's signature does not match this app's key, so it was not installed.".into()
        }
        other => other.to_string(),
    }
}

/// A dialog that reports something, attached to the window when it is on screen.
fn tell(app: &AppHandle, kind: MessageDialogKind, title: &str, message: &str) {
    let mut dialog = app.dialog().message(message).title(title).kind(kind);
    if let Some(window) = visible_window(app) {
        dialog = dialog.parent(&window);
    }
    dialog.show(|_| {});
}

fn visible_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
        .filter(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

/// Download progress on the Dock icon (macOS) or the taskbar button (Windows).
struct DockProgress {
    window: Option<WebviewWindow>,
    received: u64,
    shown: Option<u64>,
}

impl DockProgress {
    fn new(window: Option<WebviewWindow>) -> Self {
        Self {
            window,
            received: 0,
            shown: None,
        }
    }

    fn add(&mut self, chunk: usize, total: Option<u64>) {
        self.received += chunk as u64;
        let (status, percent) = match total {
            Some(total) if total > 0 => (
                ProgressBarStatus::Normal,
                Some(percent(self.received, total)),
            ),
            _ => (ProgressBarStatus::Indeterminate, None),
        };
        // Once per percent, not once per chunk.
        let shown = Some(percent.unwrap_or(u64::MAX));
        if self.shown == shown {
            return;
        }
        self.shown = shown;
        if let Some(window) = &self.window {
            let _ = window.set_progress_bar(ProgressBarState {
                status: Some(status),
                progress: percent,
            });
        }
    }

    fn clear(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_progress_bar(ProgressBarState {
                status: Some(ProgressBarStatus::None),
                progress: None,
            });
        }
    }
}

fn percent(received: u64, total: u64) -> u64 {
    (received.saturating_mul(100) / total).min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_updater_is_on_only_with_its_config() {
        let mut config = Config::default();
        assert!(!enabled(&config));
        config
            .plugins
            .0
            .insert("updater".into(), serde_json::Value::Null);
        assert!(!enabled(&config));
        config.plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "key", "endpoints": ["https://example.com/latest.json"] }),
        );
        assert!(enabled(&config));
    }

    #[test]
    fn later_is_remembered_per_version_and_ends_the_check() {
        let updates = Updates::default();
        assert!(updates.begin_check().is_ok());
        assert_eq!(updates.begin_check(), Err(Phase::Checking));
        updates.later("0.3.0");
        assert!(updates.answered_later("0.3.0"));
        assert!(!updates.answered_later("0.3.1"));
        assert!(updates.begin_check().is_ok());
        updates.set(Phase::Installing);
        assert_eq!(updates.begin_check(), Err(Phase::Installing));
    }

    #[test]
    fn percent_is_capped() {
        assert_eq!(percent(0, 200), 0);
        assert_eq!(percent(100, 200), 50);
        assert_eq!(percent(300, 200), 100);
    }

    #[test]
    fn missing_targets_are_described_plainly() {
        assert_eq!(
            describe(&Error::TargetsNotFound(vec!["darwin-x86_64".into()])),
            "The latest release has no update for this computer."
        );
    }
}
