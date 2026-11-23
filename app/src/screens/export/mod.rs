//! The export dialogs' one layout (frontend/src/components/export/export-dialog-shell.tsx): the
//! dialog's own options on the left; the result on the right (the record count as the figure, what
//! is skipped, the first record exactly as written); the Hugging Face push inline below; Cancel and
//! Download at the end. Below 1024 px the first record comes before the options and the count sits
//! above the buttons, so the effect of each option stays in view. An 896 px dialog.
//!
//! The shell owns the preview: it reads `POST {base}/preview` when the dialog opens and again after
//! each change of the options (after `debounce`), keeps the last answer on screen at 60% while the
//! next one comes, and drops answers that a newer request has overtaken. It also owns the download
//! (`GET {base}?{options}`: the Downloads folder on the desktops, the browser's download on the
//! web) and the push (`POST {base}/hub` with `{options, target}`).
//!
//! # Using it
//!
//! Keep the options in a signal made outside the dialog, so they survive closing it, as the web
//! UI's dialogs keep their state. Describe the export with an [`ExportSpec`] and give
//! [`export_dialog`] the option controls, built with the pieces here: [`choice_group`] (radio rows
//! with a hint), [`option_group`] with [`check_option`] rows, and [`file_type_group`]. In labels and
//! hints, text between backticks is set in mono ("Inline `<think>` tags"); [`placement_label`] has
//! the thinking-trace words. A run's export (`run.rs`) is the worked example:
//!
//! ```ignore
//! let options = signal(PrefExportOptions { .. });
//! let spec = ExportSpec {
//!     title: "Export pairs",
//!     base: format!("/api/preference-sets/{id}/export"),
//!     options: Rc::new(move || serde_json::to_value(options.get()).unwrap_or_default()),
//!     file_type: Rc::new(move || options.with(|o| o.file_type)),
//!     debounce: Duration::ZERO,
//!     empty_text: "No pairs yet.",
//!     repo_example: "your-name/math-dpo-pairs",
//! };
//! export_dialog(open, spec, move || {
//!     vec![
//!         choice_group("Format", FORMATS.to_vec(), move || options.with(|o| o.format), move |v| options.update(|o| o.format = v)),
//!         option_group("Prompts", vec![check_option("Include system prompt", None, get, set)]),
//!         file_type_group(move || options.with(|o| o.file_type), move |v| options.update(|o| o.file_type = v)),
//!     ]
//! })
//! ```

pub mod run;

use std::rc::Rc;
use std::time::Duration;

use nsh_base::{checkbox as base_checkbox, radio, radio_group as base_radio_group, theme};
use nsh_core::prelude::*;
use serde_json::Value;

