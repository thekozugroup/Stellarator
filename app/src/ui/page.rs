//! Every screen's frame (frontend/src/components/shared/page.tsx; DESIGN.md, "Layout and
//! spacing"): a toolbar over the screen's content, which scrolls under it.
//!
//! From 768 px:
//! - The toolbar is 52 px. At rest it is the page color; once the content scrolls under it the
//!   bar material and a bottom hairline fade in (200 ms). Its empty areas move the window (macOS).
//! - A top-level screen has its title (15 px at 600) in the toolbar and its actions at the end.
//! - A detail screen (one pushed with `nav::push`) has a breadcrumb in the toolbar ("Runs / Distill
//!   gsm8k": the parents in muted ink at 500, each a link back, the current screen in ink at 600)
//!   and its display title (32 px) at the top of the content, then its meta. Its actions sit at the
//!   end of the toolbar from 1024 px, beside the meta below that.
//!
//! Below 768 px (phones), as iOS: a 44 px bar with the back button ("‹ Runs") or, on a top-level
//! screen, the Settings button at its leading end and `bar_actions` (round icon buttons) at its
//! trailing end; then the 28 px large title, the meta and the actions. Once the large title has
//! scrolled under the bar, a 17 px title fades in at the bar's center.
//!
//! The content column is at most 1280 px wide (1536 with [`Page::wide`]) with 16, 24 (from 640 px)
//! or 32 px (from 1024 px) padding. The scroll position is kept per screen in `nav`: going back
//! lands where the user was.
//!
//! ```ignore
//! page("Endpoints")
//!     .actions(|| button("Add endpoint").leading(icon::PLUS).on_press(add).into_element())
//!     .bar_actions(|| Button::icon(icon::PLUS, "Add endpoint").size(Size::IconRound).into_element())
//!     .child(list)
//! ```

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use super::material::material_bar;
use crate::nav::{self, Destination};
use crate::theme::metrics::{PAGE_MAX, TOOLBAR_HEIGHT};
use crate::theme::text::{display, title};
use crate::theme::{background, ink, muted_ink, radius};

/// The width below which the layout is a phone's (Tailwind's `md`).
pub const PHONE_BELOW: f32 = 768.0;

/// The width from which a detail screen's actions sit in the toolbar (Tailwind's `lg`).
pub const ACTIONS_IN_BAR_FROM: f32 = 1024.0;

/// The phone bar's height.
pub const PHONE_BAR: f32 = 44.0;

/// What the content clears at the bottom on a phone: the tab bar (56 px), its 12 px offset and an
/// 8 px gap (`--bottom-chrome`).
pub const BOTTOM_CHROME: f32 = 76.0;

/// Whether the window is narrower than [`PHONE_BELOW`] (reactive).
pub fn is_phone() -> bool {
    let w = window().width.0;
    w > 0.0 && w < PHONE_BELOW
}

/// The width of the window in points (reactive).
pub fn window_width() -> f32 {
    window().width.0
}

/// The page column's side padding at the window's width: 16, 24 from 640 px, 32 from 1024 px.
pub fn column_padding() -> f32 {
    let w = window_width();
    if w >= 1024.0 {
        32.0
    } else if w >= 640.0 {
        24.0
    } else {
        16.0
    }
}

type Build = Rc<dyn Fn() -> AnyElement>;

/// A screen's frame, built with the methods below and given its content with [`Page::child`].
pub struct Page {
    title: Rc<dyn Fn() -> Option<SharedString>>,
    meta: Option<Build>,
    actions: Option<Build>,
    bar_actions: Option<Build>,
    bottom_bar: Option<Build>,
    wide: bool,
    content: Vec<AnyElement>,
}

/// A screen titled `title`.
pub fn page(title: impl Into<SharedString>) -> Page {
    let title = title.into();
    page_with(move || Some(title.clone()))
}

/// A screen whose title comes from loaded data: `None` shows a placeholder bar while it loads.
pub fn page_with(title: impl Fn() -> Option<SharedString> + 'static) -> Page {
    Page {
        title: Rc::new(title),
        meta: None,
        actions: None,
        bar_actions: None,
        bottom_bar: None,
        wide: false,
        content: Vec::new(),
    }
}

impl Page {
    /// What sits under a detail screen's title (the chips row), and its actions' row.
    pub fn meta(mut self, meta: impl Fn() -> AnyElement + 'static) -> Self {
        self.meta = Some(Rc::new(meta));
        self
    }

    /// The screen's buttons: in the toolbar (see the module's notes), beside the meta otherwise.
    pub fn actions(mut self, actions: impl Fn() -> AnyElement + 'static) -> Self {
        self.actions = Some(Rc::new(actions));
        self
    }

