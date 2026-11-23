//! Add or edit one endpoint (frontend/src/components/endpoints/endpoint-form-dialog.tsx,
//! presets.tsx). A 576 px dialog: the preset chips (Add only), the grouped form (Name, Base URL,
//! API key with the eye button and "Get a key", Default seqs), the Headers disclosure, Test
//! connection with its result, then Cancel and Add endpoint (Save changes). The values survive a
//! failed save; the server's field messages land under their fields.

use std::collections::BTreeMap;
use std::rc::Rc;

use nsh_core::prelude::*;

use super::EditFocus;
use super::status::is_key_rejected;
use crate::api::types::{EndpointOut as Endpoint, EndpointTestIn, EndpointTestOut, Location};
use crate::api::{ApiError, EndpointPreset};
use crate::data::call;
use crate::format::{count, duration, int};
use crate::theme::muted_ink;
use crate::theme::text::{body, meta, mono, title};
use crate::ui::button::{Button, Size, Variant, button, external_link};
use crate::ui::chip::chip;
use crate::ui::choice::checkbox_with;
use crate::ui::field::{form_disclosure, form_group, form_item, form_row, text_field};
use crate::ui::icon;
use crate::ui::overlay::{dialog_footer, toast_success};
use crate::ui::status::{Tone, pill};

/// New runs start with this many seqs when the form has no other value.
const DEFAULT_SEQS: i64 = 16;

/// What the dialog edits: an endpoint (edit), or a new one from a preset or from nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Target {
    pub endpoint: Option<Endpoint>,
    pub preset: Option<EndpointPreset>,
    /// The field an edit opens on (`open_edit`).
    pub focus: EditFocus,
}

/// One header row: an id that stays with the row, its name and value.
#[derive(Debug, Clone, PartialEq)]
struct HeaderRow {
    id: u64,
    key: String,
    value: String,
}