use crate::api::types::{ExportPreviewOut, FileType, HubPushIn, ReasoningPlacement};
use crate::api::{ApiError, Settings};
use crate::data::{SignalExt, call};
use crate::format::{count, int};
use crate::nav::{self, Destination};
use crate::theme::fonts::{INTER_DISPLAY, ROBOTO_MONO};
use crate::theme::text::{body, mono, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::{bar, error_block};
use crate::ui::button::{Variant, button};
use crate::ui::choice::segmented_with_get;
use crate::ui::field::{form_group, form_row, text_field};
use crate::ui::icon::{self, icon};
use crate::ui::material::dialog_muted;
use crate::ui::overlay::{DialogWidth, dialog, dialog_footer};
use crate::ui::page::window_width;

/// The width from which the preview sits beside the options (Tailwind's `lg`).
const TWO_COLUMNS_FROM: f32 = 1024.0;

/// What an export dialog exports, and how.
pub struct ExportSpec {
    /// The dialog's title ("Export dataset").
    pub title: &'static str,
    /// The export's API path without a suffix: `/api/runs/4/export`. The preview is
    /// `{base}/preview`, the push `{base}/hub`, the download `{base}?{options}`.
    pub base: String,
    /// The options (reactive) as the JSON object the API takes.
    pub options: Rc<dyn Fn() -> Value>,
    /// The file type chosen (reactive), for the Download button's label.
    pub file_type: Rc<dyn Fn() -> FileType>,
    /// How long the options stay unchanged before the preview is read again (250 ms for a run, as
    /// the web UI; zero reads it at once).
    pub debounce: Duration,
    /// In place of the first record when nothing matches.
    pub empty_text: &'static str,
    /// The repository the push field shows as its placeholder and in its error.
    pub repo_example: &'static str,
}

/// The preview as the dialog shows it: the last answer, the last error, and whether a newer one is
/// on its way (or waiting out the debounce).
#[derive(Debug, Clone, Default, PartialEq)]
struct Preview {
    data: Option<ExportPreviewOut>,
    error: Option<String>,
    stale: bool,
}

/// The export dialog, shown while `open` holds. `options` makes the dialog's own controls (the
/// left column); it is called each time the dialog opens.
pub fn export_dialog(open: Signal<bool>, spec: ExportSpec, options: impl Fn() -> Vec<AnyElement> + 'static) -> impl IntoElement {
    let spec = Rc::new(spec);
    let preview = signal(Preview::default());
    // The newest request; an answer to an older one is dropped.
    let latest = signal(0_u64);
    // A push on its way: the dialog cannot be closed (Cancel, Escape, the X, the scrim) until its
    // answer comes, so the answer finds the push form and a second push cannot start.
    let pushing = signal(false);

    let fetch: Rc<dyn Fn(Value)> = {
        let base = spec.base.clone();
        Rc::new(move |body: Value| {
            let id = latest.get_untracked() + 1;
            latest.set(id);
            preview.update(|p| p.stale = true);
            let base = base.clone();
            // The dialog's host may go before the answer comes (the screen around it made again):
            // its signals are written with `try_set`.
            call(
                move |api| async move { api.export_preview(&base, &body).await },
                move |result| {
                    if latest.try_get() != Some(id) {
                        return;
                    }
                    preview.try_update(|p| {
                        p.stale = false;
                        match result {
                            Ok(data) => {
                                p.data = Some(data);
                                p.error = None;
                            }
                            Err(e) => p.error = Some(e.message),
                        }
                    });
                },
            );
        })
    };

    // Read when the dialog opens, then after each change of the options while it is open.
    let was_open = signal(false);
    {
        let spec = spec.clone();
        let fetch = fetch.clone();
        effect(move || {
            let body = (spec.options)();
            let is_open = open.get();
            let opening = is_open && !was_open.get_untracked();
            was_open.set(is_open);
            if !is_open {
                return;
            }
            if opening || spec.debounce.is_zero() {
                fetch(body);
                return;
            }
            // The options changed: the shown preview is stale until the new one comes.
            let id = latest.get_untracked() + 1;
            latest.set(id);
            preview.update(|p| p.stale = true);
            let fetch = fetch.clone();
            let _ = set_timeout(spec.debounce, move || {
                if latest.get_untracked() == id {
                    fetch(body.clone());
                }
            });
        });
    }

    let options = Rc::new(options);
    dialog(open, spec.title, move || shell(open, spec.clone(), preview, pushing, fetch.clone(), options()).into_element())
        .width(DialogWidth::FourXl)
        .gap(0.0)
        .busy(move || pushing.get())
}

/// The dialog's content.
fn shell(
    open: Signal<bool>,
    spec: Rc<ExportSpec>,
    preview: Signal<Preview>,
    pushing: Signal<bool>,
    fetch: Rc<dyn Fn(Value)>,
    options: Vec<AnyElement>,
) -> impl IntoElement {
    let two_columns = window_width() >= TWO_COLUMNS_FROM;
    let retry: Rc<dyn Fn()> = {
        let spec = spec.clone();
        Rc::new(move || fetch(untracked(|| (spec.options)())))
    };
    let result = preview_column(spec.clone(), preview, retry, two_columns);
    let options_column = view().min_w(px(0.0)).gap(px(24.0)).children(options);
    // From 1024 px the preview stays at the top of the scroll area while the options scroll past it,
    // until the options end (the web UI's `lg:sticky lg:top-0`): nsh has no sticky position, so it
    // moves down by the scroll offset, held inside the options' height.
    let scroll_ref = element_ref();
    let scrolled = signal(0.0_f32);
    let options_h = signal(0.0_f32);
    let preview_h = signal(0.0_f32);
    watch_scroll(move || scroll_ref.node(), move |e: &ScrollEvent| scrolled.set(e.offset.y.0.max(0.0)));
    let columns = if two_columns {
        view()
            .flex_row()
            .items_start()
            .gap(px(32.0))
            .child(options_column.flex_1().basis(px(0.0)).on_layout(move |b: &Bounds<Points>| options_h.set(b.size.height.0)))
            .child(
                view()
                    .flex_1()
                    .basis(px(0.0))
                    .min_w(px(0.0))
                    .on_layout(move |b: &Bounds<Points>| preview_h.set(b.size.height.0))
                    .translate_y(move || px(sticky_offset(scrolled.get(), options_h.get(), preview_h.get())))
                    .child(result),
            )
            .into_element()
    } else {
        view().gap(px(32.0)).child(result).child(options_column).into_element()
    };
    let nothing = move || preview.with(|p| p.data.as_ref().is_some_and(|d| d.count == 0));
    let scroll_area = scroll().flex_shrink(1.0).min_h(px(0.0)).mx(px(-16.0)).element_ref(&scroll_ref).child(
        view()
            .px(px(16.0))
            .pb(px(4.0))
            .child(columns)
            .child(view().mt(px(32.0)).child(hub_push(open, spec.clone(), nothing, pushing))),
    );

    // Below 1024 px the count sits above the buttons, since the right-hand column is gone.
    let phone_count = (!two_columns).then(|| {
        dynamic(move || {
            preview.with(|p| p.data.clone()).map(|d| {
                let skipped = skipped_text(&d.skipped);
                body(text(count(d.count, "record", "records")).font_weight(FontWeight::MEDIUM))
                    .tabular_nums()
                    .child(skipped.map(|s| text(format!(". {s}")).font_weight(FontWeight::NORMAL).text_color(muted_ink)))
            })
        })
    });

    let file_type = spec.file_type.clone();
    let download_label = move || format!("Download {}", file_type_label(file_type()));
    let download = {
        let spec = spec.clone();
        dynamic(move || {
            let data = preview.with(|p| p.data.clone());
            let label = download_label();
            let ready = data.as_ref().filter(|d| d.count > 0).cloned();
            let mut b = button(label).leading(icon::DOWNLOAD);
            match ready {
                Some(d) => {
                    let spec = spec.clone();
                    let name = d.filename.clone();
                    b = b.tooltip(d.filename.clone()).on_press(move || {
                        let path = format!("{}{}", spec.base, query_string(&untracked(|| (spec.options)())));
                        save(path, name.clone());
                    });
                }
                None => b = b.disabled(|| true),
            }
            footer_buttons(open, pushing, b)
        })
    };

    view()
        .min_h(px(0.0))
        .flex_shrink(1.0)
        .gap(px(16.0))
        .child(title(text(spec.title)).role(AccessibilityRole::Header).pr(px(32.0)))
        .child(scroll_area)
        .child(phone_count)
        .child(download)
}

/// How far the preview moves down to stay at the top of a scroll area scrolled by `scrolled`: no
/// further than the end of the options beside it.
fn sticky_offset(scrolled: f32, options_h: f32, preview_h: f32) -> f32 {
    scrolled.min((options_h - preview_h).max(0.0)).max(0.0)
}

/// Cancel and the download button, as the dialog's footer (its top padding taken back: the web
/// UI's `pt-0`). Cancel waits for a push on its way.
fn footer_buttons(open: Signal<bool>, pushing: Signal<bool>, download: crate::ui::button::Button) -> AnyElement {
    let cancel = button("Cancel").variant(Variant::Secondary).disabled(move || pushing.get()).on_press(move || open.set(false));
    view().mt(px(-4.0)).child(dialog_footer(vec![cancel, download])).into_element()
}

/// Saves the export at `path`: in the Downloads folder on the desktops (then shows it in the file
/// manager, as the Tauri app did), through the browser's download on the web.
fn save(path: String, name: String) {
    #[cfg(not(target_arch = "wasm32"))]
    call(
        move |api| async move { api.download(&path, &name).await },
        |result| match result {
            Ok(dest) => {
                let _ = reveal_path(dest);
            }
            Err(e) => crate::ui::overlay::toast_error(e.message),
        },
    );
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        spawn(async move {
            let origin = crate::web::page_origin().await;
            let _ = open_url(format!("{origin}{path}"));
        });
    }
}

