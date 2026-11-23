//! Materials (DESIGN.md, "Materials"; frontend/src/index.css): floating chrome is a frosted,
//! tinted material: a strong blur behind a mostly opaque fill with a faint periwinkle tint, mixed
//! in oklab (theme.json's `material-tint` and `material-thick-fill`).
//!
//! | Helper | Fill | Blur | Used on |
//! | --- | --- | --- | --- |
//! | [`material_bar`] | `material-tint` at 89% (93% in dark) | 30 px, saturate 180% | the scrolled toolbar, the phone tab bar, sticky and action bars |
//! | [`material_thick`] | `material-thick-fill` at 90% (93% in dark) | 40 px, saturate 180% | menus, pop-up lists, popovers, tooltips, dialogs, sheets, toasts |
//! | [`material_float`] | none | none | a floating panel's 1 px edge and shadow |
//!
//! nsh draws a backdrop blur on macOS, Windows, iOS, Android and the web, and none on Linux
//! (GTK). There the fill is opaque, as index.css's fallback for browsers without
//! `backdrop-filter`, so text on it keeps its contrast.

use nsh_base::theme;
use nsh_core::platform::Os;
use nsh_core::prelude::*;

use crate::theme::token;

/// Whether nsh draws backdrop blurs where the app runs.
pub fn blur_available() -> bool {
    !matches!(try_os(), Some(Os::Linux) | None)
}

/// `color` at `light` and `dark` alpha, or opaque without a blur.
fn translucent(color: AdaptiveColor, light: f32, dark: f32) -> AdaptiveColor {
    if !blur_available() {
        return color;
    }
    AdaptiveColor { light: color.light.opacity(light), dark: color.dark.opacity(dark) }
}

/// The bar material's fill.
pub fn bar_fill() -> AdaptiveColor {
    translucent(token("material-tint")(), 0.89, 0.93)
}

/// The thick material's fill.
pub fn thick_fill() -> AdaptiveColor {
    translucent(token("material-thick-fill")(), 0.90, 0.93)
}

/// `material-bar`: the fill and its blur.
pub fn material_bar<S: Styled>(s: S) -> S {
    let s = s.bg(bar_fill);
    if blur_available() { s.backdrop_blur(px(30.0)).backdrop_saturate(1.8) } else { s }
}

/// `material-bar` over a state's own tint (theme.json token `tint`) in place of `material-tint`:
/// a bar that belongs to a state, as the preference review's (`review-bar-tint`, periwinkle at
/// 10%). The same strength and blur as [`material_bar`].
pub fn material_bar_tinted<S: Styled>(s: S, tint: &'static str) -> S {
    let s = s.bg(move || translucent(token(tint)(), 0.89, 0.93));
    if blur_available() { s.backdrop_blur(px(30.0)).backdrop_saturate(1.8) } else { s }
}

/// `material-thick`: the fill and its blur.
pub fn material_thick<S: Styled>(s: S) -> S {
    let s = s.bg(thick_fill);
    if blur_available() { s.backdrop_blur(px(40.0)).backdrop_saturate(1.8) } else { s }
}

/// `material-float`'s shadow (`--material-shadow`): 0 1px 2px and 0 12px 32px -8px, darker in dark.
pub fn float_shadow() -> [BoxShadow; 2] {
    let dark = dark();
    let black = |a: f32| nsh_core::color::hsla(0.0, 0.0, 0.0, a).into();
    let (near, far) = if dark { (0.3, 0.6) } else { (0.05, 0.16) };
    [
        BoxShadow { offset_y: px(1.0), blur: px(2.0), color: black(near), ..BoxShadow::default() },
        BoxShadow { offset_y: px(12.0), blur: px(32.0), spread: px(-8.0), color: black(far), ..BoxShadow::default() },
    ]
}

/// `material-float`: a 1 px edge (`material-edge`) and the soft shadow.
pub fn material_float<S: Styled>(s: S) -> S {
    s.border_1().border_color(token("material-edge")).shadow(float_shadow())
}

/// The scrim behind dialogs and sheets: black at 32% (56% in dark).
pub fn scrim() -> AdaptiveColor {
    token("scrim")()
}

/// Inside dialogs, alerts and sheets `muted` becomes the ground (light) or the raised tone (dark),
/// so a card or group stands out from the material.
pub fn dialog_muted() -> AdaptiveColor {
    token("on-dialog-muted")()
}

/// The page ground, for a control on a `muted` card (`on-card-control`).
pub fn on_card_control() -> AdaptiveColor {
    token("on-card-control")()
}

/// The popover text color.
pub fn popover_ink() -> AdaptiveColor {
    theme().popover_foreground
}
