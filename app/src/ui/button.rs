//! Buttons (frontend/src/components/ui/button.tsx; DESIGN.md, "Components"): rounded rectangles
//! with 10 px corners, 32 px tall, 13 px labels at 500. Small sizes are 28 px (and 24 px) with
//! 8 px corners.
//!
//! - [`Variant::Primary`]: the ink fill with inverse text, at most one per view.
//! - [`Variant::Secondary`]: the raised tone (`secondary`) with ink text: toolbar actions and most
//!   buttons. On a `muted` card call [`Button::on_card`] (the ground in light, the raised tone in
//!   dark).
//! - [`Variant::Outline`]: transparent with a field-strength edge (`input`).
//! - [`Variant::Ghost`]: no fill until hovered: row actions.
//! - [`Variant::Destructive`]: red ink on the red tint.
//! - [`Variant::Link`]: underlined ink text.
//!
//! ```ignore
//! button("New run").leading(icon::PLUS).on_press(|| nav::push(Route::NewRun, None))
//! button("Cancel").variant(Variant::Secondary)
//! Button::icon(icon::ELLIPSIS, "More actions").variant(Variant::Ghost).size(Size::IconSm)
//! ```

use std::rc::Rc;

use nsh_base::{button as base_button, theme};
use nsh_core::element::ImageAsset;
use nsh_core::prelude::*;

use super::icon::{self, icon};
use crate::theme::{radius, token};

/// The look of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

/// The size of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    /// 32 px, 12 px padding.
    #[default]
    Default,
    /// 24 px, 8 px corners, 12 px text.
    Xs,
    /// 28 px, 8 px corners, 12 px text.
    Sm,
    /// 36 px.
    Lg,
    /// 32 px square.
    Icon,
    /// 24 px square, 8 px corners.
    IconXs,
    /// 28 px square, 8 px corners.
    IconSm,
    /// 36 px square.
    IconLg,
    /// The phone bar's 44 pt circle.
    IconRound,
}

impl Size {
    /// Height, horizontal padding, corner radius, text size and icon size, in points.
    fn metrics(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Size::Default => (32.0, 12.0, radius::MD, 13.0, 16.0),
            Size::Xs => (24.0, 8.0, radius::SM, 12.0, 12.0),
            Size::Sm => (28.0, 10.0, radius::SM, 12.0, 14.0),
            Size::Lg => (36.0, 14.0, radius::MD, 13.0, 16.0),
            Size::Icon => (32.0, 0.0, radius::MD, 13.0, 16.0),
            Size::IconXs => (24.0, 0.0, radius::SM, 12.0, 12.0),
            Size::IconSm => (28.0, 0.0, radius::SM, 12.0, 16.0),
            Size::IconLg => (36.0, 0.0, radius::MD, 13.0, 16.0),
            Size::IconRound => (44.0, 0.0, 22.0, 13.0, 20.0),
        }
    }

    fn square(self) -> bool {
        matches!(self, Size::Icon | Size::IconXs | Size::IconSm | Size::IconLg | Size::IconRound)
    }
}

/// A button, built with the methods below.
pub struct Button {
    label: Option<SharedString>,
    accessibility: Option<SharedString>,
    variant: Variant,
    size: Size,
    leading: Option<ImageAsset>,
    trailing: Option<ImageAsset>,
    disabled: Option<Rc<dyn Fn() -> bool>>,
    busy: Option<(Rc<dyn Fn() -> bool>, SharedString)>,
    on_press: Option<Rc<dyn Fn()>>,
    on_card: bool,
    full_width: bool,
    round: bool,
    muted: bool,
    tooltip: Option<SharedString>,
}

/// A text button (primary until [`Button::variant`] says otherwise).
pub fn button(label: impl Into<SharedString>) -> Button {
    Button {
        label: Some(label.into()),
        accessibility: None,
        variant: Variant::Primary,
        size: Size::Default,
        leading: None,
        trailing: None,
        disabled: None,
        busy: None,
        on_press: None,
        on_card: false,
        full_width: false,
        round: false,
        muted: false,
        tooltip: None,
    }
}

impl Button {
    /// An icon button: `glyph` alone, named `accessibility` for screen readers and its tooltip.
    pub fn icon(glyph: ImageAsset, accessibility: impl Into<SharedString>) -> Button {
        let accessibility = accessibility.into();
        Button {
            label: None,
            accessibility: Some(accessibility.clone()),
            variant: Variant::Ghost,
            size: Size::Icon,
            leading: Some(glyph),
            tooltip: Some(accessibility),
            ..button("")
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// An icon before the label.
    pub fn leading(mut self, glyph: ImageAsset) -> Self {
        self.leading = Some(glyph);
        self
    }

    /// An icon after the label (a chevron, an external link).
    pub fn trailing(mut self, glyph: ImageAsset) -> Self {
        self.trailing = Some(glyph);
        self
    }

    /// Dimmed and taking no presses while `disabled` holds.
    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Rc::new(disabled));
        self
    }

    /// While `busy` holds: a spinner icon and `label` ("Saving…"), and no presses.
    pub fn busy(mut self, busy: impl Fn() -> bool + 'static, label: impl Into<SharedString>) -> Self {
        self.busy = Some((Rc::new(busy), label.into()));
        self
    }

    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        self.on_press = Some(Rc::new(f));
        self
    }

    /// A secondary button on a `muted` card: the ground in light, the raised tone in dark.
    pub fn on_card(mut self) -> Self {
        self.on_card = true;
        self
    }

    /// As wide as its parent (stacked dialog footers on phones).
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Muted ink for the icon and label (the copy button, quiet row actions).
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    /// Fully rounded corners (a dialog's close button, the phone bar's buttons).
    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }

    /// The accessible name, when the label is not enough.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility = Some(label.into());
        self
    }

    /// A tooltip after the hover delay (icon buttons have their name as one).
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