/// The right-hand column (above the options below 1024 px): the count, what is skipped, the first
/// record; its loading and error states.
fn preview_column(spec: Rc<ExportSpec>, preview: Signal<Preview>, retry: Rc<dyn Fn()>, two_columns: bool) -> impl IntoElement {
    let empty_text = spec.empty_text;
    dynamic(move || -> AnyElement {
        let (data, error, stale) = preview.with(|p| (p.data.clone(), p.error.clone(), p.stale));
        let Some(data) = data else {
            if let Some(message) = error {
                return error_block(format!("Couldn’t build the preview. {message}"), Some(retry.clone())).into_element();
            }
            return view()
                .gap(px(12.0))
                .role(AccessibilityRole::Progressbar)
                .label("Loading preview")
                .child(two_columns.then(|| bar(160.0, 32.0)))
                .child(
                    view()
                        .h(px(if two_columns { 256.0 } else { 160.0 }))
                        .rounded(px(radius::XL))
                        .bg(move || theme().secondary),
                )
                .into_element();
        };
        let skipped = skipped_text(&data.skipped);
        let figure = two_columns.then(|| {
            view()
                .px(px(4.0))
                .child(
                    view()
                        .flex_row()
                        .items_baseline()
                        .gap(px(6.0))
                        .child(
                            text(int(data.count))
                                .font_family(INTER_DISPLAY.family)
                                .text_size(px(28.0))
                                .line_height(px(31.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .letter_spacing(px(-0.84))
                                .tabular_nums()
                                .text_color(move || theme().foreground),
                        )
                        .child(body(text(if data.count == 1 { "record" } else { "records" })).text_color(muted_ink)),
                )
                .child(skipped.clone().map(|s| body(text(s)).text_color(muted_ink).tabular_nums().mt(px(4.0))))
        });
        let record: AnyElement = match &data.example {
            Some(example) => {
                let json = serde_json::to_string_pretty(example).unwrap_or_default();
                view()
                    .gap(px(6.0))
                    .child(group_title("First record"))
                    .child(
                        scroll()
                            .max_h(px(if two_columns { 384.0 } else { 192.0 }))
                            .rounded(px(radius::XL))
                            .bg(dialog_muted)
                            .focusable(true)
                            .label("First record, as JSON")
                            .child(
                                mono(text(json))
                                    .line_height(px(19.5))
                                    .selectable(true)
                                    .px(px(16.0))
                                    .py(px(12.0)),
                            ),
                    )
                    .into_element()
            }
            None => body(text(empty_text))
                .text_color(muted_ink)
                .px(px(16.0))
                .py(px(24.0))
                .rounded(px(radius::XL))
                .bg(dialog_muted)
                .into_element(),
        };
        view()
            .gap(px(12.0))
            .min_w(px(0.0))
            .opacity(if stale { 0.6_f32 } else { 1.0 })
            .child(figure)
            .child(error.map(|m| error_block(format!("Couldn’t refresh the preview. {m}"), Some(retry.clone()))))
            .child(record)
            .into_element()
    })
}

/// A note above a dialog's options (what the records hold): 13 px muted, with mono between
/// backticks, 12 px in.
pub fn options_note(words: &'static str) -> AnyElement {
    rich(words, 13.0).text_size(px(13.0)).line_height(px(18.0)).text_color(muted_ink).px(px(12.0)).into_element()
}

/// A group's title: 13 px at 600 in muted ink, 12 px in from the box's edge.
fn group_title(words: &'static str) -> Text {
    body(text(words)).font_weight(FontWeight::SEMIBOLD).text_color(muted_ink).px(px(12.0))
}

/// A hairline 12 px in from each side, above every row of a box but the first.
fn row_separator(index: usize) -> Option<AnyElement> {
    (index > 0).then(|| view().absolute().top(px(0.0)).left(px(12.0)).right(px(12.0)).h(px(1.0)).bg(move || theme().border).into_element())
}

/// The box of a group: `muted` (the ground in light inside the dialog), 16 px corners.
fn group_box() -> View {
    view().rounded(px(radius::XL)).bg(dialog_muted)
}

/// Options under a title in one box (System Settings' grouped rows): [`check_option`] rows.
pub fn option_group(legend: &'static str, rows: Vec<AnyElement>) -> AnyElement {
    let rows: Vec<AnyElement> = rows.into_iter().enumerate().map(|(i, r)| view().child(r).child(row_separator(i)).into_element()).collect();
    view()
        .gap(px(6.0))
        .min_w(px(0.0))
        .role(AccessibilityRole::Group)
        .label(legend)
        .child(group_title(legend))
        .child(group_box().children(rows))
        .into_element()
}

/// One radio choice: its value, its label, and an optional one-line hint. Text between backticks is
/// set in mono.
#[derive(Debug, Clone, Copy)]
pub struct Choice<T> {
    pub value: T,
    pub label: &'static str,
    pub hint: Option<&'static str>,
}

/// Radio rows under a title: each row a label at 500 and its hint; the whole row takes the press.
pub fn choice_group<T: Copy + PartialEq + 'static>(
    legend: &'static str,
    choices: Vec<Choice<T>>,
    get: impl Fn() -> T + Clone + 'static,
    set: impl Fn(T) + Clone + 'static,
) -> AnyElement {
    choice_group_gated(legend, choices, |_| None, get, set)
}

/// [`choice_group`] where `off` gives the reason a choice cannot be taken (a format a run with
/// tools cannot export): that row shows at half strength, takes no press, and says why in place
/// of its hint.
pub fn choice_group_gated<T: Copy + PartialEq + 'static>(
    legend: &'static str,
    choices: Vec<Choice<T>>,
    off: impl Fn(T) -> Option<&'static str>,
    get: impl Fn() -> T + Clone + 'static,
    set: impl Fn(T) + Clone + 'static,
) -> AnyElement {
    let rows: Vec<AnyElement> = choices
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let why = off(c.value);
            let checked = {
                let get = get.clone();
                move || get() == c.value
            };
            let c1 = checked.clone();
            let c2 = checked.clone();
            let set = set.clone();
            let indicator = view()
                .mt(px(1.0))
                .w(px(16.0))
                .h(px(16.0))
                .flex_none()
                .rounded(px(8.0))
                .items_center()
                .justify_center()
                .border_1()
                .border_color(move || if c1() { theme().primary } else { theme().input })
                .bg(move || if c2() { theme().primary } else { theme().background })
                .child(view().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(move || theme().primary_foreground));
            let row = radio()
                .checked(checked)
                .disabled(why.is_some())
                .on_change(move |_: &bool| set(c.value))
                .flex_row()
                .items_start()
                .gap(px(12.0))
                .min_h(px(40.0))
                .px(px(12.0))
                .py(px(10.0))
                .opacity(if why.is_some() { 0.5_f32 } else { 1.0 })
                .cursor(if why.is_some() { Cursor::Default } else { Cursor::Pointer })
                .focus_visible(|s| s.ring(px(2.0)).ring_color(token("ring")))
                .child(indicator)
                .child(option_words(c.label, why.or(c.hint), true));
            view().child(row).child(row_separator(i)).into_element()
        })
        .collect();
    view()
        .gap(px(6.0))
        .min_w(px(0.0))
        .child(group_title(legend))
        .child(group_box().child(base_radio_group().accessibility_label(legend).children(rows)))
        .into_element()
}