    /// Phones only: round icon buttons at the bar's trailing end ("+", "…").
    pub fn bar_actions(mut self, actions: impl Fn() -> AnyElement + 'static) -> Self {
        self.bar_actions = Some(Rc::new(actions));
        self
    }

    /// The floating action bar at the bottom ([`super::action_bar`]): it stays 8 px over the
    /// window's bottom edge (over the tab bar on phones) while the content scrolls under it.
    pub fn bottom_bar(mut self, bar: impl Fn() -> AnyElement + 'static) -> Self {
        self.bottom_bar = Some(Rc::new(bar));
        self
    }

    /// The wide column (1536 px), for tables with many columns.
    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.content.push(child.into_element());
        self
    }
}

impl IntoElement for Page {
    fn into_element(self) -> AnyElement {
        if untracked(is_phone) { phone_page(self) } else { desktop_page(self) }
    }
}

/// The column: centered, at most 1280 (or 1536) px wide, with the side padding.
fn column(wide: bool) -> View {
    view()
        .w_full()
        .max_w(px(if wide { 1536.0 } else { PAGE_MAX }))
        .px(move || px(column_padding()))
}

/// The scroll view of a page: it starts at the screen's remembered offset and remembers where it
/// is. Content that loads later is waited for: the saved offset is applied once the content is
/// tall enough. `scrolled` hears the offset.
fn page_scroll(scrolled: Signal<f32>) -> Scroll {
    let entry = nav::current_entry();
    let saved = nav::saved_scroll(entry);
    let pending = signal(if saved > 0.0 { Some(saved) } else { None });
    let target = element_ref();
    watch_scroll(
        move || target.node(),
        move |e: &ScrollEvent| {
            if let Some(y) = pending.get_untracked() {
                let room = e.content_size.height.0 - e.size.height.0;
                if room >= y {
                    pending.set(None);
                    target.scroll_to(point(px(0.0), px(y)), false);
                }
                return;
            }
            let y = e.offset.y.0.max(0.0);
            scrolled.set(y);
            nav::remember_scroll(entry, y);
        },
    );
    scroll().flex_1().element_ref(&target)
}

/// From 768 px.
fn desktop_page(p: Page) -> AnyElement {
    let scrolled = signal(0.0_f32);
    let detail = !untracked(nav::current).is_top_level();
    let actions_in_bar = move || !detail || window_width() >= ACTIONS_IN_BAR_FROM;
    let title_fn = p.title.clone();

    // The toolbar: the material fades in once something is under it.
    let toolbar_title: AnyElement = if detail {
        breadcrumb(title_fn.clone()).into_element()
    } else {
        let t = title_fn.clone();
        view()
            .flex_1()
            .min_w(px(0.0))
            .role(AccessibilityRole::Header)
            .child(dynamic(move || match t() {
                Some(s) => title(text(s)).number_of_lines(1).into_element(),
                None => super::blocks::bar(224.0, 16.0).into_element(),
            }))
            .into_element()
    };
    let bar_actions = p.actions.clone();
    let toolbar = probe_press(
        view()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(TOOLBAR_HEIGHT))
            .flex_row()
            .justify_center()
            .window_drag(true)
            .test_id("toolbar"),
        "toolbar band",
    )
    .child(
        material_bar(view())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .border_b_1()
            .border_color(move || theme().border)
            .opacity(move || if scrolled.get() > 0.0 { 1.0_f32 } else { 0.0 })
            .transition_opacity()
            .pointer_events(PointerEvents::None),
    )
    .child(
        column(p.wide)
            .h_full()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(toolbar_title)
            .child(
                view().flex_row().items_center().gap(px(8.0)).child(dynamic(move || {
                    if actions_in_bar() { bar_actions.as_ref().map(|a| a()) } else { None }
                })),
            ),
    );

    // The content: on a detail screen the display title and the meta first.
    let meta = p.meta.clone();
    let content_actions = p.actions.clone();
    let head: Option<AnyElement> = detail.then(|| {
        let t = title_fn.clone();
        view()
            .child(dynamic(move || match t() {
                Some(s) => display(text(s)).role(AccessibilityRole::Header).into_element(),
                None => super::blocks::bar(288.0, 32.0).into_element(),
            }))
            .child(meta_row(meta.clone(), content_actions.clone(), move || !actions_in_bar(), true))
            .into_element()
    });
    let top_meta = (!detail && p.meta.is_some()).then(|| meta_row(p.meta.clone(), None, || false, false));
    let has_bottom_bar = p.bottom_bar.is_some();
    let body = column(p.wide)
        .pt(px(TOOLBAR_HEIGHT + if detail { 24.0 } else { 16.0 }))
        .pb(move || px(if has_bottom_bar { 96.0 } else if window_width() >= 1024.0 { 32.0 } else { 24.0 }))
        .child(head)
        .child(top_meta)
        .children(p.content);

    let bottom = p.bottom_bar.clone().map(|bar| {
        view()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(8.0))
            .flex_row()
            .justify_center()
            .child(column(p.wide).child(dynamic(move || bar())))
    });

    view()
        .flex_1()
        .bg(background)
        .child(page_scroll(scrolled).child(view().flex_row().justify_center().child(body)))
        .child(toolbar)
        .child(bottom)
        .into_element()
}