/// The fill of `variant` at rest and under the pointer.
fn fills(variant: Variant, on_card: bool) -> (fn() -> AdaptiveColor, fn() -> AdaptiveColor) {
    match variant {
        Variant::Primary => (|| theme().primary, || theme().primary.opacity(0.88)),
        Variant::Secondary if on_card => (|| token("on-card-control")(), || theme().accent),
        Variant::Secondary => (|| theme().secondary, || token("secondary-hover")()),
        Variant::Outline => (|| AdaptiveColor::TRANSPARENT, || theme().muted),
        Variant::Ghost => (|| AdaptiveColor::TRANSPARENT, || theme().accent),
        Variant::Destructive => (|| token("negative-tint")(), || token("destructive-hover")()),
        Variant::Link => (|| AdaptiveColor::TRANSPARENT, || AdaptiveColor::TRANSPARENT),
    }
}

/// The text and icon color of `variant`.
fn ink(variant: Variant) -> fn() -> AdaptiveColor {
    match variant {
        Variant::Primary => || theme().primary_foreground,
        Variant::Destructive => || token("negative-ink")(),
        _ => || theme().foreground,
    }
}

impl IntoElement for Button {
    fn into_element(self) -> AnyElement {
        let (height, pad, corner, text_size, icon_size) = self.size.metrics();
        let (rest, hover) = fills(self.variant, self.on_card);
        let ink = if self.muted { || theme().muted_foreground } else { ink(self.variant) };
        let disabled = self.disabled.clone();
        let busy = self.busy.clone();
        let inactive = move || disabled.as_ref().is_some_and(|d| d()) || busy.as_ref().is_some_and(|(b, _)| b());
        let on_press = self.on_press.clone();
        let busy_now = self.busy.clone();
        let label = self.label.clone();
        let mut b = base_button()
            .disabled(inactive.clone())
            .on_press(move || {
                if let Some(f) = &on_press {
                    f();
                }
            })
            .h(px(height))
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(if matches!(self.size, Size::Xs | Size::Sm) { 4.0 } else { 6.0 }))
            .rounded(px(if self.round { height / 2.0 } else { corner }))
            .bg(rest)
            .hover(move |s| s.bg(hover))
            .active(|s| s.scale(0.98))
            .disabled_style(|s| s.opacity(0.5))
            .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")).ring_offset(px(2.0)))
            .cursor(Cursor::Pointer);
        b = if self.size.square() {
            b.w(px(height)).flex_none()
        } else {
            let left = if self.leading.is_some() { pad - 2.0 } else { pad };
            let right = if self.trailing.is_some() { pad - 2.0 } else { pad };
            b.pl(px(left)).pr(px(right))
        };
        if self.full_width {
            b = b.self_stretch();
        }
        if self.variant == Variant::Outline {
            b = b.border_1().border_color(move || theme().input);
        }
        if let Some(a) = self.accessibility.clone() {
            b = b.accessibility_label(Some(a));
        }
        let glyph = self.leading;
        let leading = move || -> Option<AnyElement> {
            if busy_now.as_ref().is_some_and(|(b, _)| b()) {
                Some(icon(icon::LOADER, icon_size, ink).into_element())
            } else {
                glyph.map(|g| icon(g, icon_size, ink).into_element())
            }
        };
        let busy_label = self.busy.clone();
        let text_el = label.filter(|l| !l.is_empty()).map(|l| {
            let mut t = text(move || match &busy_label {
                Some((b, busy)) if b() => busy.clone(),
                _ => l.clone(),
            })
            .text_size(px(text_size))
            .line_height(px(if text_size >= 13.0 { 18.0 } else { 16.0 }))
            .font_weight(FontWeight::MEDIUM)
            .letter_spacing(px(-0.003 * text_size))
            .number_of_lines(1)
            .text_color(ink);
            if self.variant == Variant::Link {
                t = t.underline();
            }
            t
        });
        let b = b.child(dynamic(leading)).child(text_el).child(self.trailing.map(|g| icon(g, icon_size, ink)));
        // nsh's button takes no tooltip of its own: a view around it holds it.
        match self.tooltip.clone() {
            Some(tip) => {
                let wrap = view().flex_none().tooltip(super::tooltip::tooltip_bubble(tip));
                let wrap = if self.full_width { wrap.self_stretch() } else { wrap };
                wrap.child(b).into_element()
            }
            None => b.into_element(),
        }
    }
}

/// An inline link: 13 px underlined ink, `trailing` after it in muted ink (the external-link
/// glyph). For a link out of the app use [`external_link`].
pub fn text_link(label: impl Into<SharedString>, trailing: Option<ImageAsset>, on_press: impl Fn() + 'static) -> impl IntoElement {
    let label = label.into();
    view()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .self_start()
        .min_h(px(24.0))
        .rounded(px(radius::XS))
        .role(AccessibilityRole::Link)
        .label(label.clone())
        .focusable(true)
        .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
        .cursor(Cursor::Pointer)
        .on_press(on_press)
        .child(crate::theme::text::body(text(label)).underline())
        .child(trailing.map(|g| icon(g, 14.0, move || theme().muted_foreground)))
}

/// A link that opens `url` in the browser, with the external-link glyph.
pub fn external_link(label: impl Into<SharedString>, url: impl Into<String>) -> impl IntoElement {
    let url = url.into();
    text_link(label, Some(icon::EXTERNAL_LINK), move || {
        let _ = open_url(url.clone());
    })
}
