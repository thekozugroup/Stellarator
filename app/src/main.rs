//! `cargo run`: the app on this machine's desktop backend (AppKit, Win32 or GTK 4), from the same
//! library the `nsh run` and `nsh package` runners mount.
//!
//! The order matters: the in-process server starts first (its database is open and interrupted
//! runs are recovered before the window shows; a failure is printed here), then the window runs
//! until the app quits, then the server stops. Quitting from the UI already stops it
//! (lib.rs, `quit_cleanly`); stopping again here is a no-op, and covers any other way out.

#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(e) = stellarator_app::backend::start() {
        eprintln!("Stellarator could not start: {e}");
        std::process::exit(1);
    }

    let ui = stellarator_app::registry();
    let assets = match asset_directory(&ui) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("could not lay out the app's assets: {e}");
            std::process::exit(1);
        }
    };
    platform::set_ui_sets(vec![nsh_base::registry(), ui]);
    let theme = nsh_base::Theme::from_json(include_str!("../theme.json")).expect("theme.json is a valid theme");
    let code = platform::run_app(
        platform::AppSource::Rust(
            platform::Root::of::<stellarator_app::App>()
                .tokens(theme.into_tokens())
                .app_id(stellarator_app::backend::APP_ID),
        ),
        platform::AppOptions {
            title: "Stellarator".to_owned(),
            asset_base: Some(platform::AssetBase::Directory(assets)),
            app_id: stellarator_app::backend::APP_ID.to_owned(),
            size: (1280, 820),
            ..Default::default()
        },
    );

    stellarator_app::backend::stop_blocking();
    code
}

/// The app's registered files (assets/) laid out as the runners serve them, next to the
/// executable, so `asset:/` URLs resolve as in a packaged app.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn asset_directory(registry: &nsh_core::element::Registry) -> std::io::Result<std::path::PathBuf> {
    let executable = std::env::current_exe()?;
    let base = executable.parent().unwrap_or(std::path::Path::new(".")).join("stellarator-assets");
    for file in registry.asset_files() {
        let destination = base.join(file.path());
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(file.source, destination)?;
    }
    Ok(base)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn main() {}