/// The row under a title: the meta (muted) at its start, the actions at its end; 24 px under it.
fn meta_row(
    meta: Option<Build>,
    actions: Option<Build>,
    show_actions: impl Fn() -> bool + 'static,
    under_title: bool,
) -> impl IntoElement {
    let has_meta = meta.is_some();
    let has_actions = actions.is_some();
    dynamic(move || {
        let actions_now = has_actions && show_actions();
        if !has_meta && !actions_now {
            return view().h(px(if under_title { 24.0 } else { 0.0 })).into_element();
        }
        view()
            .mt(px(if under_title { 12.0 } else { 0.0 }))
            .mb(px(24.0))
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_x(px(24.0))
            .gap_y(px(12.0))
            .child(meta.as_ref().map(|m| view().min_w(px(0.0)).child(m())))
            .child(
                actions_now
                    .then(|| view().flex_row().flex_wrap().items_center().gap(px(8.0)).child(actions.as_ref().map(|a| a()))),
            )
            .into_element()
    })
}

/// "Runs / Distill gsm8k": each parent a link back to it, then the current screen.
fn breadcrumb(current: Rc<dyn Fn() -> Option<SharedString>>) -> impl IntoElement {
    let parents = view().flex_row().items_center().gap(px(6.0)).flex_shrink(1.0).min_w(px(0.0)).child(dynamic(move || {
        nav::parents()
            .into_iter()
            .map(|(depth, name)| {
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .min_w(px(0.0))
                    .flex_shrink(1.0)
                    .child(
                        title(text(name.clone()))
                            .font_weight(FontWeight::MEDIUM)
                            .number_of_lines(1)
                            .text_color(muted_ink)
                            .hover(|s| s.text_color(ink))
                            .role(AccessibilityRole::Link)
                            .label(name.clone())
                            .focusable(true)
                            .cursor(Cursor::Pointer)
                            .on_press(move || nav::back_to(depth)),
                    )
                    .child(title(text("/")).font_weight(FontWeight::NORMAL).text_color(move || muted_ink().opacity(0.6)).accessibility_elements_hidden(true))
                    .into_element()
            })
            .collect::<Vec<_>>()
    }));
    view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .role(AccessibilityRole::Navigation)
        .label("Breadcrumb")
        .child(parents)
        .child(
            view().min_w(px(0.0)).flex_shrink(2.0).child(dynamic(move || match current() {
                Some(s) => title(text(s)).number_of_lines(1).into_element(),
                None => super::blocks::bar(192.0, 16.0).into_element(),
            })),
        )
}

