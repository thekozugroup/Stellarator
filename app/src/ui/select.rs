//! Pop-up buttons, menus and the combobox (frontend/src/components/ui/select.tsx,
//! dropdown-menu.tsx, command.tsx), on nsh's `select`, `menu` and `combobox`.
//!
//! - [`select_button`]: a pop-up button: a secondary button (raised tone, 10 px corners) with
//!   up-and-down chevrons at its end, 32 px. Its list is in the thick material with 14 px corners
//!   and 6 px padding; items are 32 px with 8 px corners, the highlighted one `accent`.
//! - [`menu_button`]: a "…" or any trigger opening a menu of [`MenuEntry`]s: items with an icon,
//!   destructive items in red ink, separators.
//! - [`combobox_field`]: a field that filters a list as the user types.

use std::rc::Rc;

use nsh_base::{SelectItem, combobox, menu, menu_item, menu_separator, select, theme};
use nsh_core::element::{ImageAsset, StyleRefinement};
use nsh_core::prelude::*;

use super::button::{Button, Size, Variant};
use super::icon::{self, icon};
use super::material::{material_float, material_thick};
use crate::theme::metrics::CONTROL;
use crate::theme::text::{body, meta};
use crate::theme::{radius, token};

/// The classes of a list item: 32 px, 8 px corners, the highlight `accent`.
const ITEM_CLASS: &str = "min-h-8 rounded-sm px-2.5 py-1.5 text-[13px] text-popover-foreground focus:bg-accent selected:font-medium disabled:opacity-50";

/// The popup of a list or menu: the thick material, 14 px corners, 6 px padding, at least 128 px.
fn popup(s: StyleRefinement) -> StyleRefinement {
    material_float(material_thick(s.rounded(px(radius::LG)).p(px(6.0)).min_w(px(128.0))))
}

/// An option of a pop-up button: its value and its title.
pub fn option(value: impl Into<SharedString>, title: impl Into<SharedString>) -> SelectItem {
    SelectItem::new(value.into(), title.into())
}

/// A pop-up button choosing one of `options` into `value`. `title` is what the button says for a
/// value (the option's title); `placeholder` while there is none.
pub fn select_button(
    options: Vec<SelectItem>,
    value: Signal<Option<SharedString>>,
    title: impl Fn(&str) -> String + 'static,
    placeholder: &'static str,
    accessibility: &'static str,
) -> impl IntoElement {
    let trigger = view()
        .h(px(CONTROL))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .pl(px(12.0))
        .pr(px(8.0))
        .rounded(px(radius::MD))
        .bg(move || theme().secondary)
        .hover(|s| s.bg(move || theme().accent))
        .cursor(Cursor::Pointer)
        .child(
            body(text(move || value.get().map(|v| title(&v)).unwrap_or_else(|| placeholder.to_owned())))
                .number_of_lines(1)
                .text_color(move || if value.get().is_some() { theme().foreground } else { theme().muted_foreground }),
        )
        .child(icon(icon::CHEVRONS_UP_DOWN, 14.0, move || theme().muted_foreground));
    select()
        .items(options)
        .value(move || nsh_base::SelectValue::from(value.get()))
        .on_change(move |next: &Option<SharedString>| value.set(next.clone()))
        .accessibility_label(accessibility)
        .offset(px(4.0))
        .popup_style(popup)
        .item_class(ITEM_CLASS)
        .trigger(trigger)
        // As wide as its title (`w-fit`), not its column.
        .self_start()
}

/// An entry of a menu.
pub enum MenuEntry {
    /// An action: its icon, label, whether it is destructive, and what it does.
    Item { glyph: Option<ImageAsset>, label: SharedString, destructive: bool, disabled: Option<Rc<dyn Fn() -> bool>>, on_select: Rc<dyn Fn()> },
    /// A full-width hairline.
    Separator,
    /// A muted 12 px heading.
    Label(SharedString),
}

/// A menu item.
pub fn menu_entry(glyph: Option<ImageAsset>, label: impl Into<SharedString>, on_select: impl Fn() + 'static) -> MenuEntry {
    MenuEntry::Item { glyph, label: label.into(), destructive: false, disabled: None, on_select: Rc::new(on_select) }
}

