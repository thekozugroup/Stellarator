//! Text fields and forms (frontend/src/components/ui/input.tsx, textarea.tsx, input-group.tsx,
//! label.tsx, shared/field.tsx, shared/form-group.tsx; DESIGN.md, "Components").
//!
//! - [`text_field`]: 32 px, 10 px corners, the ground fill, a 1 px `input` edge (3:1), and on focus
//!   a periwinkle edge with a 3 px ring at 25%. Invalid: a red edge and ring. A trailing control (the
//!   key's eye button) sits inside its end ([`TextField::trailing`]); a search field has the
//!   magnifier at its start ([`TextField::search`]). [`text_area`] is the multiline form.
//! - [`field`]: a label above its control, the hint below, the error in its place.
//! - [`form_group`] (System Settings' grouped form): a `muted` card with 16 px corners and no
//!   border; rows divided by hairlines inset 12 px; the title above the box and a footnote below.
//!   [`form_row`] puts its label on the left and its control on the right (stacked on phones);
//!   [`form_item`] is a row with any content; [`form_disclosure`] is a group that opens and closes.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::element::input;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use super::info_tip::info_tip;
use super::material::dialog_muted;
use super::page::is_phone;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::metrics::CONTROL;
use crate::theme::text::{body, label, meta};
use crate::theme::{muted_ink, radius, token};

/// A text field bound to `value`, built with the methods below.
pub struct TextField {
    value: Signal<String>,
    placeholder: Option<SharedString>,
    masked: Option<Rc<dyn Fn() -> bool>>,
    mono: bool,
    numeric: bool,
    multiline: bool,
    search: bool,
    width: Option<f32>,
    invalid: Option<Rc<dyn Fn() -> bool>>,
    disabled: Option<Rc<dyn Fn() -> bool>>,
    on_change: Option<Rc<dyn Fn(&str)>>,
    on_submit: Option<Rc<dyn Fn()>>,
    trailing: Option<AnyElement>,
    accessibility: Option<SharedString>,
    element: Option<ElementRef>,
    background: Option<fn() -> AdaptiveColor>,
}

/// A one-line text field showing and editing `value`.
pub fn text_field(value: Signal<String>) -> TextField {
    TextField {
        value,
        placeholder: None,
        masked: None,
        mono: false,
        numeric: false,
        multiline: false,
        search: false,
        width: None,
        invalid: None,
        disabled: None,
        on_change: None,
        on_submit: None,
        trailing: None,
        accessibility: None,
        element: None,
        background: None,
    }
}

/// A multiline text field (64 px at least).
pub fn text_area(value: Signal<String>) -> TextField {
    TextField { multiline: true, ..text_field(value) }
}

impl TextField {
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Hides the text while `masked` holds (API keys, tokens).
    pub fn masked(mut self, masked: impl Fn() -> bool + 'static) -> Self {
        self.masked = Some(Rc::new(masked));
        self
    }

    /// Roboto Mono at 12 px (URLs, keys, header names).
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// A number: the numeric keyboard, tabular figures, right-aligned.
    pub fn numeric(mut self) -> Self {
        self.numeric = true;
        self
    }

    /// The magnifier at the start.
    pub fn search(mut self) -> Self {
        self.search = true;
        self
    }

    /// A fixed width in points (otherwise it fills its column).
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// The red edge and ring while `invalid` holds.
    pub fn invalid(mut self, invalid: impl Fn() -> bool + 'static) -> Self {
        self.invalid = Some(Rc::new(invalid));
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Rc::new(disabled));
        self
    }

    /// Runs after each edit, with the new text (the value is already set).
    pub fn on_change(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Return in a one-line field.
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// A control inside the field's end (an icon button, 28 px).
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_element());
        self
    }

    /// The accessible name when no visible label names the field.
    pub fn accessibility_label(mut self, text: impl Into<SharedString>) -> Self {
        self.accessibility = Some(text.into());
        self
    }

    /// The field's element, to focus it (`element.focus()`).
    pub fn element(mut self, element: &ElementRef) -> Self {
        self.element = Some(*element);
        self
    }

    /// The fill (the ground by default).
    pub fn background(mut self, color: fn() -> AdaptiveColor) -> Self {
        self.background = Some(color);
        self
    }
}

