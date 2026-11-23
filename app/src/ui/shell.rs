//! The shell (DESIGN.md, "Layout and spacing", "Shape"): the sidebar and the page column.
//!
//! - The sidebar is a panel floating [`shell_gap`] inside the window with [`panel_radius`]
//!   corners, so it is concentric with the window; on windows with corners under 20 pt it fills
//!   the window's left edge with a hairline instead. At its top, the mark and the name in a 32 px
//!   row; then Runs, Prompt sets, Endpoints, Preferences; Settings at the bottom. Rows are 32 px
//!   with 10 px corners, a 16 px icon, an 8 px gap and the label in ink at 500; the selected row is
//!   `sidebar-accent`.
//! - On macOS the title bar is hidden and the traffic lights sit in the sidebar's top band
//!   ([`titlebar_height`], 52 px), which the sidebar's content starts below. That band and the
//!   toolbar's empty areas move the window.
//! - The page column shows the open screen ([`crate::nav::current`]) in its frame
//!   ([`super::page`]).
//! - Below 768 px (a phone, or a narrow browser window), as iOS: no sidebar; the four destinations
//!   in a 56 px capsule floating 12 px from the sides and the bottom, the current one a capsule of
//!   `accent`. Settings is in each top-level screen's bar.
//! - Toasts show over everything ([`super::overlay::toaster`]).

use nsh_base::theme;
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use super::material::{float_shadow, material_bar, material_float};
use super::overlay::toaster;
use super::page::{is_phone, probe_press};
use crate::nav::{self, Destination};
use crate::screens;
use crate::theme::metrics::CONTROL;
use crate::theme::shape::{floating, panel_radius, shell_gap, sidebar_column, titlebar_height};
use crate::theme::text::{label, title};
use crate::theme::{background, ink, muted_ink, radius, token};

/// The destinations in the sidebar, in order; Settings sits apart at the bottom.
const NAV: [(Destination, &str, ImageAsset); 4] = [
    (Destination::Runs, "Runs", icon::PLAY),
    (Destination::PromptSets, "Prompt sets", icon::FILE_TEXT),
    (Destination::Endpoints, "Endpoints", icon::CABLE),
    (Destination::Preferences, "Preferences", icon::GIT_COMPARE_ARROWS),
];

/// The open screen, made again only when the route changes (not when its label does).
fn open_screen() -> impl IntoElement {
    let route = computed(nav::current);
    dynamic(move || screens::view(route.get()))
}

/// The whole window: the sidebar beside the open screen, or on a phone the open screen over the
/// tab bar. Made again when the width crosses 768 px.
pub fn shell() -> impl IntoElement {
    view()
        .flex_1()
        .bg(background)
        .child(
            when(is_phone, || view().flex_1().child(view().flex_1().child(open_screen())).child(tab_bar())).otherwise(|| {
                view()
                    .flex_1()
                    .flex_row()
                    .child(sidebar())
                    .child(view().flex_1().min_w(px(0.0)).child(open_screen()))
            }),
        )
        .child(toaster())
}

/// Phones: the four destinations in a capsule floating over the content.
fn tab_bar() -> impl IntoElement {
    let tabs = NAV.map(|(destination, name, glyph)| {
        let selected = move || nav::destination() == destination;
        view()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .rounded(px(24.0))
            .role(AccessibilityRole::Button)
            .label(name)
            .style_when(selected, |s| s.bg(move || theme().accent))
            .on_press(move || nav::open(destination))
            .child(icon(glyph, 22.0, move || if selected() { ink() } else { muted_ink() }))
            .child(
                label(text(name))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .number_of_lines(1)
                    .text_color(move || if selected() { ink() } else { muted_ink() }),
            )
    });
    // The bar material with the floating edge and shadow (blurred where nsh draws blurs).
    material_float(material_bar(view()))
        .absolute()
        .left(px(12.0))
        .right(px(12.0))
        .bottom(px(12.0))
        .h(px(56.0))
        .p(px(4.0))
        .flex_row()
        .rounded(px(28.0))
        .children(tabs)
}

/// The sidebar column: the gutter around the panel, and on macOS the band over its top that moves
/// the window.
fn sidebar() -> impl IntoElement {
    let rows = NAV.map(|(destination, name, glyph)| sidebar_row(destination, name, glyph));
    let panel = view()
        .flex_1()
        .justify_between()
        .bg(move || theme().sidebar)
        .rounded(move || px(panel_radius()))
        // Floating: `material-float`, a 1 px edge and a soft shadow. Flush: a hairline on the right.
        .border_1()
        .border_color(move || if floating() { token("material-edge")() } else { AdaptiveColor::TRANSPARENT })
        .border_r_color(move || if floating() { token("material-edge")() } else { theme().border })
        .style_when(floating, |s| s.shadow(float_shadow()))
        .px(px(8.0))
        .pt(move || px((titlebar_height() - shell_gap()).max(0.0) + 8.0))
        .pb(px(8.0))
        .child(
            view()
                .gap(px(16.0))
                .child(brand())
                .child(view().gap(px(2.0)).children(rows)),
        )
        .child(sidebar_row(Destination::Settings, "Settings", icon::SETTINGS));
    view()
        .w(move || px(sidebar_column()))
        .flex_none()
        .p(move || px(shell_gap()))
        .child(panel)
        .child(
            // The band over the sidebar's top that moves the window like a title bar, with the
            // traffic lights on it. Nothing interactive sits in it.
            probe_press(
                view()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(move || px(titlebar_height()))
                    .window_drag(true)
                    .test_id("sidebar-band"),
                "sidebar band",
            ),
        )
}

/// The mark, monochrome in ink, and the name.
fn brand() -> impl IntoElement {
    view()
        .h(px(CONTROL))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(6.0))
        .child(icon(icon::MARK, 20.0, ink))
        .child(title(text("Stellarator")).text_size(px(13.0)).line_height(px(18.0)))
}

/// A destination: its icon (muted until selected) and its label.
fn sidebar_row(destination: Destination, name: &'static str, glyph: ImageAsset) -> impl IntoElement {
    let selected = move || nav::destination() == destination;
    view()
        .h(px(CONTROL))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .rounded(px(radius::MD))
        .role(AccessibilityRole::Button)
        .label(name)
        .accessibility_state(move || AccessibilityState { selected: selected(), ..AccessibilityState::default() })
        .style_when(selected, |s| s.bg(move || theme().sidebar_accent))
        .hover(|s| s.bg(move || if selected() { theme().sidebar_accent } else { token("hover")() }))
        .on_press(move || nav::open(destination))
        .child(icon(glyph, 16.0, move || if selected() { ink() } else { muted_ink() }))
        .child(label(text(name)).number_of_lines(1))
}