/// The dialog's content for `target`. `presets` are the chips (Add only); `close` closes the
/// dialog; `saved` runs with the saved endpoint.
pub fn endpoint_form(
    target: Target,
    presets: Vec<EndpointPreset>,
    close: Rc<dyn Fn()>,
    saved: Rc<dyn Fn(Endpoint)>,
) -> impl IntoElement {
    let endpoint = target.endpoint.clone();
    let editing = endpoint.is_some();
    let preset = target.preset.clone();
    let has_saved_key = endpoint.as_ref().is_some_and(|e| e.has_api_key);
    let key_hint = endpoint.as_ref().and_then(|e| e.api_key_hint.clone()).unwrap_or_else(|| "set".into());

    let preset_id = signal(preset.as_ref().map(|p| p.id.clone()));
    let key_url = signal(preset.as_ref().filter(|p| p.needs_api_key).and_then(|p| p.api_key_url.clone()));
    let location = signal(preset.as_ref().map(|p| p.location).unwrap_or(Location::Local));
    let name = signal(endpoint.as_ref().map(|e| e.name.clone()).or(preset.as_ref().map(|p| p.name.clone())).unwrap_or_default());
    let base_url = signal(
        endpoint.as_ref().map(|e| e.base_url.clone()).or(preset.as_ref().map(|p| p.base_url.clone())).unwrap_or_default(),
    );
    let api_key = signal(String::new());
    let show_key = signal(false);
    let remove_key = signal(false);
    let seqs = signal(
        endpoint
            .as_ref()
            .map(|e| e.default_concurrency)
            .or(preset.as_ref().map(|p| p.default_concurrency))
            .unwrap_or(DEFAULT_SEQS)
            .to_string(),
    );
    let next_id = signal(0_u64);
    let rows_of = move |headers: &BTreeMap<String, String>| -> Vec<HeaderRow> {
        headers
            .iter()
            .map(|(k, v)| {
                let id = next_id.get_untracked() + 1;
                next_id.set(id);
                HeaderRow { id, key: k.clone(), value: v.clone() }
            })
            .collect()
    };
    let headers = signal(rows_of(
        &endpoint.as_ref().map(|e| e.headers.clone()).or(preset.as_ref().map(|p| p.headers.clone())).unwrap_or_default(),
    ));
    let headers_open = signal(!headers.get_untracked().is_empty());

    let err_name = signal(None::<String>);
    let err_url = signal(None::<String>);
    let err_key = signal(None::<String>);
    let err_seqs = signal(None::<String>);
    let form_error = signal(None::<String>);
    let test = signal(None::<EndpointTestOut>);
    let test_error = signal(None::<String>);
    let testing = signal(false);
    let saving = signal(false);

    let name_ref = element_ref();
    let url_ref = element_ref();
    let key_ref = element_ref();
    let seqs_ref = element_ref();

    let stale_test = move || {
        test.set(None);
        test_error.set(None);
    };
    let header_object = move || -> BTreeMap<String, String> {
        headers.with_untracked(|rows| {
            rows.iter().filter(|h| !h.key.trim().is_empty()).map(|h| (h.key.trim().to_owned(), h.value.clone())).collect()
        })
    };

    // A preset fills the form (Custom clears it) and focuses what still needs input.
    let pick = move |p: Option<EndpointPreset>| {
        preset_id.set(Some(p.as_ref().map(|p| p.id.clone()).unwrap_or_else(|| "custom".into())));
        key_url.set(p.as_ref().filter(|p| p.needs_api_key).and_then(|p| p.api_key_url.clone()));
        location.set(p.as_ref().map(|p| p.location).unwrap_or(Location::Local));
        name.set(p.as_ref().map(|p| p.name.clone()).unwrap_or_default());
        base_url.set(p.as_ref().map(|p| p.base_url.clone()).unwrap_or_default());
        seqs.set(p.as_ref().map(|p| p.default_concurrency).unwrap_or(DEFAULT_SEQS).to_string());
        api_key.set(String::new());
        let rows = rows_of(&p.as_ref().map(|p| p.headers.clone()).unwrap_or_default());
        let any = !rows.is_empty();
        headers.set(rows);
        headers_open.set(any);
        for e in [err_name, err_url, err_key, err_seqs, form_error] {
            e.set(None);
        }
        stale_test();
        let needs_key = p.as_ref().is_some_and(|p| p.needs_api_key);
        let _ = request_animation_frame(move |_| if needs_key { key_ref.focus() } else { name_ref.focus() });
    };

    // Open with the key focused when the preset needs one; an edit with its name focused and
    // selected, as the web UI's dialog focuses its first field.
    if editing && target.focus == EditFocus::Key {
        let _ = request_animation_frame(move |_| key_ref.focus());
    } else if editing && target.focus == EditFocus::Url {
        let _ = request_animation_frame(move |_| url_ref.focus());
    } else if preset.as_ref().is_some_and(|p| p.needs_api_key) {
        let _ = request_animation_frame(move |_| key_ref.focus());
    } else if editing {
        let len = name.get_untracked().encode_utf16().count() as u32;
        let _ = request_animation_frame(move |_| {
            name_ref.focus();
            name_ref.set_selection(0, len);
        });
    }

    let endpoint_id = endpoint.as_ref().map(|e| e.id);
    let run_test = move || {
        if testing.get_untracked() {
            return;
        }
        testing.set(true);
        let body = EndpointTestIn {
            base_url: base_url.get_untracked().trim().to_owned(),
            api_key: Some(api_key.get_untracked()).filter(|k| !k.is_empty()),
            headers: header_object(),
            endpoint_id: endpoint_id.filter(|_| !remove_key.get_untracked()),
        };
        call(
            move |api| async move { api.test_endpoint(&body).await },
            move |result| {
                testing.set(false);
                match result {
                    Ok(r) => {
                        test.set(Some(r));
                        test_error.set(None);
                    }
                    Err(e) => {
                        test.set(None);
                        test_error.set(Some(e.message));
                    }
                }
            },
        );
    };

    let show_errors = move |fields: &BTreeMap<String, String>| {
        let get = |f: &str| fields.get(f).cloned().filter(|m| !m.is_empty());
        err_name.set(get("name"));
        err_url.set(get("base_url"));
        err_key.set(get("api_key"));
        err_seqs.set(get("default_concurrency"));
        let first = [("name", name_ref), ("base_url", url_ref), ("api_key", key_ref), ("default_concurrency", seqs_ref)]
            .into_iter()
            .find(|(f, _)| get(f).is_some());
        if let Some((_, r)) = first {
            let _ = request_animation_frame(move |_| r.focus());
        }
    };

    let close_after = close.clone();
    let submit = move || {
        if saving.get_untracked() {
            return;
        }
        form_error.set(None);
        let mut errors = BTreeMap::new();
        let url = base_url.get_untracked().trim().to_owned();
        if name.get_untracked().trim().is_empty() {
            errors.insert("name".to_owned(), "Enter a name so you can tell this endpoint apart.".to_owned());
        }
        if url.is_empty() {
            errors.insert("base_url".to_owned(), "Enter the base URL of the server, for example http://192.168.1.50:8000/v1.".to_owned());
        } else if !starts_with_http(&url) {
            errors.insert("base_url".to_owned(), "Base URL must start with http:// or https://.".to_owned());
        }
        let n = seqs.get_untracked().trim().parse::<i64>().ok().filter(|n| (1..=4096).contains(n));
        if n.is_none() {
            errors.insert("default_concurrency".to_owned(), "Enter a whole number of seqs from 1 to 4096.".to_owned());
        }
        show_errors(&errors);
        let Some(n) = n.filter(|_| errors.is_empty()) else { return };
        let mut body = serde_json::json!({
            "name": name.get_untracked().trim(),
            "base_url": url,
            "default_concurrency": n,
            "headers": header_object(),
        });
        if !editing {
            body["location"] = serde_json::json!(location.get_untracked());
        }
        let key = api_key.get_untracked();
        if !key.is_empty() {
            body["api_key"] = serde_json::json!(key);
        } else if remove_key.get_untracked() {
            body["api_key"] = serde_json::json!("");
        }
        saving.set(true);
        let close = close_after.clone();
        let saved = saved.clone();
        call(
            move |api| async move {
                match endpoint_id {
                    Some(id) => api.update_endpoint(id, &body).await,
                    None => api.create_endpoint(&body).await,
                }
            },
            move |result: Result<Endpoint, ApiError>| {
                saving.set(false);
                match result {
                    Ok(ep) => {
                        toast_success(if ep.last_ok == Some(true) {
                            format!("Saved {}. It serves {}.", ep.name, count(ep.models.len() as i64, "model", "models"))
                        } else {
                            format!("Saved {}, but it is unreachable right now.", ep.name)
                        });
                        saved(ep);
                        close();
                    }
                    Err(e) if !e.fields.is_empty() => {
                        let mapped = e.fields.iter().map(|(k, v)| (k.split('.').next().unwrap_or(k).to_owned(), v.clone())).collect();
                        show_errors(&mapped);
                    }
                    Err(e) => form_error.set(Some(e.message)),
                }
            },
        );
    };

    // The preset chips.
    let preset_row = (!editing && !presets.is_empty()).then(|| {
        let mut chips: Vec<AnyElement> = presets
            .iter()
            .map(|p| {
                let id = p.id.clone();
                let p = p.clone();
                chip(p.name.clone())
                    .selected(move || preset_id.with(|v| v.as_deref() == Some(id.as_str())))
                    .on_press(move || pick(Some(p.clone())))
                    .into_element()
            })
            .collect();
        chips.push(
            chip("Custom")
                .selected(move || preset_id.with(|v| v.as_deref() == Some("custom")))
                .on_press(move || pick(None))
                .into_element(),
        );
        view().flex_row().flex_wrap().gap(px(6.0)).role(AccessibilityRole::Group).label("Preset").children(chips)
    });

    // The rows of the form.
    let name_row = form_row(
        "Name",
        text_field(name)
            .placeholder("DGX Spark")
            .invalid(move || err_name.with(Option::is_some))
            .on_change(move |_| err_name.set(None))
            .element(&name_ref),
    )
    .error(err_name);
    let url_row = form_row(
        "Base URL",
        text_field(base_url)
            .placeholder("http://192.168.1.50:8000/v1")
            .mono()
            .invalid(move || err_url.with(Option::is_some))
            .on_change(move |_| {
                err_url.set(None);
                stale_test();
            })
            .element(&url_ref),
    )
    .error(err_url)
    .hint_when(move || {
        is_local(&base_url.get()).then(|| SharedString::from("From Docker, this machine is http://host.docker.internal:PORT/v1."))
    });
    let eye = dynamic(move || {
        let shown = show_key.get();
        Button::icon(if shown { icon::EYE_OFF } else { icon::EYE }, if shown { "Hide API key" } else { "Show API key" })
            .variant(Variant::Ghost)
            .size(Size::IconSm)
            .on_press(move || show_key.update(|s| *s = !*s))
    });
    let key_field = text_field(api_key)
        .masked(move || !show_key.get())
        .mono()
        .disabled(move || remove_key.get())
        .invalid(move || err_key.with(Option::is_some))
        .on_change(move |_| {
            err_key.set(None);
            stale_test();
        })
        .element(&key_ref)
        .trailing(eye);
    let key_field = if has_saved_key { key_field } else { key_field.placeholder("sk-012345…") };
    let key_extras = view()
        .gap(px(6.0))
        .child(dynamic(move || {
            let url = key_url.get()?;
            (err_key.get().is_none() && !has_saved_key).then(|| external_link("Get a key", url).into_element())
        }))
        .child(dynamic(move || {
            (err_key.get().is_none() && has_saved_key).then(|| {
                if remove_key.get() {
                    meta(text("The saved key is removed when you save.")).into_element()
                } else {
                    meta(text("Saved key "))
                        .child(mono(text(key_hint.clone())).text_color(muted_ink))
                        .child(text(". Leave blank to keep it."))
                        .into_element()
                }
            })
        }))
        .child(has_saved_key.then(|| {
            checkbox_with("Remove the saved key", move || remove_key.get(), move |on| {
                remove_key.set(on);
                if on {
                    api_key.set(String::new());
                }
                stale_test();
            })
        }));
    let key_row = form_row("API key", view().gap(px(6.0)).child(key_field).child(key_extras))
        .optional_when(move || key_url.get().is_none())
        .error(err_key);
    let seqs_row = form_row(
        "Default seqs",
        text_field(seqs)
            .numeric()
            .width(96.0)
            .invalid(move || err_seqs.with(Option::is_some))
            .on_change(move |_| err_seqs.set(None))
            .element(&seqs_ref),
    )
    .hint("Prefilled on new runs.")
    .align_end()
    .error(err_seqs);

    let group = form_group(vec![name_row.into_element(), url_row.into_element(), key_row.into_element(), seqs_row.into_element()])
        .in_dialog();

    // The headers: two mono fields and a remove button per row, then Add header.
    let header_rows = move || {
        let mut rows: Vec<AnyElement> = headers
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, h)| header_row(i, h, headers, stale_test).into_element())
            .collect();
        rows.push(
            form_item(
                button("Add header")
                    .variant(Variant::Ghost)
                    .size(Size::Sm)
                    .leading(icon::PLUS)
                    .on_press(move || {
                        let id = next_id.get_untracked() + 1;
                        next_id.set(id);
                        headers.update(|rows| rows.push(HeaderRow { id, key: String::new(), value: String::new() }));
                    }),
            )
            .ml(px(-6.0))
            .into_element(),
        );
        rows
    };
    let disclosure = form_disclosure(
        "Headers",
        move || {
            let n = headers.with(Vec::len);
            if n > 0 { count(n as i64, "header", "headers") } else { "Optional".to_owned() }
        },
        headers_open,
        header_rows,
        true,
    );

    // Test connection and its result.
    let test_section = view()
        .gap(px(8.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(12.0))
                .child(
                    button("Test connection")
                        .variant(Variant::Secondary)
                        .busy(move || testing.get(), "Testing…")
                        .disabled(move || base_url.with(|u| u.trim().is_empty()))
                        .on_press(run_test),
                )
                .child(dynamic(move || {
                    base_url.with(|u| u.trim().is_empty()).then(|| meta(text("Enter a base URL to test it.")))
                })),
        )
        .child(dynamic(move || test_result(test.get(), test_error.get())));

    let content = scroll().flex_shrink(1.0).min_h(px(0.0)).mx(px(-16.0)).child(
        view()
            .px(px(16.0))
            .py(px(4.0))
            .gap(px(20.0))
            .child(preset_row)
            .child(group)
            .child(disclosure)
            .child(test_section)
            .child(dynamic(move || form_error.get().map(|m| body(text(m)).text_color(crate::theme::token("negative-ink")).role(AccessibilityRole::Alert)))),
    );

    let cancel_close = close.clone();
    let submit_press = submit.clone();
    view()
        .min_h(px(0.0))
        .flex_shrink(1.0)
        .child(title(text(if editing { "Edit endpoint" } else { "Add endpoint" })).role(AccessibilityRole::Header).pr(px(32.0)).pb(px(16.0)))
        .child(content)
        .child(view().pt(px(16.0)).flex_none().child(dialog_footer(vec![
            button("Cancel").variant(Variant::Secondary).disabled(move || saving.get()).on_press(move || cancel_close()),
            button(if editing { "Save changes" } else { "Add endpoint" })
                .busy(move || saving.get(), "Saving…")
                .on_press(move || submit_press()),
        ])))
}