impl IntoElement for TextField {
    fn into_element(self) -> AnyElement {
        let value = self.value;
        let invalid = self.invalid.clone();
        let is_invalid = move || invalid.as_ref().is_some_and(|f| f());
        let i1 = is_invalid.clone();
        let i2 = is_invalid.clone();
        let disabled = self.disabled.clone();
        let is_disabled = move || disabled.as_ref().is_some_and(|f| f());
        let on_change = self.on_change.clone();
        let bg = self.background.unwrap_or(|| theme().background);
        // Mono fields (URLs, keys) stay 12 px at every width, as the web UI's on a pointer.
        let mono = self.mono;
        let mut field = input()
            .value(move || value.get())
            .on_change(move |text: &str| {
                value.set(text.to_owned());
                if let Some(f) = &on_change {
                    f(text);
                }
            })
            .multiline(self.multiline)
            .min_h(px(if self.multiline { 64.0 } else { CONTROL }))
            .h(px(if self.multiline { 96.0 } else { CONTROL }))
            .px(px(12.0))
            .py(px(if self.multiline { 8.0 } else { 0.0 }))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(move || if i1() { theme().destructive } else { theme().input })
            .bg(bg)
            // 13 px, and 16 px below 768 px (the web UI's `text-base md:text-sm`).
            .text_size(move || px(if mono { 12.0 } else if is_phone() { 16.0 } else { 13.0 }))
            .line_height(move || px(if mono { 16.0 } else if is_phone() { 20.0 } else { 18.0 }))
            .text_color(move || theme().foreground)
            .placeholder_color(move || resolved(theme().muted_foreground))
            .caret_color(move || resolved(token("periwinkle")()))
            .focus(move |s| {
                s.border_color(move || if i2() { theme().destructive } else { token("ring")() })
                    .ring(px(3.0))
                    .ring_color(move || if is_invalid() { theme().destructive.opacity(0.2) } else { token("ring")().opacity(0.25) })
            })
            .disabled(is_disabled)
            .disabled_style(|s| s.bg(move || theme().muted).opacity(0.6))
            .auto_correct(false);
        if let Some(p) = self.placeholder {
            field = field.placeholder(p.to_string());
        }
        if let Some(m) = self.masked {
            field = field.masked(move || m());
        }
        if self.mono {
            field = field.font_family(ROBOTO_MONO.family).letter_spacing(px(0.0));
        }
        if self.numeric {
            field = field.keyboard_type(KeyboardType::Numeric).text_right().tabular_nums();
        }
        if self.search {
            field = field.pl(px(32.0));
        }
        if self.trailing.is_some() {
            field = field.pr(px(40.0));
        }
        if let Some(a) = self.accessibility {
            field = field.label(a);
        }
        if let Some(e) = self.element {
            field = field.element_ref(&e);
        }
        if let Some(f) = self.on_submit {
            field = field.on_submit(move |_: &str| f());
        }
        let field = match self.width {
            Some(w) => field.w(px(w)),
            None => field.self_stretch(),
        };
        if self.trailing.is_none() && !self.search {
            return field.into_element();
        }
        let mut wrap = view().min_w(px(0.0)).child(field);
        wrap = match self.width {
            Some(w) => wrap.w(px(w)),
            None => wrap.self_stretch(),
        };
        if self.search {
            wrap = wrap.child(
                view()
                    .absolute()
                    .left(px(10.0))
                    .top(px(8.0))
                    .child(icon(icon::SEARCH, 16.0, move || theme().muted_foreground)),
            );
        }
        if let Some(t) = self.trailing {
            wrap = wrap.child(view().absolute().right(px(2.0)).top(px(2.0)).child(t));
        }
        wrap.into_element()
    }
}

/// A field's label: 13 px at 500, "(optional)" muted after it.
pub fn field_label(text_label: impl Into<SharedString>, optional: bool) -> impl IntoElement {
    label(text(text_label.into()))
        .number_of_lines(2)
        .child(optional.then(|| text(" (optional)").font_weight(FontWeight::NORMAL).text_color(muted_ink)))
}

/// A hint under a control: 12 px muted.
pub fn hint(text_hint: impl Into<SharedString>) -> impl IntoElement {
    meta(text(text_hint.into()))
}

/// An error under a control: 13 px in the red ink, read out when it appears.
pub fn error_text(message: impl Into<SharedString>) -> impl IntoElement {
    body(text(message.into())).text_color(token("negative-ink")).role(AccessibilityRole::Alert)
}

/// A hint that may come and go (`None` hides it).
type HintFn = Rc<dyn Fn() -> Option<SharedString>>;

