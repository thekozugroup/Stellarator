//! Dialogs, alerts, side sheets and toasts (frontend/src/components/ui/dialog.tsx,
//! alert-dialog.tsx, sheet.tsx, sonner.tsx). All are drawn in the thick material
//! ([`super::material`]) with the material's edge and shadow, over a scrim.
//!
//! | Overlay | Corners | Padding | Width | Notes |
//! | --- | --- | --- | --- | --- |
//! | [`Dialog`] | 26 px | 16 px | 384 px ([`DialogWidth::Sm`]) or 576 px ([`DialogWidth::Xl`]), at most the window less 32 px | holds 10 px cards (26 - 16) |
//! | [`alert`] | 26 px | 16 px | 320 px, 384 px from 640 px | title, description, Cancel and the action |
//! | [`side_sheet`] | the panel radius | 0 | 448 px, the whole window below 640 px | 8 px inside the window from 640 px, as the sidebar |
//! | [`toast_success`] and the others | 14 px | 16 px | 356 px, bottom right, 24 px from the edges | 4 s, then gone |
//!
//! Inside a dialog or sheet `muted` reads as [`super::material::dialog_muted`] (the ground in light,
//! the raised tone in dark): pass `in_dialog` to the form groups and cards in it.

use std::rc::Rc;
use std::time::Duration;

use nsh_base::{SheetEvent, dialog as base_dialog, sheet};
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::button::{Button, Size, Variant, button};
use super::icon::{self, icon};
use super::material::{material_float, material_thick, popover_ink, scrim};
use super::page::{BOTTOM_CHROME, is_phone, window_width};
use crate::theme::shape::{panel_radius, shell_gap};
use crate::theme::text::{body, title};
use crate::theme::{muted_ink, radius, token};

/// A dialog's corner radius (`rounded-3xl`).
pub const DIALOG_RADIUS: f32 = 26.0;
/// A dialog's padding; cards inside have 26 - 16 = 10 px corners.
pub const DIALOG_PADDING: f32 = 16.0;

/// The widths of a dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogWidth {
    /// 320 px (`max-w-xs`).
    Xs,
    /// 384 px (`max-w-sm`), the default.
    #[default]
    Sm,
    /// 576 px (`max-w-xl`): forms.
    Xl,
}

impl DialogWidth {
    fn points(self) -> f32 {
        match self {
            DialogWidth::Xs => 320.0,
            DialogWidth::Sm => 384.0,
            DialogWidth::Xl => 576.0,
        }
    }
}

/// A dialog, shown while `open` holds. Build it with the methods below; its content is made each
/// time it opens.
pub struct Dialog {
    open: Signal<bool>,
    label: SharedString,
    width: DialogWidth,
    close_button: bool,
    on_close: Option<Rc<dyn Fn()>>,
    content: Rc<dyn Fn() -> AnyElement>,
    gap: f32,
}

/// A dialog named `label` (its title, for screen readers) showing `content` while `open` holds.
pub fn dialog(open: Signal<bool>, label: impl Into<SharedString>, content: impl Fn() -> AnyElement + 'static) -> Dialog {
    Dialog {
        open,
        label: label.into(),
        width: DialogWidth::Sm,
        close_button: true,
        on_close: None,
        content: Rc::new(content),
        gap: 16.0,
    }
}

impl Dialog {
    pub fn width(mut self, width: DialogWidth) -> Self {
        self.width = width;
        self
    }

    /// Without the round X at the top right.
    pub fn no_close_button(mut self) -> Self {
        self.close_button = false;
        self
    }