/// A row's label (`strong`: at 500) and its hint under it.
fn option_words(label: &'static str, hint: Option<&'static str>, strong: bool) -> impl IntoElement {
    let mut l = rich(label, 13.0).text_size(px(13.0)).line_height(px(18.0)).text_color(move || theme().foreground);
    if strong {
        l = l.font_weight(FontWeight::MEDIUM);
    }
    view()
        .flex_1()
        .min_w(px(0.0))
        .gap(px(2.0))
        .child(l)
        .child(hint.map(|h| rich(h, 12.0).text_size(px(12.0)).line_height(px(16.0)).text_color(muted_ink)))
}

/// A checkbox row: the box, its label and an optional hint; the whole row takes the press.
pub fn check_option(
    label: &'static str,
    hint: Option<&'static str>,
    get: impl Fn() -> bool + Clone + 'static,
    set: impl Fn(bool) + 'static,
) -> AnyElement {
    let shown = get.clone();
    base_checkbox()
        .checked(move || get())
        .on_change(move |next: &bool| set(*next))
        .flex_row()
        .items_start()
        .gap(px(12.0))
        .min_h(px(40.0))
        .px(px(12.0))
        .py(px(10.0))
        .cursor(Cursor::Pointer)
        .indicator_style(|s| {
            s.mt(px(1.0))
                .w(px(16.0))
                .h(px(16.0))
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(radius::XS))
                .border_1()
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .checked(|s| s.bg(move || theme().primary).border_color(move || theme().primary))
        })
        .indicator(view().opacity(move || if shown() { 1.0_f32 } else { 0.0 }).child(icon(icon::CHECK, 14.0, move || theme().primary_foreground)))
        .child(option_words(label, hint, false))
        .into_element()
}

