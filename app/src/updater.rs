//! In-app updates from GitHub Releases, with nsh's updater (`nsh-updater`).
//!
//! On only in builds packaged with nsh.json's `"updater"` section, which CI adds when the update
//! signing secrets are set (docs/desktop.md): `nsh package` then ships it as
//! `assets/nsh-updater.json`. Without it nothing here runs and no update request is made.
//!
//! - The manifest is `nsh-latest.json` of the latest release, signed with the release key
//!   (`latest.json.sig` beside it). Its own name keeps it apart from the Tauri app's `latest.json`
//!   while both apps are released.
//! - The first check runs a few seconds after start, then one every 6 hours while the app runs.
//! - A newer version opens a dialog: Install and Restart, or Later. Later stops the automatic
//!   prompts for that version until the app restarts.
//! - Install and Restart downloads the archive for this platform, checks its size and SHA-256
//!   against the signed manifest, stops the server the way quitting does (requests finish, runs
//!   are paused), replaces the installed app and restarts it.
//! - An app installed where the user cannot write (the .deb, an AppImage's mount) cannot replace
//!   itself: the dialog offers Download, which opens the release page.
//! - Automatic checks fail quietly; errors go to the log. Check for Updates… (the macOS app menu)
//!   reports the result or the error in a dialog, and so does a failed download or install.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use nsh_core::element::desktop::restart;
use nsh_core::prelude::*;
use nsh_updater::install::Installation;
use nsh_updater::{Available, Config, Download};

/// The first check, counted from the start of the app.
const FIRST_CHECK: Duration = Duration::from_secs(5);
/// Between automatic checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// The configuration `nsh package` shipped, as read and as bytes (the installation checks it).
struct Setup {
    config: Config,
    raw: Vec<u8>,
}

/// The shipped configuration, read once; `None` without one.
fn setup() -> Option<&'static Setup> {
    static SETUP: OnceLock<Option<Setup>> = OnceLock::new();
    SETUP.get_or_init(read_setup).as_ref()
}

fn read_setup() -> Option<Setup> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // macOS: Stellarator.app/Contents/MacOS/<exe> and Contents/Resources/assets; elsewhere the
    // assets folder sits beside the executable.
    let assets = if cfg!(target_os = "macos") { dir.parent()?.join("Resources").join("assets") } else { dir.join("assets") };
    let raw = std::fs::read(assets.join(nsh_updater::CONFIG_ASSET)).ok()?;
    match Config::parse(&raw) {
        Ok(config) => Some(Setup { config, raw }),
        Err(e) => {
            log::warn!("updates are off: {e}");
            None
        }
    }
}

/// Whether this build checks for updates.
pub fn available_here() -> bool {
    setup().is_some()
}

/// The versions the user answered Later to in this run.
static LATER: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Starts the automatic checks.
pub fn start() {
    let Some(setup) = setup() else { return };
    log::info!("updates on: {}", setup.config.manifest_url());
    if let Some(installation) = Installation::current(&setup.config.id, &setup.raw) {
        installation.remove_leftovers();
    }
    let _ = spawn(async {
        timer(FIRST_CHECK).await;
        loop {
            check(false).await;
            timer(CHECK_INTERVAL).await;
        }
    });
}

/// Check for Updates…: checks now and says what it found.
pub fn check_now() {
    let _ = spawn(check(true));
}

async fn check(manual: bool) {
    let Some(setup) = setup() else { return };
    match find(setup).await {
        Ok(None) => {
            if manual {
                let _ = nsh_base::alert_dialog()
                    .title("Stellarator is up to date")
                    .description(format!("You have version {}.", setup.config.version))
                    .action("ok", "OK")
                    .show()
                    .await;
            }
        }
        Ok(Some(update)) => {
            let later = LATER.lock().unwrap_or_else(|e| e.into_inner()).contains(&update.version);
            if manual || !later {
                offer(setup, update).await;
            }
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            if manual {
                let _ = nsh_base::alert_dialog()
                    .title("Could not check for updates")
                    .description(e)
                    .action("ok", "OK")
                    .show()
                    .await;
            }
        }
    }
}

