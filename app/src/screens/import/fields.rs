//! The import screen's controls (frontend/src/components/import/source-fields.tsx,
//! mapping-fields.tsx, detected-summary.tsx, import-mode.tsx): the source tabs and the fields of
//! each source, the mapping, the name, Simple mode's "Detected" list and the Simple or Advanced
//! switch. Each reads and writes the one [`State`].

use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;

use super::helpers::{
    FORMAT_OPTIONS, ImportMode, SPLIT_OPTIONS, SourceTab, SummaryPart, SummaryRow, format_label, format_name, info, save_import_mode,
    split_hint, split_name,
};
use super::state::State;
use crate::api::types::{ColumnInfo, ColumnKind, ImportFormat, SplitMode, Turns};
use crate::format::bytes;
use crate::theme::fonts::ROBOTO_MONO;
use crate::theme::text::{body, meta};
use crate::theme::{muted_ink, radius, token};
use crate::ui::button::{Size, Variant, button};
use crate::ui::choice::{checkbox_with, segmented, segmented_with_get};
use crate::ui::field::{form_group, form_item, form_row, text_area, text_field};
use crate::ui::icon::{self, icon as glyph};
use crate::ui::info_tip::info_tip;
use crate::ui::select::{option, select_row};

const ALL: &str = "__all__";
const NONE: &str = "__none__";

/// "Simple | Advanced" and its (i), as the Rendered or Raw switch.
pub fn mode_toggle(st: State) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .child(segmented_with_get(
            vec![(ImportMode::Simple, "Simple"), (ImportMode::Advanced, "Advanced")],
            move || st.mode.get(),
            move |m| {
                st.mode.set(m);
                save_import_mode(m);
            },
            "Import settings",
            false,
        ))
        .child(info_tip("Simple and Advanced", info::MODE))
}

/// The source tabs, as wide as the column.
pub fn source_tabs(st: State) -> impl IntoElement {
    segmented(
        vec![(SourceTab::HuggingFace, "Hugging Face"), (SourceTab::File, "File"), (SourceTab::Paste, "Paste")],
        st.tab,
        "Source",
        true,
    )
}

/// Dataset, and in Advanced the config and split once the dataset has told them.
pub fn hugging_face_fields(st: State, load: impl Fn() + Copy + 'static, error: Signal<Option<String>>) -> impl IntoElement {
    let loading = move || st.fetching.get() && st.preview.with(Option::is_none);
    let dataset = form_row(
        "Dataset",
        view()
            .flex_row()
            .gap(px(8.0))
            .child(
                view().flex_1().min_w(px(0.0)).child(
                    text_field(st.hf_input)
                        .mono_body()
                        .placeholder("openai/gsm8k")
                        .invalid(move || error.with(Option::is_some))
                        .on_submit(load),
                ),
            )
            .child(
                button("Load")
                    .variant(Variant::Secondary)
                    .disabled(move || st.hf_input.with(|i| i.trim().is_empty()) || loading())
                    .busy(loading, "Loading…")
                    .on_press(load),
            ),
    )
    .info(info::DATASET)
    .error(error)
    .stacked();

    let splits = move || -> Vec<AnyElement> {
        if st.simple() {
            return Vec::new();
        }
        let Some(info_now) = st.info().filter(|i| !i.splits.is_empty()) else { return Vec::new() };
        let mut configs: Vec<String> = Vec::new();
        for s in &info_now.splits {
            if !configs.contains(&s.config) {
                configs.push(s.config.clone());
            }
        }
        let config = st.effective_config();
        let in_config: Vec<String> =
            info_now.splits.iter().filter(|s| Some(&s.config) == config.as_ref()).map(|s| s.split.clone()).collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        if configs.len() > 1 {
            let options: Vec<SelectItem> = configs.iter().map(|c| option(c.clone(), c.clone())).collect();
            rows.push(
                form_row(
                    "Config",
                    select_row(
                        options,
                        move || st.effective_config().map(SharedString::from),
                        move |next| {
                            if let Some(c) = next {
                                st.pick_config(&c);
                            }
                        },
                        "Config",
                    )
                    .fill()
                    .mono(),
                )
                .info(info::CONFIG)
                .into_element(),
            );
        }
        let mut options: Vec<SelectItem> = in_config.iter().map(|s| option(s.clone(), s.clone())).collect();
        if in_config.len() > 1 {
            options.push(option(ALL, "All splits"));
        }
        rows.push(
            form_row(
                "Split",
                select_row(
                    options,
                    move || {
                        if st.all_splits.get() { Some(SharedString::from(ALL)) } else { st.effective_split().map(SharedString::from) }
                    },
                    move |next| {
                        if let Some(v) = next {
                            st.pick_split(if v.as_ref() == ALL { None } else { Some(v.to_string()) });
                        }
                    },
                    "Split",
                )
                .fill(),
            )
            .info(info::SPLIT)
            .into_element(),
        );
        rows
    };
    form_group(vec![dataset.into_element()]).dynamic_rows(splits)
}