/// A destructive menu item (red ink).
pub fn menu_destructive(glyph: Option<ImageAsset>, label: impl Into<SharedString>, on_select: impl Fn() + 'static) -> MenuEntry {
    MenuEntry::Item { glyph, label: label.into(), destructive: true, disabled: None, on_select: Rc::new(on_select) }
}

impl MenuEntry {
    /// Dims the item and ignores it while `disabled` holds.
    pub fn disabled(self, disabled: impl Fn() -> bool + 'static) -> Self {
        match self {
            MenuEntry::Item { glyph, label, destructive, on_select, .. } => {
                MenuEntry::Item { glyph, label, destructive, disabled: Some(Rc::new(disabled)), on_select }
            }
            other => other,
        }
    }
}

fn entry(e: &MenuEntry) -> AnyElement {
    match e {
        MenuEntry::Item { glyph, label, destructive, disabled, on_select } => {
            let destructive = *destructive;
            let ink = move || if destructive { token("negative-ink")() } else { theme().popover_foreground };
            let f = on_select.clone();
            let mut item = menu_item()
                .on_select(move || f())
                .min_h(px(CONTROL))
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(radius::SM))
                .focus(move |s| s.bg(move || if destructive { theme().destructive.opacity(0.1) } else { theme().accent }))
                .hover(move |s| s.bg(move || if destructive { theme().destructive.opacity(0.1) } else { theme().accent }))
                .disabled_style(|s| s.opacity(0.5))
                .child(glyph.map(|g| icon(g, 16.0, ink)))
                .child(body(text(label.clone())).number_of_lines(1).text_color(ink));
            if let Some(d) = disabled.clone() {
                item = item.disabled(move || d());
            }
            item.into_element()
        }
        MenuEntry::Separator => menu_separator().h(px(1.0)).my(px(6.0)).mx(px(-6.0)).bg(move || theme().border).into_element(),
        MenuEntry::Label(l) => meta(text(l.clone())).px(px(10.0)).py(px(4.0)).font_weight(FontWeight::MEDIUM).into_element(),
    }
}

/// A menu opened by `trigger`, aligned with its end; `width` sets its width (the web UI's `w-44`
/// is 176), `None` sizes it to its items (128 px at least). The trigger must take presses itself
/// (a button without a tooltip, or a view): the menu puts its press handler on it.
pub fn menu_with(trigger: impl IntoElement, entries: Vec<MenuEntry>, width: Option<f32>) -> impl IntoElement {
    let entries = Rc::new(entries);
    menu()
        .offset(px(4.0))
        .align(Align::End)
        .popup_style(move |s| {
            let s = popup(s);
            match width {
                Some(w) => s.w(px(w)),
                None => s,
            }
        })
        .trigger(trigger)
        .child(dynamic(move || entries.iter().map(entry).collect::<Vec<_>>()))
}

/// The "…" button of a row or toolbar and its menu (`width` as [`menu_with`]).
pub fn menu_button(accessibility: impl Into<SharedString>, entries: Vec<MenuEntry>, width: Option<f32>) -> impl IntoElement {
    menu_with(
        Button::icon(icon::ELLIPSIS, accessibility).variant(Variant::Ghost).size(Size::IconSm).without_tooltip(),
        entries,
        width,
    )
}

/// A field that filters `options` as the user types and sets `value` to the one picked.
pub fn combobox_field(
    options: Vec<SelectItem>,
    value: Signal<Option<SharedString>>,
    placeholder: &'static str,
    accessibility: &'static str,
) -> impl IntoElement {
    combobox()
        .items(options)
        .placeholder(placeholder)
        .value(move || value.get().map(|v| vec![v]))
        .on_change(move |next: &Vec<SharedString>| value.set(next.first().cloned()))
        .accessibility_label(accessibility)
        .offset(px(4.0))
        .popup_style(popup)
        .item_class(ITEM_CLASS)
        .h(px(CONTROL))
        .px(px(12.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(move || theme().input)
        .bg(move || theme().background)
        .text_size(px(13.0))
}