/// The manifest, checked: a newer version for this machine, or none.
async fn find(setup: &'static Setup) -> Result<Option<Available>, String> {
    let manifest_url = setup.config.manifest_url();
    let signature_url = setup.config.signature_url();
    let (manifest, signature) = on_server_runtime(async move {
        let client = client()?;
        let manifest = get(&client, &manifest_url).await?;
        let signature = match signature_url {
            Some(url) => Some(get(&client, &url).await?),
            None => None,
        };
        Ok((manifest, signature))
    })
    .await?;
    setup.config.check(&manifest, signature.as_deref(), &nsh_updater::platform()).map_err(|e| e.to_string())
}

/// Asks the user, then installs or opens the download.
async fn offer(setup: &'static Setup, update: Available) {
    let installation = Installation::current(&setup.config.id, &setup.raw).filter(Installation::can_install);
    let can_install = installation.is_some() && update.sha256.is_some();
    let mut description = format!("You have version {}.", setup.config.version);
    if let Some(notes) = update.notes.as_deref().filter(|n| !n.trim().is_empty()) {
        description.push_str("\n\n");
        description.push_str(notes.trim());
    }
    let dialog = nsh_base::alert_dialog().title(format!("Stellarator {} is available", update.version)).description(description);
    let dialog = if can_install { dialog.action("install", "Install and Restart") } else { dialog.action("download", "Download") };
    let answer = dialog.cancel("later", "Later").show().await;
    match answer.ok().and_then(|a| a.action).as_deref() {
        Some("install") => {
            if let Some(installation) = installation {
                if let Err(e) = install(update, installation).await {
                    log::error!("the update could not be installed: {e}");
                    let _ = nsh_base::alert_dialog()
                        .title("The update could not be installed")
                        .description(e)
                        .action("ok", "OK")
                        .show()
                        .await;
                }
            }
        }
        Some("download") => {
            let _ = open_url(format!("{}/releases/latest", crate::desktop::REPO_URL));
        }
        _ => LATER.lock().unwrap_or_else(|e| e.into_inner()).push(update.version),
    }
}

/// Downloads and checks the archive, stops the server, replaces the app and restarts.
async fn install(update: Available, installation: Installation) -> Result<(), String> {
    let sha256 = update.sha256.clone().ok_or("the manifest names no archive for this computer")?;
    let name = if update.url.ends_with(".zip") { "update.zip" } else { "update.tar.gz" };
    let path: PathBuf = std::env::temp_dir().join(format!("stellarator-{}-{name}", update.version));
    let url = update.url.clone();
    let size = update.size;
    let file = path.clone();
    on_server_runtime(async move {
        let client = client()?;
        let mut response = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{url} answered {}", response.status()));
        }
        let mut download = Download::new(&sha256, size);
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            download.update(&chunk).map_err(|e| e.to_string())?;
            bytes.extend_from_slice(&chunk);
        }
        download.finish().map_err(|e| e.to_string())?;
        std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))
    })
    .await?;
    crate::backend::stop().await;
    let result = installation.install(&path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    result?;
    log::info!("installed Stellarator {}; restarting", update.version);
    restart();
    Ok(())
}

/// An HTTPS client (rustls, the system's proxy settings).
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("stellarator/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

/// The body of `url`, which must answer 200.
async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client.get(url).send().await.map_err(|e| format!("{url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response.bytes().await.map(|b| b.to_vec()).map_err(|e| format!("{url}: {e}"))
}

/// Runs `work` on the server's tokio runtime (reqwest needs one) and awaits it from nsh's.
async fn on_server_runtime<T: Send + 'static>(
    work: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    let backend = crate::backend::start()?;
    backend.runtime_handle().spawn(work).await.map_err(|e| e.to_string())?
}
