//! Icons (DESIGN.md, "Iconography"): lucide glyphs at 16 px with a 1.75 px stroke, shipped as SVG
//! files in assets/icons (made from frontend/node_modules/lucide-react, ISC license in
//! assets/icons/LICENSE.txt) and tinted with a token. The mark (assets/mark.svg) is
//! frontend/src/components/logo.tsx's path.

use nsh_core::element::{ImageAsset, asset};
use nsh_core::prelude::*;

pub const PLAY: ImageAsset = asset!("icons/play.svg");
pub const FILE_TEXT: ImageAsset = asset!("icons/file-text.svg");
pub const CABLE: ImageAsset = asset!("icons/cable.svg");
pub const GIT_COMPARE_ARROWS: ImageAsset = asset!("icons/git-compare-arrows.svg");
pub const SETTINGS: ImageAsset = asset!("icons/settings.svg");
pub const PLUS: ImageAsset = asset!("icons/plus.svg");

/// The Stellarator mark, 20 px.
pub const MARK: ImageAsset = asset!("mark.svg");

/// `glyph` at `size` points in `color`.
pub fn icon(
    glyph: ImageAsset,
    size: f32,
    color: impl IntoReactive<AdaptiveColor>,
) -> impl IntoElement {
    image(glyph).w(px(size)).h(px(size)).flex_none().tint_color(color)
}