    /// Runs when the user closes it (Escape, the X, a press on the scrim), before `open` is set to
    /// false.
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The space between the header, the body and the footer (16 px; 0 when the content spaces
    /// itself).
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

/// The overlay that holds a dialog's panel: the scrim (a press on it cancels) and the panel
/// centered, at most the window less 16 px on each side.
fn modal(open: Signal<bool>, label: SharedString, cancel: Rc<dyn Fn()>, panel: impl Fn() -> AnyElement + 'static) -> impl IntoElement {
    let panel = Rc::new(panel);
    when(move || open.get(), move || {
        let c1 = cancel.clone();
        let c2 = cancel.clone();
        let panel = panel.clone();
        base_dialog()
            .animation(ModalAnimation::Fade)
            .label(label.clone())
            .on_cancel(move || c1())
            .child(
                view()
                    .absolute()
                    .inset(px(0.0))
                    .items_center()
                    .justify_center()
                    .p(px(16.0))
                    .child(view().absolute().inset(px(0.0)).bg(scrim).on_press(move || c2()))
                    .child(panel()),
            )
    })
}

impl IntoElement for Dialog {
    fn into_element(self) -> AnyElement {
        let open = self.open;
        let on_close = self.on_close.clone();
        let cancel: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(f) = &on_close {
                f();
            }
            open.set(false);
        });
        let width = self.width.points();
        let content = self.content.clone();
        let close_button = self.close_button;
        let gap = self.gap;
        let close = cancel.clone();
        modal(self.open, self.label.clone(), cancel, move || {
            let close = close.clone();
            material_float(material_thick(view()))
                .w_full()
                .max_w(px(width))
                .max_h(move || px((window().height.0 - 32.0).max(200.0)))
                .rounded(px(DIALOG_RADIUS))
                .p(px(DIALOG_PADDING))
                .gap(px(gap))
                .overflow_hidden()
                .child(content())
                .child(close_button.then(|| {
                    view().absolute().top(px(12.0)).right(px(12.0)).child(
                        Button::icon(icon::X, "Close").variant(Variant::Ghost).size(Size::IconSm).round().on_press(move || close()),
                    )
                }))
                .into_element()
        })
        .into_element()
    }
}

/// A dialog's header: the title (15 px at 600) and an optional muted description, 8 px apart.
pub fn dialog_header(heading: impl Into<SharedString>, description: Option<AnyElement>) -> impl IntoElement {
    view()
        .gap(px(8.0))
        .pr(px(32.0))
        .child(title(text(heading.into())).role(AccessibilityRole::Header))
        .child(description.map(|d| view().child(d)))
}

/// A muted description line for [`dialog_header`].
pub fn dialog_description(words: impl Into<SharedString>) -> AnyElement {
    body(text(words.into())).text_color(muted_ink).into_element()
}

/// A dialog's footer: the buttons at the end, in a row; on a phone stacked, the main action on top
/// and each as wide as the dialog. Pass them in reading order (Cancel first).
pub fn dialog_footer(buttons: Vec<Button>) -> impl IntoElement {
    let buttons = Rc::new(std::cell::RefCell::new(Some(buttons)));
    let phone = untracked(|| window_width() < 640.0);
    let list = buttons.borrow_mut().take().unwrap_or_default();
    if phone {
        view()
            .pt(px(4.0))
            .gap(px(8.0))
            .children(list.into_iter().rev().map(|b| b.full_width().into_element()).collect::<Vec<_>>())
            .into_element()
    } else {
        view()
            .pt(px(4.0))
            .flex_row()
            .justify_end()
            .gap(px(8.0))
            .children(list.into_iter().map(IntoElement::into_element).collect::<Vec<_>>())
            .into_element()
    }
}

/// What an alert asks for.
pub struct Alert {
    pub title: SharedString,
    pub description: SharedString,
    /// The action's label ("Delete").
    pub action: SharedString,
    /// The action's label while it runs ("Deleting…").
    pub busy_label: SharedString,
    /// A destructive action (red) or a plain one (primary).
    pub destructive: bool,
}