/// The file type as a full-width segmented control under its title.
pub fn file_type_group(get: impl Fn() -> FileType + 'static, set: impl Fn(FileType) + 'static) -> AnyElement {
    view()
        .gap(px(12.0))
        .child(group_title("File type"))
        .child(segmented_with_get(
            FILE_TYPES.iter().map(|t| (*t, file_type_label(*t))).collect(),
            get,
            set,
            "File type",
            true,
        ))
        .into_element()
}

/// The file types, in the order the control shows them.
pub const FILE_TYPES: [FileType; 4] = [FileType::Jsonl, FileType::Json, FileType::Parquet, FileType::Csv];

/// "JSONL", "JSON", "Parquet", "CSV".
pub fn file_type_label(t: FileType) -> &'static str {
    match t {
        FileType::Jsonl => "JSONL",
        FileType::Json => "JSON",
        FileType::Parquet => "Parquet",
        FileType::Csv => "CSV",
    }
}

/// Where the thinking trace goes, in words (mono between backticks).
pub fn placement_label(p: ReasoningPlacement) -> &'static str {
    match p {
        ReasoningPlacement::ThinkTags => "Inline `<think>` tags",
        ReasoningPlacement::ReasoningContent => "`reasoning_content` field",
        ReasoningPlacement::Thinking => "`thinking` field",
        ReasoningPlacement::Omit => "Leave out",
    }
}

