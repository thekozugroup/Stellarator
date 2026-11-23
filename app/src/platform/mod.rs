//! The window's platform side: on macOS the empty unified toolbar that gives macOS 26's
//! toolbar-window shape ([`macos`]), the window radius the layout derives its corners from
//! ([`crate::theme::shape`]), and a diagnostic probe.
//!
//! The probe runs when the environment variable `STELLARATOR_PROBE` is `1`. About 3 s after launch
//! it prints to standard output, one `probe:` line each: the OS and its version, the window radius
//! the layout uses, and on macOS the window's `_cornerRadius` (when the window answers that
//! selector) before and after the toolbar, the traffic lights' frames, and which view the window's
//! frame view hits at (140, 4), (140, 26), (140, 40) and (300, 26) from the window's top left. At
//! 10 s it posts left clicks to the band (the sidebar's at (140, 26), the toolbar's at (300, 26))
//! and to the New run button's place at (window width - 80, 26); views of the app that receive them
//! print `probe: press reached nsh: <what>`. The app keeps running.
//!
//! `STELLARATOR_PROBE=drag` prints the same readings and posts no clicks; the band then has no
//! press handlers, so CI can drag the window by it with the system's mouse.
//!
//! `STELLARATOR_NO_TOOLBAR=1` leaves the toolbar out, to compare the window without it.

#[cfg(target_os = "macos")]
mod macos;

use std::time::Duration;

use nsh_core::prelude::*;

use crate::theme::shape::WindowShape;

/// Whether the probe runs (`STELLARATOR_PROBE=1`, or `drag`).
pub fn probing() -> bool {
    probe_mode() != ProbeMode::Off
}

/// What the probe does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    Off,
    /// `STELLARATOR_PROBE=1`: the readings, then clicks posted to the top band, which the band's
    /// views report.
    Clicks,
    /// `STELLARATOR_PROBE=drag`: the readings only, and the band without press handlers, so a drag
    /// from outside (CI's mouse) can move the window.
    Drag,
}

/// The probe's mode, from `STELLARATOR_PROBE`.
pub fn probe_mode() -> ProbeMode {
    static MODE: std::sync::OnceLock<ProbeMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("STELLARATOR_PROBE").as_deref() {
        Ok("1") => ProbeMode::Clicks,
        Ok("drag") => ProbeMode::Drag,
        _ => ProbeMode::Off,
    })
}

/// A probe line on standard output (flushed: CI reads it while the app runs).
pub fn probe_line(line: impl std::fmt::Display) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "probe: {line}");
    let _ = out.flush();
}

/// Sets up the main window once it exists: the toolbar on macOS, then the measured radius into
/// [`WindowShape`]; starts the probe when asked. Called from the root while it renders.
pub fn setup_window() {
    let shape = global::<WindowShape>();
    #[cfg(target_os = "macos")]
    {
        let attach = !std::env::var("STELLARATOR_NO_TOOLBAR").is_ok_and(|v| v == "1");
        spawn(async move {
            let chrome = match run_on_main_thread(move || macos::use_toolbar_window(attach)).await {
                Ok(chrome) => chrome,
                Err(e) => {
                    log::warn!("could not set up the window's toolbar: {e}");
                    return;
                }
            };
            // AppKit gives the new corners once the window has laid out the toolbar.
            timer(Duration::from_millis(300)).await;
            let measured = run_on_main_thread(macos::corner_radius_of_main_window).await.ok().flatten();
            let radius = macos::ui_radius(&chrome, measured);
            log::info!("window radius {radius} (toolbar {}, measured {measured:?})", chrome.toolbar);
            shape.update(|s| s.radius = radius);
            if probing() {
                macos::remember(chrome);
            }
        });
    }
    if probing() {
        spawn(async move {
            timer(Duration::from_secs(3)).await;
            let used = shape.get_untracked();
            probe_line(format!("os {} {:?}", os().name(), os_version()));
            probe_line(format!("appearance {}", if dark() { "dark" } else { "light" }));
            probe_line(format!(
                "layout window radius {} (sidebar {})",
                used.radius,
                if used.radius >= crate::theme::shape::FLOAT_FROM { "floating" } else { "flush" }
            ));
            probe_line(format!("window size {:?}", window()));
            #[cfg(target_os = "macos")]
            {
                match run_on_main_thread(macos::report).await {
                    Ok(lines) => lines.into_iter().for_each(probe_line),
                    Err(e) => probe_line(format!("macOS report failed: {e}")),
                }
                if probe_mode() != ProbeMode::Clicks {
                    return;
                }
                timer(Duration::from_secs(7)).await;
                match run_on_main_thread(macos::post_band_clicks).await {
                    Ok(lines) => lines.into_iter().for_each(probe_line),
                    Err(e) => probe_line(format!("posting clicks failed: {e}")),
                }
                timer(Duration::from_secs(2)).await;
                probe_line("clicks done");
            }
        });
    }
}