/// The card that takes a file (by a press, or dropped on it), the format in Advanced.
pub fn file_fields(st: State, pick: impl Fn() + Copy + 'static, dropped: impl Fn(String) + Copy + 'static, error: Signal<Option<String>>) -> impl IntoElement {
    let over = signal(false);
    let card = dynamic(move || {
        let upload = st.upload.get();
        let uploading = st.uploading.get();
        let has = upload.is_some() || uploading;
        let content: AnyElement = if has {
            let name = upload.as_ref().map(|u| u.filename.clone()).or_else(|| st.upload_name.get()).unwrap_or_default();
            let status = if uploading {
                "Uploading…".to_owned()
            } else {
                upload
                    .as_ref()
                    .map(|u| format!("{} · detected {}", bytes(u.size.max(0) as u64), format_label(&u.detected_format)))
                    .unwrap_or_default()
            };
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .child(glyph(icon::FILE_TEXT, 20.0, muted_ink))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(body(text(name)).font_family(ROBOTO_MONO.family).number_of_lines(1))
                        .child(meta(text(status)).text_color(muted_ink)),
                )
                .child(button("Replace file").variant(Variant::Secondary).size(Size::Sm).on_card().disabled(move || st.uploading.get()).on_press(pick))
                .into_element()
        } else {
            view()
                .items_start()
                .gap(px(8.0))
                .child(body(text("Choose a file")).font_weight(FontWeight::MEDIUM))
                .child(meta(text("or drop it here. Text, Markdown, CSV, TSV, JSON, JSONL or Parquet.")).text_color(muted_ink))
                .into_element()
        };
        let mut zone = view()
            .rounded(px(radius::XL))
            .p(px(16.0))
            .bg(move || if over.get() { theme().accent } else { theme().muted })
            .on_file_drag_enter(move |_: &FileDragEvent| over.set(true))
            .on_file_drag_leave(move || over.set(false))
            .on_file_drop(move |e: &FileDropEvent| {
                over.set(false);
                if let Some(uri) = e.uris.first() {
                    if !st.uploading.get_untracked() {
                        dropped(uri.clone());
                    }
                }
            })
            .child(content);
        if !has {
            zone = zone.role(AccessibilityRole::Button).label("Choose a file").cursor(Cursor::Pointer).focusable(true).on_press(pick);
        }
        zone
    });
    let error_line = dynamic(move || {
        error.get().map(|m| body(text(m)).text_color(token("negative-ink")).role(AccessibilityRole::Alert).mt(px(-8.0)))
    });
    let format_row = when(move || !st.simple() && st.upload.with(Option::is_some), move || {
        let options: Vec<SelectItem> = FORMAT_OPTIONS.iter().map(|(f, label)| option(format_name(*f), *label)).collect();
        form_group(vec![
            form_row(
                "Format",
                select_row(
                    options,
                    move || Some(SharedString::from(format_name(st.format.get()))),
                    move |next| {
                        if let Some(v) = next {
                            if let Some((f, _)) = FORMAT_OPTIONS.iter().find(|(f, _)| format_name(*f) == v.as_ref()) {
                                st.format.set(*f);
                            }
                        }
                    },
                    "Format",
                )
                .fill(),
            )
            .info(info::FORMAT)
            .into_element(),
        ])
    });
    view().gap(px(16.0)).child(card).child(error_line).child(format_row)
}

/// The text to paste.
pub fn paste_fields(st: State) -> impl IntoElement {
    form_group(vec![
        form_row(
            "Prompts",
            text_area(st.paste)
                .mono_body()
                .height(192.0)
                .placeholder("Explain how a stellarator confines plasma.\n---\nWrite a haiku about copper coils."),
        )
        .hint("Separate prompts with ---, a blank line or headings.")
        .info(info::PASTE)
        .stacked()
        .into_element(),
    ])
}

