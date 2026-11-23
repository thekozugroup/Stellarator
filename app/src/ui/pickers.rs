//! Pickers drawn by the screen: a pop-up button whose rows and trigger the screen draws, and a
//! combobox for a value the user may also type. They share `select.rs`'s popup (the thick
//! material, 14 px corners) and item rows (32 px, 8 px corners).
//!
//! - [`RichSelect`]: the New run form's Endpoint and Prompt set (a pixel tile, the name, a muted
//!   detail); as wide as its column, the red edge and ring while invalid.
//! - [`TypedCombobox`]: the New run form's Model: a field that filters the listed items as the
//!   user types; the screen offers what was typed as one more item.

use std::rc::Rc;

use nsh_base::{SelectEntry, SelectItem, SelectValue, combobox, select, theme};
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use super::select::{ITEM_CLASS, popup};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::metrics::CONTROL;
use crate::theme::{radius, token};
use crate::theme::text::body;

/// A pop-up button on a card, whose items and trigger are drawn by the screen. `items` is read again when
/// what it reads changes; `shown` is what the trigger says for the value (`None`: the
/// `placeholder`, muted); `render_item` draws one row.
pub struct RichSelect {
    pub items: Rc<dyn Fn() -> Vec<SelectItem>>,
    pub value: Rc<dyn Fn() -> Option<SharedString>>,
    pub on_change: Rc<dyn Fn(SharedString)>,
    pub shown: Rc<dyn Fn() -> Option<AnyElement>>,
    pub render_item: Rc<dyn Fn(&SelectEntry) -> AnyElement>,
    pub placeholder: &'static str,
    pub accessibility: &'static str,
    pub invalid: Rc<dyn Fn() -> bool>,
    pub element: Option<ElementRef>,
}

impl IntoElement for RichSelect {
    fn into_element(self) -> AnyElement {
        let placeholder = self.placeholder;
        let shown = self.shown.clone();
        let content = dynamic(move || -> AnyElement {
            match shown() {
                Some(el) => el,
                None => body(text(placeholder)).text_color(move || theme().muted_foreground).into_element(),
            }
        });
        let (i1, i2) = (self.invalid.clone(), self.invalid.clone());
        let trigger = view()
            .h(px(CONTROL))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .pl(px(12.0))
            .pr(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(move || if i1() { theme().destructive } else { AdaptiveColor::TRANSPARENT })
            .style_when(move || i2(), |s| s.ring(px(3.0)).ring_color(move || theme().destructive.opacity(0.2)))
            // On a card: the ground in light, the raised tone in dark (index.css `--on-card-control`).
            .bg(token("on-card-control"))
            .hover(|s| s.bg(move || theme().accent))
            .cursor(Cursor::Pointer)
            .child(view().flex_1().min_w(px(0.0)).flex_row().items_center().gap(px(8.0)).child(content))
            .child(icon(icon::CHEVRONS_UP_DOWN, 14.0, move || theme().muted_foreground));
        let value = self.value.clone();
        let items = self.items.clone();
        let on_change = self.on_change.clone();
        let render = self.render_item.clone();
        let mut picker = select()
            .items(move || items())
            .value(move || SelectValue::from(value()))
            .on_change(move |next: &Option<SharedString>| {
                if let Some(v) = next {
                    on_change(v.clone());
                }
            })
            .render_item(move |entry: &SelectEntry| render(entry))
            .accessibility_label(self.accessibility)
            .offset(px(4.0))
            .popup_style(popup)
            .item_class(ITEM_CLASS)
            .trigger(trigger)
            .self_stretch();
        if let Some(e) = self.element {
            picker = picker.element_ref(&e);
        }
        picker.into_element()
    }
}

/// A combobox for a value the user may also type: the items are the listed ones; `on_query` hears
/// what is typed so the screen can offer it as one more item. A 32 px field in the look of a
/// secondary button on a card (the web UI's trigger), with the up-and-down chevrons at its end.
pub struct TypedCombobox {
    pub items: Rc<dyn Fn() -> Vec<SelectItem>>,
    pub value: Rc<dyn Fn() -> Vec<SharedString>>,
    pub on_change: Rc<dyn Fn(SharedString)>,
    pub on_query: Rc<dyn Fn(&str)>,
    pub placeholder: &'static str,
    pub accessibility: &'static str,
    /// What the list says while it has no row.
    pub empty: Rc<dyn Fn() -> String>,
    pub mono: bool,
    pub disabled: Rc<dyn Fn() -> bool>,
    pub invalid: Rc<dyn Fn() -> bool>,
    pub element: Option<ElementRef>,
    /// Draws one row from what the item is; without it a row shows the item's title.
    pub render_item: Option<Rc<dyn Fn(&SelectEntry) -> AnyElement>>,
}

impl IntoElement for TypedCombobox {
    fn into_element(self) -> AnyElement {
        let (i1, i2) = (self.invalid.clone(), self.invalid.clone());
        let disabled = self.disabled.clone();
        let items = self.items.clone();
        let value = self.value.clone();
        let on_change = self.on_change.clone();
        let on_query = self.on_query.clone();
        let empty = self.empty.clone();
        let mono = self.mono;
        let render = self.render_item.clone();
        let mut field = combobox()
            .items(move || items())
            .placeholder(self.placeholder)
            .value(move || Some(value()))
            .on_change(move |next: &Vec<SharedString>| {
                if let Some(v) = next.first() {
                    on_change(v.clone());
                }
            })
            .on_query_change(move |query: &SharedString| on_query(query))
            .accessibility_label(self.accessibility)
            .disabled(move || disabled())
            .empty(body(text(move || empty())).px(px(10.0)).py(px(6.0)).text_color(move || theme().muted_foreground))
            .trigger_button(icon(icon::CHEVRONS_UP_DOWN, 14.0, move || theme().muted_foreground))
            .offset(px(4.0))
            .popup_style(popup)
            .item_class(ITEM_CLASS)
            .group_class("px-2.5 py-1.5 text-xs text-muted-foreground")
            .input_class("flex-1 min-w-0 self-stretch bg-transparent text-[13px] text-foreground")
            .input_style(move |s: StyleRefinement| if mono { s.font_family(ROBOTO_MONO.family).text_size(px(12.0)) } else { s })
            .h(px(CONTROL))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .pl(px(12.0))
            .pr(px(8.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(move || if i1() { theme().destructive } else { AdaptiveColor::TRANSPARENT })
            .style_when(move || i2(), |s| s.ring(px(3.0)).ring_color(move || theme().destructive.opacity(0.2)))
            // As the secondary button the web UI's field is: the raised fill (it stays gray on a
            // card, as the web UI's popover trigger does), the accent under the pointer, dimmed
            // while there is no endpoint.
            .bg(move || theme().secondary)
            .hover(|s| s.bg(move || theme().accent))
            .disabled_style(|s| s.opacity(0.5))
            .self_stretch()
            .text_size(px(13.0));
        if let Some(render) = render {
            field = field.render_item(move |entry: &SelectEntry| render(entry));
        }
        if let Some(e) = self.element {
            field = field.element_ref(&e);
        }
        field.into_element()
    }
}