/// The hint, or the error in its place while `error` has one.
fn note(hint_text: Option<HintFn>, error: Option<Signal<Option<String>>>, end: bool) -> impl IntoElement {
    dynamic(move || -> Option<AnyElement> {
        let el = match error.and_then(|e| e.get()) {
            Some(message) if !message.is_empty() => error_text(message).into_element(),
            _ => {
                let h = hint_text.as_ref().and_then(|h| h())?;
                if end { meta(text(h)).text_right().into_element() } else { hint(h).into_element() }
            }
        };
        Some(el)
    })
}

/// A label above its control, the hint below, the error under the control in place of the hint.
pub struct Field {
    label: SharedString,
    control: AnyElement,
    hint: Option<HintFn>,
    error: Option<Signal<Option<String>>>,
    info: Option<SharedString>,
    optional: bool,
}

pub fn field(text_label: impl Into<SharedString>, control: impl IntoElement) -> Field {
    Field { label: text_label.into(), control: control.into_element(), hint: None, error: None, info: None, optional: false }
}

impl Field {
    pub fn hint(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.hint = Some(Rc::new(move || Some(text.clone())));
        self
    }

    /// A hint shown while `hint` gives one.
    pub fn hint_when(mut self, hint: impl Fn() -> Option<SharedString> + 'static) -> Self {
        self.hint = Some(Rc::new(hint));
        self
    }

    /// Shown under the control while it holds a message.
    pub fn error(mut self, error: Signal<Option<String>>) -> Self {
        self.error = Some(error);
        self
    }

    /// An (i) button after the label that explains the field.
    pub fn info(mut self, text: impl Into<SharedString>) -> Self {
        self.info = Some(text.into());
        self
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

impl IntoElement for Field {
    fn into_element(self) -> AnyElement {
        let label_row = view()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(field_label(self.label.clone(), self.optional))
            .child(self.info.map(|i| info_tip(self.label.clone(), i)));
        view().gap(px(6.0)).child(label_row).child(self.control).child(note(self.hint, self.error, false)).into_element()
    }
}

/// The fill of a form group: `muted`, or inside a dialog or sheet the ground (light) or the raised
/// tone (dark).
fn group_fill(in_dialog: bool) -> fn() -> AdaptiveColor {
    if in_dialog { dialog_muted } else { || theme().muted }
}

/// A grouped form: the title above the box, the rows inside divided by inset hairlines, the
/// footnote below.
pub struct FormGroup {
    title: Option<SharedString>,
    footnote: Option<AnyElement>,
    rows: Vec<AnyElement>,
    in_dialog: bool,
}

pub fn form_group(rows: Vec<AnyElement>) -> FormGroup {
    FormGroup { title: None, footnote: None, rows, in_dialog: false }
}

impl FormGroup {
    /// 13 px at 600, muted, above the box.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 12 px muted text below the box (any element: a sentence with a link).
    pub fn footnote(mut self, footnote: impl IntoElement) -> Self {
        self.footnote = Some(footnote.into_element());
        self
    }

    /// Inside a dialog or sheet: the box takes the ground (light) or the raised tone (dark).
    pub fn in_dialog(mut self) -> Self {
        self.in_dialog = true;
        self
    }
}

/// A hairline 12 px in from each side of a group, above every row but the first.
fn inset_hairline() -> impl IntoElement {
    view().absolute().top(px(0.0)).left(px(12.0)).right(px(12.0)).h(px(1.0)).bg(move || theme().border)
}

/// The rows of a group, each but the first with the hairline above it.
fn separated(rows: Vec<AnyElement>) -> Vec<AnyElement> {
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| view().child(row).child((i > 0).then(inset_hairline)).into_element())
        .collect()
}

impl IntoElement for FormGroup {
    fn into_element(self) -> AnyElement {
        view()
            .gap(px(6.0))
            .min_w(px(0.0))
            .child(self.title.map(|t| {
                body(text(t)).px(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(muted_ink).role(AccessibilityRole::Header)
            }))
            .child(view().rounded(px(radius::XL)).bg(group_fill(self.in_dialog)).children(separated(self.rows)))
            .child(self.footnote.map(|f| view().px(px(12.0)).child(f)))
            .into_element()
    }
}

/// A form row: label left, control right (stacked on phones). Build with the methods below.
pub struct FormRow {
    label: SharedString,
    control: AnyElement,
    optional: Option<Rc<dyn Fn() -> bool>>,
    info: Option<SharedString>,
    hint: Option<HintFn>,
    error: Option<Signal<Option<String>>>,
    end: bool,
    stacked: bool,
}

pub fn form_row(text_label: impl Into<SharedString>, control: impl IntoElement) -> FormRow {
    FormRow {
        label: text_label.into(),
        control: control.into_element(),
        optional: None,
        info: None,
        hint: None,
        error: None,
        end: false,
        stacked: false,
    }
}

impl FormRow {
    /// "(optional)" after the label.
    pub fn optional(mut self) -> Self {
        self.optional = Some(Rc::new(|| true));
        self
    }

