//! Settings (frontend/src/pages/settings.tsx): grouped forms at most 640 px wide. Hugging Face
//! (the token: add, replace, remove), Storage (the data folder), Appearance (Light, Dark, System)
//! and About (the icon, the version, how the app runs).

use nsh_core::prelude::*;

use crate::api::Settings;
use crate::data::{Load, call};
use crate::theme::appearance::{Choice, choice, set_choice};
use crate::theme::text::{body, meta, mono, title};
use crate::theme::{muted_ink, token};
use crate::ui::blocks::{bar, error_block};
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::choice::segmented_with_get;
use crate::ui::field::{form_group, form_item, form_row, text_field};
use crate::ui::icon;
use crate::ui::overlay::{Alert, alert, toast_success};
use crate::ui::page::{is_phone, page};
use crate::ui::tooltip::copy_button;

/// The Settings screen.
pub fn screen() -> impl IntoElement {
    let load = signal(Load::<Settings>::Loading);
    let reload = move || call(|api| async move { api.settings().await }, move |r| load.set(Load::from(r)));
    reload();
    let settings = move || load.with(|l| l.loaded().cloned());

    let content = dynamic(move || -> AnyElement {
        if let Some(message) = load.with(|l| l.error().map(str::to_owned)) {
            return view()
                .max_w(px(640.0))
                .child(error_block(
                    format!("Couldn’t load settings. {message}"),
                    Some(std::rc::Rc::new(move || {
                        load.set(Load::Loading);
                        reload();
                    })),
                ))
                .into_element();
        }
        view()
            .max_w(px(640.0))
            .gap(px(24.0))
            .child(hugging_face(load))
            .child(storage(settings))
            .child(appearance())
            .child(about(settings))
            .into_element()
    });
    page("Settings").child(content)
}

/// The Hugging Face token: what is saved, Add or Replace and Remove, and the form to paste one.
fn hugging_face(load: Signal<Load<Settings>>) -> impl IntoElement {
    let settings = move || load.with(|l| l.loaded().cloned());
    let editing = signal(false);
    let token_text = signal(String::new());
    let show = signal(false);
    let error = signal(None::<String>);
    let confirm_remove = signal(false);
    let saving = signal(false);
    let field = element_ref();

    // PUT the token ("" removes it).
    let write = move |value: String| {
        saving.set(true);
        let sent = !value.is_empty();
        call(
            move |api| async move { api.save_hf_token(&value).await },
            move |result| {
                saving.set(false);
                match result {
                    Ok(s) => {
                        load.set(Load::Loaded(s));
                        toast_success(if sent { "Hugging Face token saved." } else { "Hugging Face token removed." });
                        editing.set(false);
                        token_text.set(String::new());
                        confirm_remove.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e.fields.get("hf_token").cloned().unwrap_or(e.message)));
                        confirm_remove.set(false);
                    }
                }
            },
        );
    };
    let submit = move || {
        if !editing.get_untracked() || saving.get_untracked() {
            return;
        }
        let value = token_text.get_untracked().trim().to_owned();
        if value.is_empty() {
            error.set(Some("Paste a token that starts with hf_.".into()));
            field.focus();
            return;
        }
        error.set(None);
        write(value);
    };

    let status = dynamic(move || -> AnyElement {
        match settings() {
            None => bar(192.0, 16.0).into_element(),
            Some(s) if s.hf_token_from_env => view()
                .child(
                    body(text("Set by the "))
                        .child(mono(text("HF_TOKEN")))
                        .child(text(" environment variable"))
                        .child(s.hf_token_hint.map(|h| text(format!(" ({h})")).text_color(muted_ink))),
                )
                .child(meta(text("Set where the container starts.")).mt(px(2.0)))
                .into_element(),
            Some(s) if s.hf_token_set => body(text("A token is saved "))
                .child(mono(text(s.hf_token_hint.unwrap_or_default())).text_color(muted_ink))
                .into_element(),
            Some(_) => body(text("No token")).text_color(muted_ink).into_element(),
        }
    });
    let buttons = dynamic(move || {
        let s = settings()?;
        if s.hf_token_from_env || editing.get() {
            return None;
        }
        Some(
            view()
                .flex_row()
                .flex_wrap()
                .gap(px(8.0))
                .child(
                    button(if s.hf_token_set { "Replace token" } else { "Add token" })
                        .variant(Variant::Secondary)
                        .on_card()
                        .on_press(move || {
                            error.set(None);
                            editing.set(true);
                            let _ = request_animation_frame(move |_| field.focus());
                        }),
                )
                .child(s.hf_token_set.then(|| button("Remove token").variant(Variant::Ghost).on_press(move || confirm_remove.set(true)))),
        )
    });
    let token_row = form_row(
        "Token",
        view()
            .gap(px(6.0))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_x(px(12.0))
                    .gap_y(px(8.0))
                    .child(view().min_h(px(28.0)).min_w(px(0.0)).justify_center().child(status))
                    .child(buttons),
            )
            .child(dynamic(move || {
                error.get().filter(|_| !editing.get()).map(|m| body(text(m)).text_color(token("negative-ink")).role(AccessibilityRole::Alert))
            })),
    );

    let form_rows = dynamic(move || {
        editing.get().then(|| {
            let eye = dynamic(move || {
                let shown = show.get();
                Button::icon(if shown { icon::EYE_OFF } else { icon::EYE }, if shown { "Hide token" } else { "Show token" })
                    .variant(Variant::Ghost)
                    .size(Size::IconSm)
                    .on_press(move || show.update(|s| *s = !*s))
            });
            view()
                .child(
                    view()
                        .child(inset_rule())
                        .child(
                            form_row(
                                "Access token",
                                text_field(token_text)
                                    .placeholder("hf_012345…")
                                    .masked(move || !show.get())
                                    .mono()
                                    .invalid(move || error.with(Option::is_some))
                                    .on_change(move |_| error.set(None))
                                    .on_submit(submit)
                                    .element(&field)
                                    .trailing(eye),
                            )
                            .error(error),
                        ),
                )
                .child(
                    view().child(inset_rule()).child(
                        form_item(
                            view()
                                .flex_row()
                                .flex_wrap()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    button("Cancel").variant(Variant::Secondary).on_card().disabled(move || saving.get()).on_press(
                                        move || {
                                            editing.set(false);
                                            token_text.set(String::new());
                                            error.set(None);
                                        },
                                    ),
                                )
                                .child(button("Save token").busy(move || saving.get(), "Saving…").on_press(submit)),
                        ),
                    ),
                )
        })
    });

    let footnote = meta(text("For gated datasets and Hub pushes. ")).child(text_link_inline("Create a token", "https://huggingface.co/settings/tokens"));
    let group = form_group(vec![token_row.into_element(), form_rows.into_element()]).title("Hugging Face").footnote(footnote);

    let confirm = alert(
        confirm_remove,
        || Alert {
            title: "Remove the saved token?".into(),
            description: "Gated imports and Hub pushes stop working.".into(),
            action: "Remove token".into(),
            busy_label: "Removing…".into(),
            destructive: true,
        },
        move || saving.get(),
        || None,
        move || write(String::new()),
    );
    view().child(group).child(confirm)
}

