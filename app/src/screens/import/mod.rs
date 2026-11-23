//! Import prompts (frontend/src/pages/prompt-import.tsx): Hugging Face datasets (every split, the
//! first user turn of conversations), files (md, txt, csv, tsv, json, jsonl, parquet) and pasted
//! text, in Simple mode (what was detected) or Advanced (every setting), with a preview that is read
//! again as the settings change. Import starts the work in the background and opens the new set,
//! whose page shows the progress.
//!
//! From 1024 px the source and settings are a 448 px column beside the preview, with the button
//! under them; below, one column (source, preview, settings) with the button in the floating bar.

mod fields;
mod helpers;
mod preview;
mod state;

use std::time::Duration;

use nsh_core::file_dialog::FileFilter;
use nsh_core::picker::{FilePickRequest, PickedStatus};
use nsh_core::prelude::*;

use self::fields::{
    detected_summary, file_fields, hugging_face_fields, limits_group, mapping_group, mode_toggle, name_group, paste_fields, source_tabs,
};
use self::helpers::{EXTENSIONS, SourceTab, import_label, import_summary, is_dataset_id, parse_dataset};
use self::preview::preview_panel;
use self::state::{HfInfo, Request, State};
use crate::api::ApiError;
use crate::data::call;
use crate::nav::{self, Route};
use crate::theme::text::body;
use crate::theme::{muted_ink, radius, token};
use crate::ui::action_bar::action_bar;
use crate::ui::button::button;
use crate::ui::page::{page, window_width};

/// A preview waits this long after the last change to the mapping or the pasted text.
const SETTLE: Duration = Duration::from_millis(400);
/// Simple mode loads a dataset by itself once its name has been still this long.
const AUTO_LOAD: Duration = Duration::from_millis(600);
/// The width from which the settings sit beside the preview (Tailwind's `lg`).
const TWO_COLUMNS_FROM: f32 = 1024.0;
/// The settings column (`28rem`).
const SETTINGS_COLUMN: f32 = 448.0;

/// A signal that follows `f`, for what a control shows under itself.
fn derived(f: impl Fn() -> Option<String> + 'static) -> Signal<Option<String>> {
    let out = signal(None::<String>);
    effect(move || out.set(f()));
    out
}

/// The message of a field in the commit's error, when the server named it.
fn commit_field(st: State, field: &'static str) -> Option<String> {
    st.commit_error.with(|e| e.as_ref().and_then(|e| e.fields.get(field).cloned()))
}

