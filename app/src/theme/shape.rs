//! One concentric chain of corners (DESIGN.md, "Shape"): every radius derives from the window's
//! real corner radius.
//!
//! | Window | Radius | Sidebar |
//! | --- | --- | --- |
//! | macOS 26 with the empty unified toolbar (platform/macos.rs) | 26, or what AppKit reports | floats 8 px inside, corners 26 - 8 = 18 |
//! | macOS 26 without the toolbar | 16 | flush |
//! | macOS 14 and 15 | 10 | flush |
//! | Windows 11 | 8 | flush |
//! | Linux | 0 | flush |
//!
//! Below [`FLOAT_FROM`] the sidebar is not a floating panel: it fills the window's left edge with
//! a 1 px border, square-cornered, so nothing in the page touches a window corner.

use nsh_core::element::Os;
use nsh_core::prelude::*;

use super::metrics::{SHELL_GAP, SIDEBAR_PANEL, TOOLBAR_HEIGHT};

/// The window radius from which the sidebar floats inside the window.
pub const FLOAT_FROM: f32 = 20.0;

/// The main window's shape, as the platform reports it ([`crate::platform`] sets it once the
/// window exists; until then, the value the platform is expected to give).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowShape {
    /// The window's corner radius in points.
    pub radius: f32,
    /// The macOS title bar is hidden: the traffic lights sit in the sidebar's top band, which the
    /// layout keeps free ([`titlebar_height`]).
    pub overlay_title_bar: bool,
}

impl Default for WindowShape {
    fn default() -> Self {
        WindowShape::expected(try_os(), try_os().map(|_| os_version()).unwrap_or((0, 0)))
    }
}

impl WindowShape {
    /// The shape `os` at `version` gives the main window before anything is measured.
    pub fn expected(os: Option<Os>, version: (u16, u16)) -> Self {
        match os {
            // The empty unified toolbar gives macOS 26's toolbar-window corners.
            Some(Os::MacOs) if version.0 >= 26 => WindowShape { radius: 26.0, overlay_title_bar: true },
            Some(Os::MacOs) => WindowShape { radius: 10.0, overlay_title_bar: true },
            Some(Os::Windows) => WindowShape { radius: 8.0, overlay_title_bar: false },
            _ => WindowShape { radius: 0.0, overlay_title_bar: false },
        }
    }
}

/// The main window's shape (reactive).
pub fn window_shape() -> WindowShape {
    global::<WindowShape>().get()
}

/// Whether the sidebar floats inside the window (the window radius is [`FLOAT_FROM`] or more).
pub fn floating() -> bool {
    window_shape().radius >= FLOAT_FROM
}

/// How far the sidebar panel sits inside the window: 8, or 0 when flush.
pub fn shell_gap() -> f32 {
    if floating() { SHELL_GAP } else { 0.0 }
}

/// The sidebar panel's corners: the window's minus the gap, so the two are concentric; square
/// when flush.
pub fn panel_radius() -> f32 {
    if floating() { (window_shape().radius - SHELL_GAP).max(0.0) } else { 0.0 }
}

/// The band at the top of the sidebar kept free for the traffic lights (macOS), as tall as the
/// toolbar; 0 elsewhere.
pub fn titlebar_height() -> f32 {
    if window_shape().overlay_title_bar { TOOLBAR_HEIGHT } else { 0.0 }
}

/// The sidebar column: the gap, the panel and the gap.
pub fn sidebar_column() -> f32 {
    SIDEBAR_PANEL + 2.0 * shell_gap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_platform_starts_with_its_window_radius() {
        assert_eq!(WindowShape::expected(Some(Os::MacOs), (26, 1)).radius, 26.0);
        assert_eq!(WindowShape::expected(Some(Os::MacOs), (15, 6)).radius, 10.0);
        assert_eq!(WindowShape::expected(Some(Os::Windows), (10, 0)).radius, 8.0);
        assert_eq!(WindowShape::expected(Some(Os::Linux), (0, 0)).radius, 0.0);
        assert!(WindowShape::expected(Some(Os::MacOs), (26, 0)).overlay_title_bar);
        assert!(!WindowShape::expected(Some(Os::Windows), (10, 0)).overlay_title_bar);
    }
}