    /// "(optional)" after the label while `optional` holds (the API key, unless a preset needs one).
    pub fn optional_when(mut self, optional: impl Fn() -> bool + 'static) -> Self {
        self.optional = Some(Rc::new(optional));
        self
    }

    pub fn info(mut self, text: impl Into<SharedString>) -> Self {
        self.info = Some(text.into());
        self
    }

    pub fn hint(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.hint = Some(Rc::new(move || Some(text.clone())));
        self
    }

    /// A hint shown while `hint` gives one (the Docker note under a localhost URL).
    pub fn hint_when(mut self, hint: impl Fn() -> Option<SharedString> + 'static) -> Self {
        self.hint = Some(Rc::new(hint));
        self
    }

    pub fn error(mut self, error: Signal<Option<String>>) -> Self {
        self.error = Some(error);
        self
    }

    /// A compact control (a stepper, a segmented control) at the trailing edge of its column.
    pub fn align_end(mut self) -> Self {
        self.end = true;
        self
    }

    /// The label above the control at every width (text areas).
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }
}

impl IntoElement for FormRow {
    fn into_element(self) -> AnyElement {
        let side_by_side = !self.stacked && !untracked(is_phone);
        let label_row = view()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .min_h(px(if side_by_side { CONTROL } else { 0.0 }))
            .child(
                body(text(self.label.clone())).number_of_lines(2).child(self.optional.map(|optional| {
                    text(move || if optional() { " (optional)" } else { "" }).text_color(muted_ink)
                })),
            )
            .child(self.info.map(|i| info_tip(self.label.clone(), i)));
        let end = self.end && side_by_side;
        let mut control_col = view().gap(px(6.0)).min_w(px(0.0)).child(self.control).child(note(self.hint, self.error, end));
        if self.end && side_by_side {
            control_col = control_col.items_end();
        }
        let row = view().px(px(12.0)).py(px(8.0)).gap(px(6.0));
        if side_by_side {
            row.flex_row()
                .items_start()
                .gap(px(16.0))
                .child(label_row.w(px(192.0)).flex_none())
                .child(control_col.flex_1())
                .into_element()
        } else {
            row.child(label_row).child(control_col).into_element()
        }
    }
}

/// A row with any content (a line of buttons, a fact without a label): 12 px sides, 8 px top and
/// bottom.
pub fn form_item(content: impl IntoElement) -> View {
    view().px(px(12.0)).py(px(8.0)).child(content)
}

/// A group that opens and closes, for the settings most runs never touch: the header row is the
/// trigger (a chevron, the title, and while closed a summary at the end); the rows inside keep the
/// group's hairlines.
pub fn form_disclosure(
    title: impl Into<SharedString>,
    summary: impl Fn() -> String + 'static,
    open: Signal<bool>,
    rows: impl Fn() -> Vec<AnyElement> + 'static,
    in_dialog: bool,
) -> impl IntoElement {
    let rows = Rc::new(rows);
    let title = title.into();
    let header = view()
        .min_h(px(36.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(radius::XL))
        .role(AccessibilityRole::Button)
        .label(title.clone())
        .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(move || open.update(|o| *o = !*o))
        .child(
            icon(icon::CHEVRON_RIGHT, 16.0, move || theme().muted_foreground)
                .rotate(move || if open.get() { 90.0_f32.to_radians() } else { 0.0 }),
        )
        .child(body(text(title)).font_weight(FontWeight::SEMIBOLD))
        .child(view().flex_1())
        .child(dynamic(move || (!open.get()).then(|| body(text(summary())).number_of_lines(1).text_color(muted_ink))));
    view()
        .rounded(px(radius::XL))
        .bg(group_fill(in_dialog))
        .child(header)
        .child(when(move || open.get(), move || {
            // Made again when what the rows read changes (a header added or removed).
            let rows = rows.clone();
            view().child(inset_hairline()).child(dynamic(move || view().children(separated(rows()))))
        }))
}

/// `color` in the appearance shown (for props that take one color, not a light and dark pair).
fn resolved(color: AdaptiveColor) -> nsh_core::color::Hsla {
    if dark() { color.dark } else { color.light }
}
