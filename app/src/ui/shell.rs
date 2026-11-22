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
//! - The page column has a 52 px toolbar (the screen's title, its actions at the end) over the
//!   screen's content, both in a column at most 1280 px wide with 32 px padding.

use nsh_base::theme;
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use crate::screens::{self, Screen};
use crate::theme::metrics::{CONTROL, PAGE_MAX, PAGE_PADDING, TOOLBAR_HEIGHT};
use crate::theme::shape::{floating, panel_radius, shell_gap, sidebar_column, titlebar_height};
use crate::theme::text::{label, title};
use crate::theme::{background, ink, muted_ink, radius, token};

/// The destinations in the sidebar, in order; Settings sits apart at the bottom.
const NAV: [(Screen, &str, ImageAsset); 4] = [
    (Screen::Runs, "Runs", icon::PLAY),
    (Screen::PromptSets, "Prompt sets", icon::FILE_TEXT),
    (Screen::Endpoints, "Endpoints", icon::CABLE),
    (Screen::Preferences, "Preferences", icon::GIT_COMPARE_ARROWS),
];

/// The whole window: the sidebar beside the screen that is open.
pub fn shell() -> impl IntoElement {
    view()
        .flex_1()
        .flex_row()
        .bg(background)
        .child(sidebar())
        .child(view().flex_1().min_w(px(0.0)).child(dynamic(move || screens::view(screens::current()))))
}

/// The sidebar column: the gutter around the panel, and on macOS the band over its top that moves
/// the window.
fn sidebar() -> impl IntoElement {
    let rows = NAV.map(|(screen, name, glyph)| sidebar_row(screen, name, glyph));
    let panel = view()
        .flex_1()
        .justify_between()
        .bg(move || theme().sidebar)
        .rounded(move || px(panel_radius()))
        // Floating: `material-float`, a 1 px edge and a soft shadow. Flush: a hairline on the right.
        .border_1()
        .border_color(move || if floating() { token("material-edge")() } else { AdaptiveColor::TRANSPARENT })
        .border_r_color(move || if floating() { token("material-edge")() } else { theme().border })
        .style_when(floating, |s| s.shadow(material_shadow()))
        .px(px(8.0))
        .pt(move || px((titlebar_height() - shell_gap()).max(0.0) + 8.0))
        .pb(px(8.0))
        .child(
            view()
                .gap(px(16.0))
                .child(brand())
                .child(view().gap(px(2.0)).children(rows)),
        )
        .child(sidebar_row(Screen::Settings, "Settings", icon::SETTINGS));
    view()
        .w(move || px(sidebar_column()))
        .flex_none()
        .p(move || px(shell_gap()))
        .child(panel)
        .child(
            // The band over the sidebar's top that moves the window like a title bar, with the
            // traffic lights on it. Nothing interactive sits in it.
            view()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .h(move || px(titlebar_height()))
                .window_drag(true)
                .test_id("sidebar-band"),
        )
}

/// `material-float`'s shadow (frontend/src/index.css, `--material-shadow` in light): 0 1px 2px at
/// 5%, 0 12px 32px -8px at 16%.
fn material_shadow() -> [BoxShadow; 2] {
    let black = |a: f32| nsh_core::color::hsla(0.0, 0.0, 0.0, a).into();
    [
        BoxShadow { offset_y: px(1.0), blur: px(2.0), color: black(0.05), ..BoxShadow::default() },
        BoxShadow { offset_y: px(12.0), blur: px(32.0), spread: px(-8.0), color: black(0.16), ..BoxShadow::default() },
    ]
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
fn sidebar_row(screen: Screen, name: &'static str, glyph: ImageAsset) -> impl IntoElement {
    let selected = move || screens::current().destination() == screen;
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
        .on_press(move || screens::open(screen))
        .child(icon(glyph, 16.0, move || if selected() { ink() } else { muted_ink() }))
        .child(label(text(name)).number_of_lines(1))
}

/// A screen: the toolbar with its title and actions over its content, which scrolls.
pub fn page(
    screen_title: &'static str,
    actions: Option<AnyElement>,
    content: impl IntoElement,
) -> impl IntoElement {
    view()
        .flex_1()
        .child(
            // The toolbar moves the window wherever it has no control (macOS); the actions in it
            // stay clickable.
            view()
                .h(px(TOOLBAR_HEIGHT))
                .flex_none()
                .flex_row()
                .justify_center()
                .window_drag(true)
                .test_id("toolbar")
                .child(
                    column()
                        .flex_row()
                        .items_center()
                        .gap(px(12.0))
                        .child(title(text(screen_title)).flex_1().number_of_lines(1))
                        .child(view().flex_row().items_center().gap(px(8.0)).child(actions)),
                ),
        )
        .child(
            scroll()
                .flex_1()
                .child(view().flex_row().justify_center().child(column().pt(px(16.0)).pb(px(32.0)).child(content))),
        )
}

/// The page column: at most 1280 px wide, 32 px padding.
fn column() -> View {
    view().flex_1().max_w(px(PAGE_MAX)).px(px(PAGE_PADDING))
}