/// One header: its name and value in mono, and the remove button.
fn header_row(index: usize, row: HeaderRow, headers: Signal<Vec<HeaderRow>>, stale_test: impl Fn() + Copy + 'static) -> impl IntoElement {
    let id = row.id;
    let key = signal(row.key);
    let value = signal(row.value);
    let n = index + 1;
    form_item(
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(
                view().flex_1().min_w(px(0.0)).child(
                    text_field(key)
                        .mono()
                        .placeholder("X-Org-Id")
                        .accessibility_label(format!("Header {n} name"))
                        .on_change(move |t| {
                            let t = t.to_owned();
                            headers.update(|rows| rows.iter_mut().filter(|r| r.id == id).for_each(|r| r.key = t.clone()));
                            stale_test();
                        }),
                ),
            )
            .child(
                view().flex_1().min_w(px(0.0)).child(
                    text_field(value)
                        .mono()
                        .placeholder("value")
                        .accessibility_label(format!("Header {n} value"))
                        .on_change(move |t| {
                            let t = t.to_owned();
                            headers.update(|rows| rows.iter_mut().filter(|r| r.id == id).for_each(|r| r.value = t.clone()));
                            stale_test();
                        }),
                ),
            )
            .child(
                Button::icon(icon::X, format!("Remove header {n}"))
                    .variant(Variant::Ghost)
                    .size(Size::IconSm)
                    .on_press(move || headers.update(|rows| rows.retain(|r| r.id != id))),
            ),
    )
}