/// An alert dialog while `open` holds: the title, the description, Cancel and the action. `spec`
/// is read as it opens. While `busy` holds the action shows a spinner and neither button takes
/// presses (nor does Escape or the scrim close it); `error` shows in a red block above the buttons.
pub fn alert(
    open: Signal<bool>,
    spec: impl Fn() -> Alert + 'static,
    busy: impl Fn() -> bool + 'static,
    error: impl Fn() -> Option<String> + 'static,
    on_action: impl Fn() + 'static,
) -> impl IntoElement {
    let busy = Rc::new(busy);
    let error = Rc::new(error);
    let on_action = Rc::new(on_action);
    let spec = Rc::new(spec);
    let b0 = busy.clone();
    let cancel: Rc<dyn Fn()> = Rc::new(move || {
        if !b0() {
            open.set(false);
        }
    });
    let cancel_button = cancel.clone();
    modal(open, SharedString::from("Alert"), cancel, move || {
        let b1 = busy.clone();
        let b2 = busy.clone();
        let b3 = busy.clone();
        let act = on_action.clone();
        let error = error.clone();
        let close = cancel_button.clone();
        let spec = spec();
        let wide = window_width() >= 640.0;
        let cancel = button("Cancel").variant(Variant::Secondary).disabled(move || b1()).on_press(move || close());
        let action = button(spec.action.clone())
            .variant(if spec.destructive { Variant::Destructive } else { Variant::Primary })
            .busy(move || b2(), spec.busy_label.clone())
            .on_press(move || {
                if !b3() {
                    act();
                }
            });
        let header = view()
            .gap(px(6.0))
            .items_start()
            .child(title(text(spec.title.clone())).role(AccessibilityRole::Header).when(!wide, |t| t.text_center()))
            .child(body(text(spec.description.clone())).text_color(muted_ink).when(!wide, |t| t.text_center()));
        let header = if wide { header } else { header.items_center() };
        material_float(material_thick(view()))
            .w_full()
            .max_w(px(if wide { 384.0 } else { 320.0 }))
            .rounded(px(DIALOG_RADIUS))
            .p(px(DIALOG_PADDING))
            .gap(px(16.0))
            .role(AccessibilityRole::AlertDialog)
            .label(spec.title.clone())
            .child(header)
            .child(dynamic(move || error().map(|m| super::blocks::error_block(m, None))))
            .child(dialog_footer(vec![cancel, action]))
            .into_element()
    })
}

/// A side sheet at the window's right edge while `open` holds: 8 px inside the window with the
/// panel radius from 640 px (as the sidebar), the whole height and width below. `label` names it
/// for screen readers.
pub fn side_sheet(open: Signal<bool>, label: impl Into<SharedString>, content: impl Fn() -> AnyElement + 'static) -> impl IntoElement {
    let label = label.into();
    let content = Rc::new(content);
    sheet()
        .side(Placement::Right)
        .open(move || Some(open.get()))
        .on_open_change(move |e: &SheetEvent| {
            if let SheetEvent::OpenChange(o, _) = e {
                open.set(*o);
            }
        })
        .accessibility_label(label)
        .backdrop_style(|s| s.bg(scrim))
        .popup_style(|s| {
            let inset = if window_width() >= 640.0 { shell_gap().max(8.0) } else { 0.0 };
            let corner = if inset > 0.0 { panel_radius().max(radius::XL) } else { 0.0 };
            material_float(material_thick(s))
                .top(px(inset))
                .bottom(px(inset))
                .right(px(inset))
                .w(px((window_width() - 2.0 * inset).min(448.0)))
                .rounded(px(corner))
                .overflow_hidden()
        })
        .child(dynamic(move || content()))
}

/// A sheet's header: the title (15 px at 500) and the muted description, 2 px apart, 16 px padding.
pub fn sheet_header(heading: impl IntoElement, description: Option<AnyElement>) -> impl IntoElement {
    view().gap(px(2.0)).p(px(16.0)).pr(px(48.0)).child(heading).child(description)
}