/// Below 768 px.
fn phone_page(p: Page) -> AnyElement {
    let scrolled = signal(0.0_f32);
    let route = untracked(nav::current);
    let detail = !route.is_top_level();
    let title_fn = p.title.clone();
    // The large title (28 px on 31 px, 4 px under the bar) has gone under the bar.
    let title_under = move || scrolled.get() > 4.0 + 31.0;

    let leading: AnyElement = if detail {
        back_control(title_under).into_element()
    } else {
        settings_button().into_element()
    };
    let small_title = {
        let t = title_fn.clone();
        text(move || t().map(|s| s.to_string()).unwrap_or_default())
            .text_size(px(17.0))
            .line_height(px(22.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ink)
            .number_of_lines(1)
            .text_center()
            .opacity(move || if title_under() { 1.0_f32 } else { 0.0 })
            .transition_opacity()
            .accessibility_elements_hidden(true)
    };
    let bar_actions = p.bar_actions.clone();
    let bar = view()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .h(px(PHONE_BAR))
        .child(
            material_bar(view())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .border_b_1()
                .border_color(move || theme().border)
                .opacity(move || if scrolled.get() > 0.0 { 1.0_f32 } else { 0.0 })
                .transition_opacity()
                .pointer_events(PointerEvents::None),
        )
        .child(
            view()
                .h_full()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .px(move || px(column_padding()))
                .child(view().flex_1().basis(px(0.0)).min_w(px(0.0)).flex_row().items_center().child(leading))
                .child(view().max_w(move || px(window_width() * 0.5)).child(small_title))
                .child(
                    view()
                        .flex_1()
                        .basis(px(0.0))
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(dynamic(move || bar_actions.as_ref().map(|a| a()))),
                ),
        );

    let t = title_fn.clone();
    let large_title = dynamic(move || match t() {
        Some(s) => display(text(s))
            .text_size(px(28.0))
            .line_height(px(31.0))
            .letter_spacing(px(-0.84))
            .role(AccessibilityRole::Header)
            .into_element(),
        None => super::blocks::bar(288.0, 28.0).into_element(),
    });
    let has_bottom_bar = p.bottom_bar.is_some();
    let body = column(p.wide)
        .pt(px(PHONE_BAR + 4.0))
        // Clear of the tab bar, and of the action bar over it.
        .pb(px(BOTTOM_CHROME + if has_bottom_bar { 72.0 } else { 0.0 }))
        .child(large_title)
        .child(meta_row(p.meta.clone(), p.actions.clone(), || true, true))
        .children(p.content);

    let bottom = p.bottom_bar.clone().map(|bar| {
        view()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(BOTTOM_CHROME))
            .px(move || px(column_padding()))
            .child(dynamic(move || bar()))
    });

    view()
        .flex_1()
        .bg(background)
        .child(page_scroll(scrolled).child(body))
        .child(bar)
        .child(bottom)
        .into_element()
}

/// "‹ Runs": back to the parent screen, as iOS. The label goes once the small title takes the bar.
fn back_control(label_hidden: impl Fn() -> bool + 'static) -> impl IntoElement {
    let parent = move || nav::parents().last().map(|(_, l)| l.clone()).unwrap_or_default();
    view()
        .h(px(44.0))
        .min_w(px(44.0))
        .ml(px(-8.0))
        .pr(px(8.0))
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .rounded(px(radius::MD))
        .role(AccessibilityRole::Button)
        .label(move || format!("Back to {}", parent()))
        .cursor(Cursor::Pointer)
        .on_press(|| {
            nav::back();
        })
        .child(icon(icon::CHEVRON_LEFT, 24.0, ink))
        .child(
            text(parent)
                .text_size(px(17.0))
                .line_height(px(22.0))
                .text_color(ink)
                .number_of_lines(1)
                .opacity(move || if label_hidden() { 0.0_f32 } else { 1.0 }),
        )
}

/// Phones: Settings at the leading end of each top-level screen's bar, a round secondary button
/// (`accent` while Settings is open).
fn settings_button() -> impl IntoElement {
    let open = move || nav::destination() == Destination::Settings;
    view()
        .w(px(32.0))
        .h(px(32.0))
        .rounded(px(16.0))
        .items_center()
        .justify_center()
        .role(AccessibilityRole::Button)
        .label("Settings")
        .cursor(Cursor::Pointer)
        .bg(move || if open() { theme().accent } else { theme().secondary })
        .on_press(|| nav::open(Destination::Settings))
        .child(icon(icon::SETTINGS, 16.0, ink))
}

/// With the click probe on (`STELLARATOR_PROBE=1`), `band` says on standard output when a press
/// reaches it, so CI can tell whether clicks in the macOS title bar band get to the app's views.
/// Off, it is unchanged (a press handler would keep the band from moving the window).
pub fn probe_press(band: View, what: &'static str) -> View {
    if crate::platform::probe_mode() == crate::platform::ProbeMode::Clicks {
        band.on_press(move || crate::platform::probe_line(format!("press reached nsh: {what}")))
    } else {
        band
    }
}

/// A card (`Panel`): `muted`, 16 px corners, no border or shadow, 16 px padding, with an optional
/// header row (a 15 px title, a muted description, actions). Never put a card inside a card.
pub fn panel(
    heading: Option<SharedString>,
    description: Option<SharedString>,
    actions: Option<AnyElement>,
    content: impl IntoElement,
) -> impl IntoElement {
    let header = (heading.is_some() || actions.is_some()).then(|| {
        view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_x(px(16.0))
            .gap_y(px(8.0))
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(4.0))
            .child(
                view()
                    .min_w(px(0.0))
                    .child(heading.map(|h| title(text(h))))
                    .child(description.map(|d| crate::theme::text::body(text(d)).text_color(muted_ink))),
            )
            .child(actions.map(|a| view().flex_row().flex_wrap().items_center().gap(px(8.0)).child(a)))
    });
    view()
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .child(header)
        .child(view().p(px(16.0)).child(content))
}