/// The Import screen.
pub fn screen() -> impl IntoElement {
    let st = State::new();
    let fetch_ticket = signal(0_u64);
    let wait_ticket = signal(0_u64);
    let last_request = signal(None::<Request>);
    let data_identity = signal(String::new());

    // ---- the preview: asked for when the source or the mapping changes ----
    let start_fetch = move |req: Request| {
        let n = fetch_ticket.get_untracked() + 1;
        fetch_ticket.set(n);
        // The preview of another source is not shown while this one loads.
        if data_identity.get_untracked() != req.identity {
            st.preview.set(None);
            st.preview_error.set(None);
        }
        st.fetching.set(true);
        let identity = req.identity.clone();
        let dataset = match &req.source {
            crate::api::types::ImportSource::Huggingface { dataset, .. } => Some(dataset.clone()),
            _ => None,
        };
        call(
            move |api| async move { api.import_preview(&req.source, &req.mapping).await },
            move |result| {
                if fetch_ticket.get_untracked() != n {
                    return;
                }
                st.fetching.set(false);
                match result {
                    Ok(p) => {
                        // Remember the dataset's configs and splits so the selects survive a failed new preview.
                        if let (Some(dataset), false) = (dataset, p.hf_splits.is_empty()) {
                            st.hf_info.set(Some(HfInfo { dataset, splits: p.hf_splits.clone(), config: p.hf_config.clone(), split: p.hf_split.clone() }));
                        }
                        data_identity.set(identity);
                        st.preview_error.set(None);
                        st.preview.set(Some(p));
                    }
                    Err(e) => {
                        st.preview.set(None);
                        st.preview_error.set(Some(e.message));
                    }
                }
            },
        );
    };
    effect(move || {
        let request = st.request();
        st.has_source.set(st.source_present());
        let n = wait_ticket.get_untracked() + 1;
        wait_ticket.set(n);
        let Some(req) = request else {
            // Nothing to ask for: an answer on its way is dropped.
            fetch_ticket.update(|t| *t += 1);
            last_request.set(None);
            st.fetching.set(false);
            st.settling.set(false);
            // A row limit to fix keeps the last preview, and with it the settings it shows (the
            // limit field among them, with its error under it). No source: nothing to show.
            if !(st.source_present() && st.limits_invalid()) {
                data_identity.set(String::new());
                st.preview.set(None);
                st.preview_error.set(None);
            }
            return;
        };
        let previous = last_request.get_untracked();
        if previous.as_ref() == Some(&req) {
            return;
        }
        let pasted = matches!(req.source, crate::api::types::ImportSource::Paste { .. });
        let source_changed = previous.as_ref().is_none_or(|p| p.source != req.source);
        // A new source is read at once; typing and settings wait for a pause.
        let delay = if pasted || !source_changed { Some(SETTLE) } else { None };
        last_request.set(Some(req.clone()));
        match delay {
            None => {
                st.settling.set(false);
                start_fetch(req);
            }
            Some(delay) => {
                st.settling.set(true);
                let _ = set_timeout(delay, move || {
                    if wait_ticket.get_untracked() == n {
                        st.settling.set(false);
                        start_fetch(req);
                    }
                });
            }
        }
    });
    let refetch = move || {
        if let Some(req) = untracked(|| st.request()) {
            start_fetch(req);
        }
    };

    // ---- the dataset: Load, and Simple mode's load by itself ----
    let load_dataset = move |value: String| {
        let ds = parse_dataset(&value);
        if ds.is_empty() {
            return;
        }
        st.hf_input.set(ds.clone());
        if ds != st.hf_dataset.get_untracked() {
            st.hf_config.set(None);
            st.hf_split.set(None);
            st.all_splits.set(false);
            st.reset_mapping();
            st.name.set(String::new());
            st.name_touched.set(false);
            st.hf_dataset.set(ds);
        } else {
            refetch();
        }
    };
    let load = move || load_dataset(st.hf_input.get_untracked());
    let auto_ticket = signal(0_u64);
    effect(move || {
        let input = st.hf_input.get();
        let n = auto_ticket.get_untracked() + 1;
        auto_ticket.set(n);
        if !st.simple() || st.tab.get() != SourceTab::HuggingFace {
            return;
        }
        let ds = parse_dataset(&input);
        if is_dataset_id(&ds) && ds != st.hf_dataset.get_untracked() {
            let _ = set_timeout(AUTO_LOAD, move || {
                if auto_ticket.get_untracked() == n {
                    load_dataset(input);
                }
            });
        }
    });

    // ---- the name: follows the default until the user types ----
    effect(move || {
        let default = st.default_name();
        if !st.name_touched.get() {
            st.name.set(default);
        }
    });

    // ---- a file: chosen in the system's open dialog (or the browser's file input), or dropped ----
    let uploaded = move |result: Result<Option<crate::api::types::UploadOut>, ApiError>| {
        st.uploading.set(false);
        match result {
            Ok(Some(u)) => {
                st.upload_error.set(None);
                st.upload.set(Some(u));
                st.format.set(crate::api::types::ImportFormat::Auto);
                st.reset_mapping();
            }
            Ok(None) => {}
            Err(e) => st.upload_error.set(Some(e.message)),
        }
    };
    let begin_upload = move |name: String| {
        st.name.set(String::new());
        st.name_touched.set(false);
        st.upload_error.set(None);
        st.upload_name.set(Some(name));
        st.uploading.set(true);
    };
    let pick_file = move || {
        if st.uploading.get_untracked() {
            return;
        }
        let request = FilePickRequest::new(false, 0).filter(FileFilter::new("Prompt files", EXTENSIONS));
        call(
            move |api| async move {
                let files = pick_files(request).await;
                let Some(file) = files.into_iter().next() else { return Ok(None) };
                begin_upload(file.name.clone());
                if file.status == PickedStatus::TooLarge {
                    return Err(ApiError { status: None, message: "That file is too large to import.".into(), fields: Default::default() });
                }
                api.import_upload(&file.name, file.data.bytes().to_vec()).await.map(Some)
            },
            uploaded,
        );
    };
    let dropped = move |uri: String| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(path) = file_path_of(&uri) else { return };
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            begin_upload(name.clone());
            call(
                move |api| async move {
                    let bytes = std::fs::read(&path)
                        .map_err(|e| ApiError { status: None, message: format!("Couldn’t read {name}: {e}."), fields: Default::default() })?;
                    api.import_upload(&name, bytes).await.map(Some)
                },
                uploaded,
            );
        }
        #[cfg(target_arch = "wasm32")]
        let _ = uri;
    };

    // ---- the commit ----
    let submit = move || {
        let (blocked, source, mapping, name) =
            untracked(|| (st.gate().blocked(), st.commit_source(), st.mapping(), st.import_name().trim().to_owned()));
        if blocked || st.committing.get_untracked() {
            return;
        }
        let Some(source) = source else { return };
        // The request starts under the screen before the state changes: setting `committing`
        // makes the button that was pressed again, and a task started after that would belong to
        // the button's owner, which is gone.
        call(
            move |api| async move { api.import_commit(&source, &mapping, &name).await },
            move |result| {
                st.committing.set(false);
                match result {
                    // The server answers with the set still importing: its page shows the progress.
                    Ok(set) => nav::reset_to(Route::PromptSet { id: set.id }, Some(set.name)),
                    Err(e) => st.commit_error.set(Some(e)),
                }
            },
        );
        st.committing.set(true);
        st.commit_error.set(None);
    };

    // ---- what the controls say under themselves ----
    let hf_error = derived(move || {
        if st.tab.get() == SourceTab::HuggingFace && st.preview.with(Option::is_none) && st.preview_error.with(Option::is_some) {
            st.preview_error.get()
        } else {
            commit_field(st, "source.dataset")
        }
    });
    let file_error = derived(move || {
        if st.upload_error.with(Option::is_some) {
            return st.upload_error.get();
        }
        (st.tab.get() == SourceTab::File && st.preview.with(Option::is_none)).then(|| st.preview_error.get()).flatten()
    });
    let panel_error = derived(move || {
        if st.tab.get() == SourceTab::HuggingFace && st.info().is_none() { None } else { st.preview_error.get() }
    });
    let empty_hint = derived(move || hf_error.get().map(|_| "Fix the dataset above to see its prompts.".to_owned()));
    let offset_error = derived(move || st.offset_error().or_else(|| commit_field(st, "mapping.offset")));
    let limit_error = derived(move || st.limit_error().or_else(|| commit_field(st, "mapping.limit")));
    let name_error = derived(move || commit_field(st, "name"));

    // ---- the sections ----
    let busy_style = move |v: View| v.opacity(move || if st.committing.get() { 0.6_f32 } else { 1.0 }).pointer_events(move || if st.committing.get() { PointerEvents::None } else { PointerEvents::Auto });
    let source_section = move || {
        let fields = dynamic(move || -> AnyElement {
            match st.tab.get() {
                SourceTab::HuggingFace => hugging_face_fields(st, load, hf_error).into_element(),
                SourceTab::File => file_fields(st, pick_file, dropped, file_error).into_element(),
                SourceTab::Paste => paste_fields(st).into_element(),
            }
        });
        let summary = dynamic(move || {
            if !st.simple() || st.preview_error.with(Option::is_some) {
                return None;
            }
            let rows = st.preview.with(|p| p.as_ref().map(|p| import_summary(p, &st.summary_state())))?;
            (!rows.is_empty()).then(|| detected_summary(rows))
        });
        busy_style(view().gap(px(16.0)).min_w(px(0.0)).child(source_tabs(st)).child(fields).child(summary))
    };
    let preview_section = move || view().min_w(px(0.0)).child(preview_panel(st, refetch, panel_error, empty_hint));
    let mapping_section = move || {
        when(move || !st.simple() && st.preview.with(Option::is_some), move || {
            busy_style(
                view()
                    .gap(px(16.0))
                    .min_w(px(0.0))
                    .child(mapping_group(st))
                    .child(limits_group(st, offset_error, limit_error, submit))
                    .child(name_group(st, name_error, submit)),
            )
        })
    };
    let submit_section = move |fit: bool| {
        let error = dynamic(move || {
            st.commit_error.with(|e| e.as_ref().map(|e| e.message.clone())).map(|m| {
                body(text(m))
                    .text_color(token("negative-ink"))
                    .role(AccessibilityRole::Alert)
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(radius::XL))
                    .bg(token("negative-tint"))
            })
        });
        let action = dynamic(move || {
            let label = import_label(st.committing.get(), st.estimated());
            let gate = st.gate();
            let b = button(label).disabled(move || gate.blocked() || st.committing.get()).busy(move || st.committing.get(), "Starting…").on_press(submit);
            if fit { b.into_element() } else { b.full_width().into_element() }
        });
        let reason = dynamic(move || {
            if st.committing.get() {
                return None;
            }
            st.gate().reason().map(|r| crate::theme::text::meta(text(r)).text_color(muted_ink))
        });
        view().gap(px(8.0)).min_w(px(0.0)).items_start().child(error).child(action).child(reason)
    };

    let wide = move || window_width() >= TWO_COLUMNS_FROM;
    let layout = when(wide, move || {
        view()
            .flex_row()
            .items_start()
            .gap(px(40.0))
            .child(
                view()
                    .w(px(SETTINGS_COLUMN))
                    .flex_none()
                    .gap(px(32.0))
                    .child(source_section())
                    .child(mapping_section())
                    .child(submit_section(true)),
            )
            .child(view().flex_1().min_w(px(0.0)).child(preview_section()))
    })
    .otherwise(move || view().gap(px(32.0)).child(source_section()).child(preview_section()).child(mapping_section()));

    page("Import prompts")
        .wide()
        .actions(move || mode_toggle(st).into_element())
        .bottom_bar(move || {
            // From 1024 px the button is part of the settings column; below it floats.
            if wide() {
                view().into_element()
            } else {
                action_bar(vec![view().flex_1().min_w(px(0.0)).child(submit_section(false)).into_element()]).into_element()
            }
        })
        .bottom_bar_below_lg()
        .child(layout)
}

/// The path a dropped `file:` URL names (percent-encoding undone), when it is a local file.
#[cfg(not(target_arch = "wasm32"))]
fn file_path_of(uri: &str) -> Option<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // "file://host/path" has a host; "file:///path" has none.
    let path = rest.find('/').map(|i| &rest[i..])?;
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(std::path::PathBuf::from(String::from_utf8(out).ok()?))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_url_names_a_local_path() {
        assert_eq!(file_path_of("file:///tmp/a%20b.md"), Some(std::path::PathBuf::from("/tmp/a b.md")));
        assert_eq!(file_path_of("file:///tmp/%E2%9C%93.csv"), Some(std::path::PathBuf::from("/tmp/✓.csv")));
        assert_eq!(file_path_of("file:///tmp/100%.md"), Some(std::path::PathBuf::from("/tmp/100%.md")));
        assert_eq!(file_path_of("https://example.com/a.md"), None);
    }
}