/// The round X at a sheet's top right (10 px in).
pub fn sheet_close(open: Signal<bool>) -> impl IntoElement {
    view()
        .absolute()
        .top(px(10.0))
        .right(px(10.0))
        .child(Button::icon(icon::X, "Close").variant(Variant::Ghost).size(Size::IconSm).on_press(move || open.set(false)))
}

/// The kind of a toast: its icon and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

/// A toast on the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub message: SharedString,
    /// When it goes.
    until: nsh_core::time::Instant,
}

/// The toasts shown, oldest first.
#[derive(Debug, Clone, Default, PartialEq)]
struct Toasts {
    next: u64,
    shown: Vec<Toast>,
}

/// How long a toast stays (sonner's default).
const TOAST_FOR: Duration = Duration::from_secs(4);

fn show(kind: ToastKind, message: impl Into<SharedString>) {
    let toasts = global::<Toasts>();
    toasts.update(|t| {
        t.next += 1;
        let id = t.next;
        t.shown.push(Toast { id, kind, message: message.into(), until: nsh_core::time::now() + TOAST_FOR });
        // At most three at once, as sonner shows.
        if t.shown.len() > 3 {
            t.shown.remove(0);
        }
    });
}

/// "Deleted gpt-4o." with a green check.
pub fn toast_success(message: impl Into<SharedString>) {
    show(ToastKind::Success, message);
}

/// What failed, with a red mark.
pub fn toast_error(message: impl Into<SharedString>) {
    show(ToastKind::Error, message);
}

/// A plain notice.
pub fn toast_info(message: impl Into<SharedString>) {
    show(ToastKind::Info, message);
}

fn toast_glyph(kind: ToastKind) -> (ImageAsset, fn() -> AdaptiveColor) {
    match kind {
        ToastKind::Success => (icon::CIRCLE_CHECK, || token("positive-ink")()),
        ToastKind::Error => (icon::CIRCLE_ALERT, || token("negative-ink")()),
        ToastKind::Info => (icon::INFO, popover_ink),
    }
}

/// Where toasts show: bottom right, 24 px from the edges; on a phone across the bottom, 16 px from
/// the sides, over the tab bar. Put it once in the shell.
pub fn toaster() -> impl IntoElement {
    let toasts = global::<Toasts>();
    let list = move || toasts.with(|t| t.shown.clone());
    // The toaster lives as long as the window, so its clock removes toasts whatever screen showed
    // them.
    let _ = set_interval(Duration::from_millis(250), move || {
        let now = nsh_core::time::now();
        if toasts.with_untracked(|t| t.shown.iter().any(|x| x.until <= now)) {
            toasts.update(|t| t.shown.retain(|x| x.until > now));
        }
    });
    // The overlay covers the window and lets presses through to what is under it.
    overlay().absolute().inset(px(0.0)).pointer_events(PointerEvents::BoxNone).child(dynamic(move || {
        let phone = is_phone();
        let stack = view()
            .absolute()
            .right(px(if phone { 16.0 } else { 24.0 }))
            .bottom(px(if phone { BOTTOM_CHROME + 8.0 } else { 24.0 }))
            .w(px(if phone { (window_width() - 32.0).max(0.0) } else { 356.0 }))
            .gap(px(8.0))
            .pointer_events(PointerEvents::BoxNone);
        stack.child(each(list, |t: &Toast| t.id, move |t, _| {
            let toast = t.get_untracked();
            let (glyph, color) = toast_glyph(toast.kind);
            material_float(material_thick(view()))
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .p(px(16.0))
                .rounded(px(radius::LG))
                .role(AccessibilityRole::Alert)
                .label(toast.message.clone())
                .on_press(move || toasts.update(|x| x.shown.retain(|y| y.id != toast.id)))
                .child(icon(glyph, 16.0, color))
                .child(body(text(toast.message.clone())).font_weight(FontWeight::MEDIUM).text_color(popover_ink).flex_1())
        }))
    }))
}
