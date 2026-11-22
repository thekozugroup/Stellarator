//! The Stellarator server inside the app's process: the database, the run engine, the imports and
//! the exports, assembled by `stellarator_server::full_parts` exactly as the Docker server and the
//! Tauri app assemble them. There is no TCP listener: the UI calls the server's axum router
//! directly ([`crate::api`]).
//!
//! The server runs on a tokio runtime of its own (threads named `stellarator-server`), apart from
//! nsh's runtime, so a slow request or a run never holds the UI's tasks.
//!
//! Life cycle:
//! - [`start`] opens the database and recovers interrupted runs and imports. It runs once per
//!   process: `src/main.rs` calls it before the window opens, and the UI set's setup
//!   (`lib.rs`) calls it in the runners `nsh run` and `nsh package` generate, which do not use
//!   `src/main.rs`.
//! - [`stop`] lets in-flight requests finish and pauses runs, waiting at most [`STOP_TIMEOUT`]. The
//!   UI calls it before it quits; `src/main.rs` calls [`stop_blocking`] after the window closed, in
//!   case nothing did.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use stellarator_core::Config;
use stellarator_core::config::Mode;
use stellarator_server::Lifecycle;

use crate::api::Api;

/// The app's id, as in nsh.json and desktop/src-tauri/tauri.conf.json. It names the data folder.
pub const APP_ID: &str = "com.thekozugroup.stellarator";

/// How long stopping waits for runs to pause (as the Tauri app, desktop/src-tauri/src/main.rs).
pub const STOP_TIMEOUT: Duration = Duration::from_secs(15);

/// The running server: its runtime, its router (inside [`Api`]) and its start and stop hooks.
pub struct Backend {
    runtime: tokio::runtime::Runtime,
    api: Api,
    lifecycle: Vec<Arc<dyn Lifecycle>>,
    data_dir: PathBuf,
    stopped: AtomicBool,
}

impl Backend {
    /// The API the UI calls.
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// The data folder in use.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
}

/// The server, once [`start`] ran: `Err` holds why it could not start, for the UI to show.
static BACKEND: OnceLock<Result<Backend, String>> = OnceLock::new();

/// Starts the server the first time it is called; later calls return the same result.
pub fn start() -> Result<&'static Backend, String> {
    BACKEND.get_or_init(|| open().map_err(|e| format!("{e:#}"))).as_ref().map_err(Clone::clone)
}

/// The server if it started.
pub fn get() -> Option<&'static Backend> {
    BACKEND.get().and_then(|b| b.as_ref().ok())
}

/// Why the server could not start, if it did not.
pub fn start_error() -> Option<String> {
    BACKEND.get().and_then(|b| b.as_ref().err().cloned())
}

fn open() -> anyhow::Result<Backend> {
    stellarator_server::init_tracing();
    let data_dir = data_dir()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("stellarator-server")
        .build()?;
    let mut config = Config::new(data_dir.clone(), Mode::Desktop);
    // No launch token: nothing listens on a port, so there is nothing to guard.
    config.launch_token = None;
    log::info!("starting the Stellarator server in-process, data folder {}", data_dir.display());
    let parts = runtime.block_on(stellarator_server::full_parts(config))?;
    for hook in &parts.lifecycle {
        runtime.block_on(hook.start())?;
    }
    let router = stellarator_server::app(&parts);
    let api = Api::in_process(router, runtime.handle().clone());
    Ok(Backend { runtime, api, lifecycle: parts.lifecycle, data_dir, stopped: AtomicBool::new(false) })
}

/// `STELLARATOR_DATA_DIR`, or the folder the Tauri app uses (`app_local_data_dir()` for the same
/// identifier), so both apps see the same runs: ~/Library/Application Support/<id> on macOS,
/// %LOCALAPPDATA%\<id> on Windows, ~/.local/share/<id> on Linux. Created if missing.
pub fn data_dir() -> anyhow::Result<PathBuf> {
    let dir = match std::env::var_os("STELLARATOR_DATA_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dirs::data_local_dir()
            .ok_or_else(|| anyhow::anyhow!("could not find the app data folder"))?
            .join(APP_ID),
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Stops the server the way quitting does: in-flight requests finish and runs are paused, waiting
/// at most [`STOP_TIMEOUT`]. Only the first call stops it; later ones return at once. Await it
/// from any executor: the work runs on the server's runtime.
pub async fn stop() {
    let Some(backend) = get() else { return };
    if backend.stopped.swap(true, Ordering::SeqCst) {
        return;
    }
    let hooks = backend.lifecycle.clone();
    let task = backend.runtime.spawn(stop_hooks(hooks));
    let _ = task.await;
}

/// [`stop`] for a thread outside any async runtime (src/main.rs, after the window closed).
pub fn stop_blocking() {
    let Some(backend) = get() else { return };
    if backend.stopped.swap(true, Ordering::SeqCst) {
        return;
    }
    backend.runtime.block_on(stop_hooks(backend.lifecycle.clone()));
}

async fn stop_hooks(hooks: Vec<Arc<dyn Lifecycle>>) {
    log::info!("stopping the Stellarator server");
    let all = async {
        for hook in &hooks {
            hook.stop().await;
        }
    };
    match tokio::time::timeout(STOP_TIMEOUT, all).await {
        Ok(()) => log::info!("the Stellarator server stopped"),
        Err(_) => log::warn!(
            "the Stellarator server did not stop within {} s; quitting anyway",
            STOP_TIMEOUT.as_secs()
        ),
    }
}