/// `words` as text, the parts between backticks in Roboto Mono at 0.92 of `size` (the web UI's
/// `<code>` in option labels). The caller sets the size, color and weight, which the parts inherit.
pub fn rich(words: &'static str, size: f32) -> Text {
    let mut t = text("");
    for (part, code) in spans(words) {
        let span = text(part);
        t = t.child(if code { span.font_family(ROBOTO_MONO.family).text_size(px(size * 0.92)).letter_spacing(px(0.0)) } else { span });
    }
    t
}

/// `words` cut at its backticks: (text, is code).
fn spans(words: &str) -> Vec<(&str, bool)> {
    words.split('`').enumerate().filter(|(_, p)| !p.is_empty()).map(|(i, p)| (p, i % 2 == 1)).collect()
}

/// Skip reasons from the preview, in words.
fn skip_words(reason: &str) -> String {
    match reason {
        "truncated" => "truncated".into(),
        "failed" => "failed".into(),
        "pending" => "not finished".into(),
        "missing" => "missing a response".into(),
        "no_reasoning" => "without a thinking trace".into(),
        "extra_samples" => "extra samples per prompt".into(),
        "empty_response" => "empty".into(),
        other => other.replace('_', " "),
    }
}

/// "Skipped: 10 failed, 3 truncated." or nothing when nothing was skipped. The reasons keep the
/// server's order (its JSON object's).
pub fn skipped_text(skipped: &std::collections::BTreeMap<String, i64>) -> Option<String> {
    skipped_text_of(skipped.iter().map(|(k, v)| (k.as_str(), *v)))
}

fn skipped_text_of<'a>(skipped: impl Iterator<Item = (&'a str, i64)>) -> Option<String> {
    let parts: Vec<String> = skipped.filter(|(_, n)| *n > 0).map(|(k, n)| format!("{} {}", int(n), skip_words(k))).collect();
    (!parts.is_empty()).then(|| format!("Skipped: {}.", parts.join(", ")))
}

/// The options as a query string, as the web UI's `qs`: `?format=messages&include_system=true`;
/// empty and null values left out; empty when nothing is left.
pub fn query_string(options: &Value) -> String {
    let Value::Object(map) = options else { return String::new() };
    let parts: Vec<String> = map
        .iter()
        .filter_map(|(k, v)| {
            let value = match v {
                Value::Null => return None,
                Value::String(s) if s.is_empty() => return None,
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                other => other.to_string(),
            };
            Some(format!("{}={}", encode(k), encode(&value)))
        })
        .collect();
    if parts.is_empty() { String::new() } else { format!("?{}", parts.join("&")) }
}

/// `application/x-www-form-urlencoded`, as `URLSearchParams` writes it.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A repository as "owner/name": letters, digits, `_`, `.` and `-` on each side of one slash.
fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if valid_split(a) && valid_split(b))
}