/// What Simple mode found, in place of the settings.
pub fn detected_summary(rows: Vec<SummaryRow>) -> impl IntoElement {
    let term_width = rows.iter().map(|r| r.term.chars().count()).max().unwrap_or(0) as f32 * 7.4 + 3.0;
    let lines = rows.into_iter().map(move |r| {
        let spans: Vec<AnyElement> = r
            .parts
            .iter()
            .map(|p| match p {
                SummaryPart::Text(s) => text(s.clone()).into_element(),
                SummaryPart::Code(s) => text(s.clone()).font_family(ROBOTO_MONO.family).text_size(px(12.0)).into_element(),
            })
            .collect();
        let value = body(text("")).children(spans);
        let value = if r.warn { value.text_color(token("coral-ink")) } else { value };
        view()
            .flex_row()
            .items_start()
            .gap(px(16.0))
            .child(view().w(px(term_width)).flex_none().child(body(text(r.term.clone())).text_color(muted_ink)))
            .child(view().flex_1().min_w(px(0.0)).child(value))
            .into_element()
    });
    view()
        .gap(px(6.0))
        .min_w(px(0.0))
        .child(body(text("Detected")).px(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(muted_ink).role(AccessibilityRole::Header))
        .child(view().rounded(px(radius::XL)).bg(move || theme().muted).p(px(16.0)).gap(px(8.0)).children(lines.collect::<Vec<_>>()))
}

/// "text", "conversation", ...: what kind of column it is.
fn kind_label(c: &ColumnInfo) -> &'static str {
    match c.kind {
        ColumnKind::Messages => "conversation",
        ColumnKind::Text => "text",
        ColumnKind::Number => "number",
        ColumnKind::Other => "other",
    }
}

/// Prompt column, System prompt column and the turns switch; or, for text, how it is split.
pub fn mapping_group(st: State) -> impl IntoElement {
    let tabular = move || st.preview.with(|p| p.as_ref().is_some_and(|p| !p.columns.is_empty()));
    when(tabular, move || {
        let prompt = dynamic(move || {
            let (options, titles) = column_options(st, false);
            Some(
                form_row(
                    "Prompt column",
                    select_row(
                        options,
                        move || {
                            let chosen = st.prompt_field.get();
                            let value = match chosen {
                                None => st.preview.with(|p| p.as_ref().and_then(|p| p.prompt_field.clone())),
                                Some(v) => v,
                            };
                            value.map(SharedString::from)
                        },
                        move |next| st.prompt_field.set(Some(next.map(|v| v.to_string()).filter(|v| v != NONE))),
                        "Prompt column",
                    )
                    .title(move |v| titles(v))
                    .fill(),
                )
                .info(info::PROMPT_COLUMN),
            )
        });
        let system = dynamic(move || {
            let (options, titles) = column_options(st, true);
            Some(
                form_row(
                    "System prompt column",
                    select_row(
                        options,
                        move || {
                            let chosen = st.system_field.get();
                            let value = match chosen {
                                None => st.preview.with(|p| p.as_ref().and_then(|p| p.system_field.clone())),
                                Some(v) => v,
                            };
                            Some(SharedString::from(value.unwrap_or_else(|| NONE.to_owned())))
                        },
                        move |next| st.system_field.set(Some(next.map(|v| v.to_string()).filter(|v| v != NONE))),
                        "System prompt column",
                    )
                    .title(move |v| titles(v))
                    .fill(),
                )
                .info(info::SYSTEM_COLUMN),
            )
        });
        // Which part of a conversation becomes the prompt: only for datasets of conversations.
        let turns = move || -> Vec<AnyElement> {
            if !st.preview.with(|p| p.as_ref().is_some_and(|p| p.has_conversations)) {
                return Vec::new();
            }
            vec![
                form_row(
                    "Prompt",
                    segmented(vec![(Turns::FirstUser, "First user turn"), (Turns::UpToLastUser, "Whole conversation")], st.turns, "Prompt", true),
                )
                .info(info::TURNS)
                .align_end()
                .into_element(),
            ]
        };
        form_group(vec![prompt.into_element(), system.into_element()]).title("Mapping").dynamic_rows(turns)
    })
    .otherwise(move || {
        let options: Vec<SelectItem> = SPLIT_OPTIONS.iter().map(|(m, label, _)| option(split_name(*m), *label)).collect();
        form_group(vec![
            form_row(
                "Split into prompts",
                select_row(
                    options,
                    move || Some(SharedString::from(split_name(st.split_mode.get()))),
                    move |next| {
                        if let Some(v) = next {
                            if let Some((m, _, _)) = SPLIT_OPTIONS.iter().find(|(m, _, _)| split_name(*m) == v.as_ref()) {
                                st.split_mode.set(*m);
                            }
                        }
                    },
                    "Split into prompts",
                )
                .fill(),
            )
            .hint_when(move || {
                if st.split_mode.get() != SplitMode::Auto {
                    return None;
                }
                let mode = st.preview.with(|p| p.as_ref().and_then(|p| p.split_mode));
                split_hint(mode).map(|h| SharedString::from(format!("Automatic: {h}.")))
            })
            .info(info::SPLIT_TEXT)
            .into_element(),
        ])
    })
}