/// A hairline 12 px in from each side, above a row added to a group.
fn inset_rule() -> impl IntoElement {
    view().absolute().top(px(0.0)).left(px(12.0)).right(px(12.0)).h(px(1.0)).bg(move || nsh_base::theme().border)
}

/// "Create a token": an underlined ink link in a 12 px footnote.
fn text_link_inline(label: &'static str, url: &'static str) -> impl IntoElement {
    text(label).text_color(move || nsh_base::theme().foreground).underline().role(AccessibilityRole::Link).on_press(move || {
        let _ = open_url(url);
    })
}

/// Where the data lives, and its copy button.
fn storage(settings: impl Fn() -> Option<Settings> + 'static) -> impl IntoElement {
    let dir = dynamic(move || -> AnyElement {
        match settings() {
            Some(s) => view()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .min_w(px(0.0))
                .child(mono(text(s.data_dir.clone())).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
                .child(copy_button(s.data_dir, "Copy data directory"))
                .into_element(),
            None => bar(224.0, 16.0).into_element(),
        }
    });
    form_group(vec![form_row("Data directory", dir).into_element()])
        .title("Storage")
        .footnote(meta(text("Back up this folder to keep your data.")))
}

/// Light, Dark or System.
fn appearance() -> impl IntoElement {
    let control = dynamic(move || {
        segmented_with_get(
            vec![(Choice::Light, "Light"), (Choice::Dark, "Dark"), (Choice::System, "System")],
            choice,
            set_choice,
            "Theme",
            is_phone(),
        )
    });
    form_group(vec![form_row("Theme", control).align_end().into_element()]).title("Appearance")
}

/// The icon, the name and the version; how the app runs.
fn about(settings: impl Fn() -> Option<Settings> + Copy + 'static) -> impl IntoElement {
    let version = dynamic(move || -> AnyElement {
        match settings() {
            Some(s) => body(text("Version\u{a0}")).text_color(muted_ink).child(mono(text(s.version)).text_color(move || nsh_base::theme().foreground)).into_element(),
            None => bar(96.0, 14.0).into_element(),
        }
    });
    let mode = dynamic(move || -> AnyElement {
        match settings() {
            Some(s) => body(text(match s.mode.as_str() {
                "desktop" => "Desktop app",
                "docker" => "Docker",
                _ => "Server",
            }))
            .text_color(muted_ink)
            .into_element(),
            None => bar(80.0, 16.0).into_element(),
        }
    });
    form_group(vec![
        form_item(
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .py(px(4.0))
                .child(image(icon::APP_ICON).w(px(40.0)).h(px(40.0)).flex_none())
                .child(view().gap(px(2.0)).min_w(px(0.0)).child(title(text("Stellarator"))).child(view().min_h(px(16.0)).justify_center().child(version))),
        )
        .into_element(),
        form_item(
            view()
                .min_h(px(28.0))
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(body(text("Runs as")))
                .child(mode),
        )
        .into_element(),
    ])
    .title("About")
}