/// A split name: letters, digits, `_`, `.` and `-`, at least one.
fn valid_split(split: &str) -> bool {
    !split.is_empty() && split.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// "huggingface.co/datasets/you/x" without its prefix: "you/x".
fn short_hub_url(url: &str) -> &str {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.strip_prefix("huggingface.co/datasets/").unwrap_or(url)
}

/// The Hugging Face push: the repository, the split, private or not, and Push dataset; or why it
/// cannot push (no token: a link to Settings). `nothing` holds while there is nothing to push;
/// `pushing` is the dialog's (see [`export_dialog`]).
///
/// Its signals are the dialog content's, which goes when the dialog closes: the answers write them
/// with `try_set` (the token check may come after a quick close).
fn hub_push(open: Signal<bool>, spec: Rc<ExportSpec>, nothing: impl Fn() -> bool + Clone + 'static, pushing: Signal<bool>) -> impl IntoElement {
    let settings = signal(None::<Result<Settings, String>>);
    let load_settings = move || {
        call(|api| async move { api.settings().await }, move |r: Result<Settings, ApiError>| {
            settings.try_set(Some(r.map_err(|e| e.message)));
        });
    };
    load_settings();

    let repo = signal(String::new());
    let split = signal(String::from("train"));
    let private = signal(true);
    let repo_error = signal(None::<String>);
    let split_error = signal(None::<String>);
    let push_error = signal(None::<String>);
    let pushed = signal(None::<crate::api::types::HubPushOut>);
    let repo_ref = element_ref();
    let split_ref = element_ref();
    let repo_example = spec.repo_example;

    let n1 = nothing.clone();
    let submit = move || {
        if pushing.get_untracked() || untracked(&n1) {
            return;
        }
        let repo_id = repo.get_untracked().trim().to_owned();
        let split_name = split.get_untracked().trim().to_owned();
        let bad_repo = (!valid_repo(&repo_id)).then(|| format!("Enter the repository as owner/name, like {repo_example}."));
        let bad_split = (!valid_split(&split_name)).then(|| "Use letters, numbers, dots, dashes and underscores.".to_owned());
        repo_error.set(bad_repo.clone());
        split_error.set(bad_split.clone());
        if bad_repo.is_some() {
            repo_ref.focus();
            return;
        }
        if bad_split.is_some() {
            split_ref.focus();
            return;
        }
        pushing.set(true);
        push_error.set(None);
        pushed.set(None);
        let target = HubPushIn { repo_id, private: private.get_untracked(), split: split_name };
        let options = untracked(|| (spec.options)());
        let base = spec.base.clone();
        call(
            move |api| async move { api.export_hub(&base, &options, &target).await },
            move |result| {
                pushing.try_set(false);
                match result {
                    Ok(out) => {
                        pushed.try_set(Some(out));
                    }
                    Err(e) => {
                        let field_repo = e.fields.get("target.repo_id").or_else(|| e.fields.get("repo_id")).cloned();
                        let field_split = e.fields.get("target.split").cloned();
                        if e.fields.is_empty() {
                            push_error.try_set(Some(e.message));
                        }
                        repo_error.try_set(field_repo.clone());
                        split_error.try_set(field_split.clone());
                        if field_repo.is_some() {
                            repo_ref.focus();
                        } else if field_split.is_some() {
                            split_ref.focus();
                        }
                    }
                }
            },
        );
    };

    let content = dynamic(move || -> AnyElement {
        let state = settings.get();
        match state {
            None => view().h(px(96.0)).rounded(px(radius::XL)).bg(move || theme().secondary).into_element(),
            Some(Err(message)) => error_block(
                format!("Couldn’t check for a Hugging Face token. {message}"),
                Some(Rc::new(move || {
                    settings.set(None);
                    load_settings();
                })),
            )
            .into_element(),
            Some(Ok(s)) if !s.hf_token_set => body(text("Pushing needs a Hugging Face token with write access. "))
                .text_color(muted_ink)
                .px(px(12.0))
                .py(px(12.0))
                .rounded(px(radius::XL))
                .bg(dialog_muted)
                .child(
                    text("Add a token in Settings")
                        .underline()
                        .text_color(move || theme().foreground)
                        .role(AccessibilityRole::Link)
                        .cursor(Cursor::Pointer)
                        .on_press(move || {
                            open.set(false);
                            nav::open(Destination::Settings);
                        }),
                )
                .into_element(),
            Some(Ok(_)) => {
                let submit_field = submit.clone();
                let submit_split = submit.clone();
                let submit_button = submit.clone();
                let n2 = nothing.clone();
                let group = form_group(vec![
                    form_row(
                        "Dataset repository",
                        text_field(repo)
                            .mono()
                            .placeholder(repo_example)
                            .invalid(move || repo_error.with(Option::is_some))
                            .element(&repo_ref)
                            .accessibility_label("Dataset repository")
                            .on_change(move |_| {
                                if repo_error.with_untracked(Option::is_some) {
                                    repo_error.set(None);
                                }
                            })
                            .on_submit(move || submit_field()),
                    )
                    .hint("Created if it does not exist.")
                    .error(repo_error)
                    .into_element(),
                    form_row(
                        "Split",
                        view().max_w(px(160.0)).child(
                            text_field(split)
                                .mono()
                                .invalid(move || split_error.with(Option::is_some))
                                .element(&split_ref)
                                .accessibility_label("Split")
                                .on_change(move |_| {
                                    if split_error.with_untracked(Option::is_some) {
                                        split_error.set(None);
                                    }
                                })
                                .on_submit(move || submit_split()),
                        ),
                    )
                    .error(split_error)
                    .into_element(),
                    check_option("Private dataset", None, move || private.get(), move |v| private.set(v)),
                ])
                .in_dialog();
                view()
                    .gap(px(12.0))
                    .child(group)
                    .child(dynamic(move || push_error.get().map(|m| error_block(m, None))))
                    .child(dynamic(move || pushed.get().map(pushed_note)))
                    .child(
                        view().flex_row().child(
                            button("Push dataset")
                                .variant(Variant::Secondary)
                                .busy(move || pushing.get(), "Pushing…")
                                .disabled(move || n2())
                                .on_press(move || submit_button()),
                        ),
                    )
                    .into_element()
            }
        }
    });
    view()
        .gap(px(12.0))
        .min_w(px(0.0))
        .role(AccessibilityRole::Group)
        .label("Push to Hugging Face")
        .child(group_title("Push to Hugging Face").role(AccessibilityRole::Header))
        .child(content)
}

/// "Pushed 1,284 records to you/x ↗" on the green tint; the link opens the dataset.
fn pushed_note(out: crate::api::types::HubPushOut) -> impl IntoElement {
    let url = out.url.clone();
    let short = short_hub_url(&out.url).to_owned();
    view()
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(radius::XL))
        .bg(token("positive-tint"))
        .child(
            body(text(format!("Pushed {} to ", count(out.count, "record", "records"))))
                .text_color(token("positive-ink"))
                .child(
                    text(short)
                        .font_family(ROBOTO_MONO.family)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .underline()
                        .role(AccessibilityRole::Link)
                        .cursor(Cursor::Pointer)
                        .on_press(move || {
                            let _ = open_url(url.clone());
                        }),
                )
                .child(text(" ↗").text_size(px(12.0)).accessibility_elements_hidden(true)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skipped_reasons_read_as_words() {
        let skipped = [("failed", 10), ("truncated", 3), ("no_reasoning", 0), ("some_new_reason", 2)];
        assert_eq!(
            skipped_text_of(skipped.into_iter()).as_deref(),
            Some("Skipped: 10 failed, 3 truncated, 2 some new reason.")
        );
        assert_eq!(skipped_text_of([("failed", 0)].into_iter()), None);
        assert_eq!(skip_words("pending"), "not finished");
        assert_eq!(skip_words("extra_samples"), "extra samples per prompt");
    }

    #[test]
    fn options_become_the_web_uis_query_string() {
        let options = serde_json::json!({
            "format": "messages", "file_type": "jsonl", "include_system": true, "first_sample_only": false,
            "note": "", "skip": null, "name": "a b/c"
        });
        let q = query_string(&options);
        assert!(q.starts_with('?'));
        let parts: Vec<&str> = q[1..].split('&').collect();
        assert!(parts.contains(&"format=messages"));
        assert!(parts.contains(&"include_system=true"));
        assert!(parts.contains(&"first_sample_only=false"));
        assert!(parts.contains(&"name=a+b%2Fc"));
        assert!(!q.contains("note") && !q.contains("skip"));
        assert_eq!(query_string(&serde_json::json!({})), "");
    }

    #[test]
    fn hub_targets_are_checked_as_the_web_ui_checks_them() {
        assert!(valid_repo("your-name/qwen3-math.distill_v2"));
        assert!(!valid_repo("no-owner"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo("a/"));
        assert!(!valid_repo("me/my repo"));
        assert!(valid_split("train"));
        assert!(!valid_split(""));
        assert!(!valid_split("tr ain"));
        assert_eq!(short_hub_url("https://huggingface.co/datasets/you/x"), "you/x");
        assert_eq!(short_hub_url("https://www.huggingface.co/datasets/you/x"), "you/x");
        assert_eq!(short_hub_url("https://example.com/x"), "https://example.com/x");
    }

    #[test]
    fn the_preview_stays_in_view_until_the_options_end() {
        assert_eq!(sticky_offset(0.0, 900.0, 400.0), 0.0);
        assert_eq!(sticky_offset(120.0, 900.0, 400.0), 120.0);
        assert_eq!(sticky_offset(700.0, 900.0, 400.0), 500.0);
        assert_eq!(sticky_offset(50.0, 300.0, 400.0), 0.0);
    }

    #[test]
    fn backticks_mark_mono_parts() {
        assert_eq!(spans("Inline `<think>` tags"), vec![("Inline ", false), ("<think>", true), (" tags", false)]);
        assert_eq!(spans("`thinking` field"), vec![("thinking", true), (" field", false)]);
        assert_eq!(spans("Leave out"), vec![("Leave out", false)]);
    }
}