/// The select items of a column select and the words the button says for a value.
fn column_options(st: State, optional: bool) -> (Vec<SelectItem>, impl Fn(&str) -> String + 'static) {
    let columns: Vec<ColumnInfo> = st.preview.with(|p| p.as_ref().map(|p| p.columns.clone()).unwrap_or_default());
    let ordered: Vec<ColumnInfo> =
        columns.iter().filter(|c| !c.looks_like_output).chain(columns.iter().filter(|c| c.looks_like_output)).cloned().collect();
    let mut options: Vec<SelectItem> = Vec::new();
    if optional {
        options.push(option(NONE, "None"));
    }
    let describe = |c: &ColumnInfo| format!("{} · {}", c.name, if c.looks_like_output { "output" } else { kind_label(c) });
    options.extend(ordered.iter().map(|c| option(c.name.clone(), describe(c))));
    let titles: Vec<(String, String)> = ordered.iter().map(|c| (c.name.clone(), describe(c))).collect();
    (options, move |v: &str| {
        if v == NONE {
            return "None".to_owned();
        }
        titles.iter().find(|(name, _)| name == v).map(|(_, t)| t.clone()).unwrap_or_else(|| v.to_owned())
    })
}

/// Skip rows (or prompts), Import at most, and Remove duplicate prompts.
pub fn limits_group(
    st: State,
    offset_error: Signal<Option<String>>,
    limit_error: Signal<Option<String>>,
    submit: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let tabular = move || st.preview.with(|p| p.as_ref().is_some_and(|p| !p.columns.is_empty()));
    let make = move |rows_label: &'static str, offset_help: &'static str, limit_help: &'static str| {
        let dedupe = form_item(
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .child(checkbox_with("Remove duplicate prompts", move || st.dedupe.get(), move |on| st.dedupe.set(on)))
                .child(info_tip("Remove duplicate prompts", info::DEDUPE)),
        );
        form_group(vec![
            form_row(rows_label, text_field(st.offset).on_submit(submit).invalid(move || offset_error.with(Option::is_some)))
                .error(offset_error)
                .info(offset_help)
                .into_element(),
            form_row(
                "Import at most",
                text_field(st.limit).placeholder("All").on_submit(submit).invalid(move || limit_error.with(Option::is_some)),
            )
            .error(limit_error)
            .info(limit_help)
            .into_element(),
            dedupe.into_element(),
        ])
    };
    when(tabular, move || make("Skip rows", info::OFFSET_ROWS, info::LIMIT_ROWS))
        .otherwise(move || make("Skip prompts", info::OFFSET_PROMPTS, info::LIMIT_PROMPTS))
}

/// The name of the set (Advanced).
pub fn name_group(st: State, error: Signal<Option<String>>, submit: impl Fn() + Copy + 'static) -> impl IntoElement {
    form_group(vec![
        form_row(
            "Name",
            text_field(st.name)
                .invalid(move || error.with(Option::is_some))
                .on_change(move |_| st.name_touched.set(true))
                .on_submit(submit),
        )
        .error(error)
        .info(info::NAME)
        .into_element(),
    ])
}

/// Whether `format` is one of the file formats (kept for the file picker's filter).
#[allow(dead_code, reason = "documents the formats the picker offers")]
pub fn is_known_format(format: ImportFormat) -> bool {
    FORMAT_OPTIONS.iter().any(|(f, _)| *f == format)
}