/// What Test connection found: Connected, the latency and the models; or what failed.
fn test_result(test: Option<EndpointTestOut>, error: Option<String>) -> Option<AnyElement> {
    let negative = || crate::theme::token("negative-ink")();
    match (test, error) {
        (Some(t), _) if t.ok => {
            let mut models = t.models.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
            if t.models.len() > 4 {
                models.push_str(&format!(", and {} more", int(t.models.len() as i64 - 4)));
            }
            Some(
                view()
                    .gap(px(4.0))
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_x(px(6.0))
                            .child(pill(Tone::Positive, "Connected"))
                            .child(
                                body(text(format!(
                                    "in {} · {}",
                                    duration(t.latency_ms.map(|m| m as f64)),
                                    count(t.models.len() as i64, "model", "models")
                                )))
                                .text_color(muted_ink)
                                .tabular_nums(),
                            ),
                    )
                    .child((!t.models.is_empty()).then(|| mono(text(models)).text_color(muted_ink)))
                    .into_element(),
            )
        }
        (Some(t), _) => Some(
            view()
                .gap(px(4.0))
                .items_start()
                .child(pill(Tone::Negative, if is_key_rejected(t.error.as_deref()) { "Key rejected" } else { "Unreachable" }))
                .child(body(text(t.error.unwrap_or_else(|| "The server did not answer.".into()))).text_color(negative))
                .into_element(),
        ),
        (None, Some(e)) => Some(body(text(e)).text_color(negative).into_element()),
        (None, None) => None,
    }
}

/// Whether `url` starts with http:// or https://, in any case.
fn starts_with_http(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// A base URL on this machine (localhost or 127.0.0.1), which a server in Docker cannot reach.
fn is_local(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("http://").or_else(|| lower.strip_prefix("https://"));
    rest.is_some_and(|r| {
        ["localhost", "127.0.0.1"].iter().any(|h| {
            r.strip_prefix(h).is_some_and(|after| after.is_empty() || !after.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_urls_get_the_docker_note() {
        assert!(is_local("http://localhost:8000/v1"));
        assert!(is_local("https://127.0.0.1"));
        // As the web UI's `\b`: a dot ends the host name, a letter does not.
        assert!(is_local("http://localhost.example.com/v1"));
        assert!(!is_local("http://localhostx:1"));
        assert!(!is_local("http://192.168.1.50:8000/v1"));
        assert!(starts_with_http("HTTPS://x"));
        assert!(!starts_with_http("ftp://x"));
    }
}
