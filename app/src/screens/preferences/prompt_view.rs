//! The prompt as sent (frontend/src/components/preferences/prompt-view.tsx): a `muted` card with
//! 16 px corners, one block per message with its role as a data tag when there are several. A long
//! prompt (over 1,400 characters) folds at 288 px to keep the responses in view; below 640 px every
//! prompt folds to about three lines (88 px). "Show all" opens it.

use nsh_base::theme;
use nsh_core::prelude::*;

use super::logic::{is_long_prompt, role_label};
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::{muted_ink, radius, token};
use crate::theme::text::label;
use crate::ui::model_text::{ModelTextStyle, model_text_with};
use crate::ui::page::window_width;

/// Folded height of a long prompt (`max-h-72`).
const LONG_FOLD: f32 = 288.0;
/// Folded height on a phone (`max-h-[5.5rem]`).
const PHONE_FOLD: f32 = 88.0;

/// The prompt's (role, text) messages.
pub fn prompt_view(messages: Vec<(String, String)>) -> impl IntoElement {
    let long = is_long_prompt(&messages);
    let several = messages.len() > 1;
    let open = signal(false);
    // The natural height of the messages, to tell whether a phone's fold hides anything.
    let height = signal(0.0_f32);
    let narrow = move || window_width() < 640.0;
    let fold = move || -> Option<f32> {
        if open.get() {
            None
        } else if narrow() {
            Some(PHONE_FOLD)
        } else if long {
            Some(LONG_FOLD)
        } else {
            None
        }
    };
    let tall = move || narrow() && height.get() > PHONE_FOLD + 1.0;
    let show_toggle = move || long || tall() || (open.get() && narrow());

    let blocks = messages
        .into_iter()
        .map(|(role, body)| {
            view()
                .gap(px(6.0))
                .min_w(px(0.0))
                .child(several.then(|| {
                    text(role_label(&role).to_uppercase())
                        .font_family(ROBOTO_MONO.family)
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .letter_spacing(px(0.48))
                        .text_color(muted_ink)
                }))
                .child(model_text_with(move || body.clone(), None, ModelTextStyle::small()))
                .into_element()
        })
        .collect::<Vec<_>>();

    let content = view()
        .flex_shrink(0.0)
        .gap(px(16.0))
        .px(px(20.0))
        .py(px(16.0))
        .min_w(px(0.0))
        .on_layout(move |b: &Bounds<Points>| height.set(b.size.height.0))
        .children(blocks);
    let body = view()
        .min_w(px(0.0))
        .max_h(move || match fold() {
            Some(h) => px(h),
            None => px(100_000.0),
        })
        .overflow_hidden()
        .child(content);

    let toggle = move || {
        view()
            .px(px(12.0))
            .py(px(4.0))
            .border_t_1()
            .border_color(move || theme().background)
            .child(
                view()
                    .self_start()
                    .min_h(px(32.0))
                    .flex_row()
                    .items_center()
                    .px(px(8.0))
                    .rounded(px(radius::SM))
                    .hover(|s| s.bg(move || theme().accent))
                    .role(AccessibilityRole::Button)
                    .accessibility_state(move || AccessibilityState { expanded: Some(open.get()), ..AccessibilityState::default() })
                    .focusable(true)
                    .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
                    .cursor(Cursor::Pointer)
                    .on_press(move || open.update(|o| *o = !*o))
                    .child(label(text(move || if open.get() { "Show less" } else { "Show all" }))),
            )
    };

    view()
        .rounded(px(radius::XL))
        .bg(move || theme().muted)
        .child(body)
        .child(when(show_toggle, toggle))
}
