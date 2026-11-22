//! The design system in Rust: the color tokens (theme.json, ported from frontend/src/index.css),
//! the fonts, the type scale and the radii of DESIGN.md.
//!
//! Components never use raw colors: they name a token ([`token`], or the shadcn fields of
//! `nsh_base::theme()`), and each token has a light and a dark value that follow the system's
//! appearance without a render.

// The full scale is here for the screens that follow; this first slice uses part of it.
#[allow(dead_code, reason = "the design system's scale; later screens use the rest")]
pub mod fonts;
pub mod shape;
#[allow(dead_code, reason = "the design system's scale; later screens use the rest")]
pub mod text;

use nsh_base::theme;
use nsh_core::prelude::*;

/// A color token of theme.json by name (`periwinkle`, `positive-ink`, ...), for reactive styles:
/// `.bg(token("periwinkle"))`. A name theme.json does not set is transparent.
pub fn token(name: &'static str) -> impl Fn() -> AdaptiveColor + Copy + 'static {
    move || theme().color(name).unwrap_or(AdaptiveColor::TRANSPARENT)
}

#[allow(dead_code, reason = "the design system's scale; later screens use the rest")]
/// The radius scale of DESIGN.md ("Shape") and frontend/src/index.css (`--radius-*`), in points.
pub mod radius {
    /// Checkboxes.
    pub const XS: f32 = 5.0;
    /// Small buttons, wells inside cards, menu items, tooltips.
    pub const SM: f32 = 8.0;
    /// Buttons, fields, sidebar rows, the table header band and row highlight.
    pub const MD: f32 = 10.0;
    /// Menus, popovers, toasts.
    pub const LG: f32 = 14.0;
    /// Cards and tiles.
    pub const XL: f32 = 16.0;
    /// Dialogs and alerts (the window's on macOS 26).
    pub const XXXL: f32 = 26.0;
}

#[allow(dead_code, reason = "the design system's scale; later screens use the rest")]
/// Fixed sizes of DESIGN.md ("Density", "Layout and spacing"), in points (pointer-fine).
pub mod metrics {
    /// The page toolbar, and on macOS the title bar band the traffic lights are centered in.
    pub const TOOLBAR_HEIGHT: f32 = 52.0;
    /// How far the sidebar panel floats inside the window.
    pub const SHELL_GAP: f32 = 8.0;
    /// The sidebar panel's width.
    pub const SIDEBAR_PANEL: f32 = 224.0;
    /// Sidebar rows, buttons, fields, menu items.
    pub const CONTROL: f32 = 32.0;
    /// Table rows.
    pub const TABLE_ROW: f32 = 44.0;
    /// The table header band.
    pub const TABLE_HEADER: f32 = 32.0;
    /// Icons in navigation.
    pub const ICON: f32 = 16.0;
    /// The page column's padding from 1024 px up.
    pub const PAGE_PADDING: f32 = 32.0;
    /// The page column's widest (80rem).
    pub const PAGE_MAX: f32 = 1280.0;
}

/// The ground of the window: `background`.
pub fn background() -> AdaptiveColor {
    theme().background
}

/// Ink: `foreground`.
pub fn ink() -> AdaptiveColor {
    theme().foreground
}

/// Muted ink: `muted-foreground`.
pub fn muted_ink() -> AdaptiveColor {
    theme().muted_foreground
}
