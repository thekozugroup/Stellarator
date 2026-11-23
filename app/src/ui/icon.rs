//! Icons (DESIGN.md, "Iconography"): lucide glyphs at 16 px with a 1.75 px stroke, shipped as SVG
//! files in assets/icons and tinted with a token. tools/lucide-svg.mjs writes them from
//! frontend/node_modules/lucide-react (ISC license in assets/icons/LICENSE.txt):
//!
//!     node app/tools/lucide-svg.mjs frontend/node_modules/lucide-react/dist/esm/icons app/assets/icons <names…>
//!
//! Then add a constant below. The mark (assets/mark.svg) is frontend/src/components/logo.tsx's path.

use nsh_core::element::{Image, ImageAsset, asset};
use nsh_core::prelude::*;

pub const PLAY: ImageAsset = asset!("icons/play.svg");
pub const FILE_TEXT: ImageAsset = asset!("icons/file-text.svg");
pub const CABLE: ImageAsset = asset!("icons/cable.svg");
pub const GIT_COMPARE_ARROWS: ImageAsset = asset!("icons/git-compare-arrows.svg");
pub const SETTINGS: ImageAsset = asset!("icons/settings.svg");
pub const PLUS: ImageAsset = asset!("icons/plus.svg");
pub const MINUS: ImageAsset = asset!("icons/minus.svg");
pub const X: ImageAsset = asset!("icons/x.svg");
pub const CHECK: ImageAsset = asset!("icons/check.svg");
pub const CHEVRON_RIGHT: ImageAsset = asset!("icons/chevron-right.svg");
pub const CHEVRON_LEFT: ImageAsset = asset!("icons/chevron-left.svg");
pub const CHEVRON_DOWN: ImageAsset = asset!("icons/chevron-down.svg");
pub const CHEVRONS_UP_DOWN: ImageAsset = asset!("icons/chevrons-up-down.svg");
pub const ELLIPSIS: ImageAsset = asset!("icons/ellipsis.svg");
pub const PENCIL: ImageAsset = asset!("icons/pencil.svg");
pub const REFRESH: ImageAsset = asset!("icons/refresh-cw.svg");
pub const TRASH: ImageAsset = asset!("icons/trash.svg");
pub const EYE: ImageAsset = asset!("icons/eye.svg");
pub const EYE_OFF: ImageAsset = asset!("icons/eye-off.svg");
pub const EXTERNAL_LINK: ImageAsset = asset!("icons/external-link.svg");
pub const SEARCH: ImageAsset = asset!("icons/search.svg");
pub const INFO: ImageAsset = asset!("icons/info.svg");
pub const COPY: ImageAsset = asset!("icons/copy.svg");
pub const RETRY: ImageAsset = asset!("icons/rotate-cw.svg");
pub const LOADER: ImageAsset = asset!("icons/loader-circle.svg");
pub const CIRCLE_CHECK: ImageAsset = asset!("icons/circle-check.svg");
pub const CIRCLE_ALERT: ImageAsset = asset!("icons/circle-exclamation-point.svg");
pub const DOWNLOAD: ImageAsset = asset!("icons/download.svg");
pub const PAUSE: ImageAsset = asset!("icons/pause.svg");
pub const ROTATE_CCW: ImageAsset = asset!("icons/rotate-ccw.svg");
pub const MINUS: ImageAsset = asset!("icons/minus.svg");

/// The app's icon (frontend/public/icon-192.png), for About.
pub const APP_ICON: ImageAsset = asset!("icon-192.png");

/// The Stellarator mark, 20 px.
pub const MARK: ImageAsset = asset!("mark.svg");

/// `glyph` at `size` points in `color`.
pub fn icon(glyph: ImageAsset, size: f32, color: impl IntoReactive<AdaptiveColor>) -> Image {
    image(glyph).w(px(size)).h(px(size)).flex_none().tint_color(color)
}
