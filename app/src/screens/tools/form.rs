//! Add MCP servers, or edit one: a 576 px dialog.
//!
//! - Adding opens on Paste config: a mono text area for the JSON of Claude Desktop, Cursor,
//!   Windsurf or VS Code, the servers it found under it, and Import, which adds each server and
//!   checks it. The answer replaces the text area: each server added with its state after the check
//!   (and a note when it was renamed), then each entry not added with the reason.
//! - The Form segment (and Edit) is the grouped form: the name, the transport (stdio or HTTP), then
//!   the command and its args with the environment for stdio, or the URL with the headers for
//!   HTTP. Saved env values and headers show their last characters, with Replace and Remove. A save
//!   checks the server as well.
//!
//! The dialog's busy flag, the pasted text and the import's answer live beside its `open` (the
//! screen's [`DialogState`]), so a request's answer always finds them; the form's own values and
//! errors are written with `try_set`.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::logic::{
    FormField, Pasted, SecretRow, ServerForm, check_form, import_outcome, import_title, join_args, pasted_line, pasted_servers, saved_rows,
    server_state, stray_notes, to_create, to_patch,
};
use crate::api::types::{McpImportOut, McpServerOut as Server, McpStatus, McpTransport};
use crate::api::ApiError;
use crate::data::call;
use crate::format::count;
use crate::theme::text::{body, label, meta, mono, title};
use crate::theme::{muted_ink, radius, token};
use crate::ui::blocks::error_block;
use crate::ui::button::{Button, Size, Variant, button};
use crate::ui::choice::segmented;
use crate::ui::field::{form_disclosure, form_group, form_item, form_row, text_area, text_field};
use crate::ui::icon;
use crate::ui::material::dialog_muted;
use crate::ui::overlay::{dialog_footer, toast_error, toast_info, toast_success};
use crate::ui::status::pill;

/// The text area's placeholder: a short config in the most common shape.
pub const EXAMPLE: &str = "{\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"npx\",\n      \"args\": [\"-y\", \"@modelcontextprotocol/server-github\"],\n      \"env\": { \"GITHUB_TOKEN\": \"ghp_…\" }\n    }\n  }\n}";

/// Where configs come from, under the text area.
const SOURCES: &str = "From Claude Desktop (claude_desktop_config.json), Cursor (mcp.json), VS Code (.vscode/mcp.json) or Windsurf. Each server is checked as it is added.";

/// Why stdio is not offered, when the server does not allow it.
pub const STDIO_OFF: &str = "Stdio servers are off here: they run commands on the server, which has no sign-in. The operator can allow them with STELLARATOR_ALLOW_STDIO_MCP=1. HTTP servers work.";

/// How the Add dialog takes a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Paste,
    Form,
}

/// The dialog's state that outlives its content (beside its `open`).
#[derive(Clone, Copy)]
pub struct DialogState {
    pub open: Signal<bool>,
    /// The server edited; `None` adds.
    pub target: Signal<Option<Server>>,
    pub mode: Signal<Mode>,
    /// A save or an import on its way: the dialog cannot close until it answers.
    pub busy: Signal<bool>,
    pub paste: Signal<String>,
    pub import_error: Signal<Option<String>>,
    /// The last import's answer, shown in place of the text area.
    pub result: Signal<Option<McpImportOut>>,
}

impl DialogState {
    pub fn new() -> Self {
        DialogState {
            open: signal(false),
            target: signal(None),
            mode: signal(Mode::Paste),
            busy: signal(false),
            paste: signal(String::new()),
            import_error: signal(None),
            result: signal(None),
        }
    }

    /// Opens Add on `mode`, fresh.
    pub fn add(&self, mode: Mode) {
        self.target.set(None);
        self.mode.set(mode);
        self.result.set(None);
        self.import_error.set(None);
        self.open.set(true);
    }

    /// Opens Edit for `server`.
    pub fn edit(&self, server: Server) {
        self.target.set(Some(server));
        self.mode.set(Mode::Form);
        self.result.set(None);
        self.open.set(true);
    }
}

/// Imports the pasted text (each server checked), then shows the answer in the dialog (opening it
/// when the empty page imported) and runs `imported`.
pub fn import(state: DialogState, imported: Rc<dyn Fn()>) {
    if state.busy.get_untracked() {
        return;
    }
    let text = state.paste.get_untracked();
    if text.trim().is_empty() {
        return;
    }
    state.import_error.set(None);
    state.busy.set(true);
    let body = crate::api::types::McpImportIn { text, check: true };
    call(move |api| async move { api.import_mcp_servers(&body).await }, move |result| {
        state.busy.set(false);
        match result {
            Ok(out) => {
                if !out.created.is_empty() {
                    state.paste.set(String::new());
                }
                state.target.set(None);
                state.result.set(Some(out));
                state.open.set(true);
                imported();
            }
            Err(e) => state.import_error.set(Some(e.fields.values().next().cloned().unwrap_or(e.message))),
        }
    });
}

/// The form's values and errors, made with the dialog's content.
#[derive(Clone, Copy)]
struct FormState {
    name: Signal<String>,
    transport: Signal<McpTransport>,
    command: Signal<String>,
    args: Signal<String>,
    url: Signal<String>,
    env: Signal<Vec<SecretRow>>,
    headers: Signal<Vec<SecretRow>>,
    env_open: Signal<bool>,
    headers_open: Signal<bool>,
    next_id: Signal<u64>,
    /// Each field's error, in `FormField` order.
    errors: [Signal<Option<String>>; 6],
    form_error: Signal<Option<String>>,
    name_ref: ElementRef,
    command_ref: ElementRef,
    url_ref: ElementRef,
}

impl FormState {
    fn new(server: Option<&Server>, stdio_allowed: bool) -> Self {
        let env = server.map(|s| saved_rows(&s.env, 1)).unwrap_or_default();
        let headers = server.map(|s| saved_rows(&s.headers, 1000)).unwrap_or_default();
        let transport = server.map(|s| s.transport).unwrap_or(if stdio_allowed { McpTransport::Stdio } else { McpTransport::Http });
        FormState {
            name: signal(server.map(|s| s.name.clone()).unwrap_or_default()),
            transport: signal(transport),
            command: signal(server.and_then(|s| s.command.clone()).unwrap_or_default()),
            args: signal(server.map(|s| join_args(&s.args)).unwrap_or_default()),
            url: signal(server.and_then(|s| s.url.clone()).unwrap_or_default()),
            env_open: signal(!env.is_empty()),
            headers_open: signal(!headers.is_empty()),
            env: signal(env),
            headers: signal(headers),
            next_id: signal(2000),
            errors: std::array::from_fn(|_| signal(None)),
            form_error: signal(None),
            name_ref: element_ref(),
            command_ref: element_ref(),
            url_ref: element_ref(),
        }
    }

    fn snapshot(&self) -> ServerForm {
        ServerForm {
            name: self.name.get_untracked(),
            transport: self.transport.get_untracked(),
            command: self.command.get_untracked(),
            args: self.args.get_untracked(),
            env: self.env.get_untracked(),
            url: self.url.get_untracked(),
            headers: self.headers.get_untracked(),
        }
    }

    /// The signal of `field`'s error.
    fn err(&self, field: FormField) -> Signal<Option<String>> {
        self.errors[field as usize]
    }

    /// `field`'s error (reactive).
    fn error(&self, field: FormField) -> Option<String> {
        self.err(field).get()
    }

    fn clear(&self, field: FormField) {
        if self.err(field).with_untracked(Option::is_some) {
            self.err(field).set(None);
        }
    }

    /// Shows `found` and focuses the first field with an error that has one.
    fn show(&self, found: Vec<(FormField, String)>) {
        let first = found.iter().map(|(f, _)| *f).min();
        if found.iter().any(|(f, _)| *f == FormField::Env) {
            self.env_open.try_set(true);
        }
        if found.iter().any(|(f, _)| *f == FormField::Headers) {
            self.headers_open.try_set(true);
        }
        for (i, e) in self.errors.iter().enumerate() {
            e.try_set(found.iter().find(|(f, _)| *f as usize == i).map(|(_, m)| m.clone()));
        }
        let target = match first {
            Some(FormField::Name) => Some(self.name_ref),
            Some(FormField::Command | FormField::Args) => Some(self.command_ref),
            Some(FormField::Url) => Some(self.url_ref),
            _ => None,
        };
        if let Some(r) = target {
            let _ = request_animation_frame(move |_| r.focus());
        }
    }
}

/// What a save says once the server answered, after its check: "Added github: 12 tools." and
/// whether it is good news.
pub fn saved_message(server: &Server, created: bool) -> (String, bool) {
    let verb = if created { "Added" } else { "Saved" };
    let state = server_state(server);
    match (&server.blocked, server.status) {
        (Some(_), _) => (format!("{verb} {}. Stdio servers are off here, so runs can’t use it.", server.name), false),
        (None, McpStatus::Ok) => (format!("{verb} {}: {}.", server.name, count(server.tools.len() as i64, "tool", "tools")), true),
        (None, McpStatus::Error) => (
            format!("{verb} {}, but the check failed: {}", server.name, state.reason.unwrap_or_else(|| "no answer.".to_owned())),
            false,
        ),
        (None, McpStatus::Unchecked) => (format!("{verb} {}.", server.name), true),
    }
}

/// The dialog's content: the result of an import, the form (Edit, or Add on Form), or the paste box.
pub fn server_dialog(
    state: DialogState,
    stdio_allowed: bool,
    imported: Rc<dyn Fn()>,
    saved: Rc<dyn Fn(Server)>,
) -> impl IntoElement {
    let editing = state.target.get_untracked();
    let fs = FormState::new(editing.as_ref(), stdio_allowed);
    let shape = computed(move || (state.result.with(Option::is_some), state.mode.get()));
    dynamic(move || -> AnyElement {
        let (has_result, mode) = shape.get();
        let editing = editing.clone();
        untracked(|| {
            if has_result {
                return result_view(state).into_element();
            }
            if editing.is_some() || mode == Mode::Form {
                return form_view(state, fs, editing, stdio_allowed, saved.clone()).into_element();
            }
            paste_view(state, stdio_allowed, imported.clone()).into_element()
        })
    })
}

/// The title row and, while adding, the Paste config | Form control under it.
fn header(state: DialogState, words: String, adding: bool) -> impl IntoElement {
    view()
        .gap(px(16.0))
        .pb(px(16.0))
        .child(title(text(words)).role(AccessibilityRole::Header).pr(px(32.0)))
        .child(adding.then(|| segmented(vec![(Mode::Paste, "Paste config"), (Mode::Form, "Form")], state.mode, "How to add", true)))
}

/// The scrolling middle of the dialog, its edges at the dialog's.
fn middle(content: impl IntoElement) -> impl IntoElement {
    scroll().flex_shrink(1.0).min_h(px(0.0)).mx(px(-16.0)).child(view().px(px(16.0)).py(px(4.0)).gap(px(16.0)).child(content))
}

fn footer(buttons: Vec<Button>) -> impl IntoElement {
    view().pt(px(16.0)).flex_none().child(dialog_footer(buttons))
}

fn frame(header: impl IntoElement, content: impl IntoElement, buttons: Vec<Button>) -> View {
    view().min_h(px(0.0)).flex_shrink(1.0).child(header).child(middle(content)).child(footer(buttons))
}

/// A sentence in the negative ink.
fn negative(words: String) -> Text {
    body(text(words)).text_color(token("negative-ink")).role(AccessibilityRole::Alert)
}

// ------------------------------------------------------------------ paste

/// The paste box, what it found, where configs come from, and Import.
pub fn paste_box(state: DialogState, stdio_allowed: bool, element: Option<ElementRef>, on_submit: Rc<dyn Fn()>) -> impl IntoElement {
    let _ = on_submit;
    let mut area = text_area(state.paste)
        .mono_body()
        .height(208.0)
        .placeholder(EXAMPLE)
        .accessibility_label("MCP config (JSON)")
        .invalid(move || state.import_error.with(Option::is_some))
        .on_change(move |_| {
            if state.import_error.with_untracked(Option::is_some) {
                state.import_error.set(None);
            }
        });
    if let Some(e) = element {
        area = area.element(&e);
    }
    let found = computed(move || state.paste.with(|t| pasted_line(&pasted_servers(t))));
    view()
        .gap(px(8.0))
        .child(area)
        .child(dynamic(move || {
            if let Some(message) = state.import_error.get() {
                return Some(negative(message).into_element());
            }
            found.get().map(|(line, problem)| {
                meta(text(line)).text_color(move || if problem { token("coral-ink")() } else { muted_ink() }).into_element()
            })
        }))
        .child(meta(text(SOURCES)))
        .child((!stdio_allowed).then(|| meta(text(STDIO_OFF)).text_color(token("coral-ink"))))
}

/// Whether Import has something to send.
pub fn can_import(state: DialogState) -> bool {
    state.paste.with(|t| !matches!(pasted_servers(t), Pasted::Empty))
}

fn paste_view(state: DialogState, stdio_allowed: bool, imported: Rc<dyn Fn()>) -> impl IntoElement {
    let area = element_ref();
    let _ = request_animation_frame(move |_| area.focus());
    let go: Rc<dyn Fn()> = {
        let imported = imported.clone();
        Rc::new(move || import(state, imported.clone()))
    };
    let g = go.clone();
    frame(
        header(state, "Add MCP servers".to_owned(), true),
        paste_box(state, stdio_allowed, Some(area), go),
        vec![
            button("Cancel").variant(Variant::Secondary).disabled(move || state.busy.get()).on_press(move || state.open.set(false)),
            button("Import")
                .busy(move || state.busy.get(), "Adding and checking…")
                .disabled(move || !can_import(state))
                .on_press(move || g()),
        ],
    )
}

// ------------------------------------------------------------------ result

/// One line of the import's outcome: the name, the state and what it says.
fn outcome_row(name: String, state: super::logic::State, detail: String, index: usize) -> AnyElement {
    view()
        .px(px(12.0))
        .py(px(10.0))
        .gap(px(2.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_x(px(12.0))
                .gap_y(px(2.0))
                .child(label(text(name)).number_of_lines(1).flex_shrink(1.0).min_w(px(0.0)))
                .child(pill(state.tone, state.word)),
        )
        .child((!detail.is_empty()).then(|| meta(text(detail)).number_of_lines(3)))
        .child((index > 0).then(|| {
            view().absolute().top(px(0.0)).left(px(12.0)).right(px(12.0)).h(px(1.0)).bg(move || theme().border)
        }))
        .into_element()
}

fn result_view(state: DialogState) -> impl IntoElement {
    let out = state.result.get_untracked().unwrap_or(McpImportOut { created: Vec::new(), errors: Vec::new(), notes: Vec::new() });
    let (created, failed) = import_outcome(&out);
    let rows: Vec<AnyElement> = created
        .into_iter()
        .chain(failed)
        .enumerate()
        .map(|(i, o)| outcome_row(o.name, o.state, o.detail, i))
        .collect();
    let empty = rows.is_empty();
    let notes = stray_notes(&out);
    let content = view()
        .gap(px(12.0))
        .child((!empty).then(|| view().rounded(px(radius::MD)).bg(dialog_muted).children(rows)))
        .child(empty.then(|| body(text("The config had no servers to add.")).text_color(muted_ink)))
        .children(notes.into_iter().map(|n| meta(text(n)).into_element()).collect::<Vec<_>>());
    frame(
        view().pb(px(16.0)).child(title(text(import_title(&out))).role(AccessibilityRole::Header).pr(px(32.0))),
        content,
        vec![
            button("Paste more").variant(Variant::Secondary).on_press(move || {
                state.result.set(None);
                state.mode.set(Mode::Paste);
            }),
            button("Done").on_press(move || state.open.set(false)),
        ],
    )
}

// ------------------------------------------------------------------ form

fn form_view(state: DialogState, fs: FormState, editing: Option<Server>, stdio_allowed: bool, saved: Rc<dyn Fn(Server)>) -> impl IntoElement {
    let adding = editing.is_none();
    let saved_server = editing.clone();
    let submit: Rc<dyn Fn()> = Rc::new(move || {
        if state.busy.get_untracked() {
            return;
        }
        fs.form_error.set(None);
        let form = fs.snapshot();
        let found = check_form(&form);
        if !found.is_empty() {
            fs.show(found);
            return;
        }
        fs.show(Vec::new());
        state.busy.set(true);
        let saved = saved.clone();
        let done = move |result: Result<Server, ApiError>| {
            state.busy.set(false);
            match result {
                Ok(server) => {
                    let (message, good) = saved_message(&server, adding);
                    if good {
                        toast_success(message);
                    } else if server.blocked.is_some() {
                        toast_info(message);
                    } else {
                        toast_error(message);
                    }
                    saved(server);
                    state.open.set(false);
                }
                Err(e) if !e.fields.is_empty() => {
                    let mut found = Vec::new();
                    let mut other = Vec::new();
                    for (path, message) in &e.fields {
                        match FormField::from_server(path) {
                            Some(f) => found.push((f, message.clone())),
                            None => other.push(message.clone()),
                        }
                    }
                    fs.show(found);
                    fs.form_error.try_set((!other.is_empty()).then(|| other.join(" ")));
                }
                Err(e) => {
                    fs.form_error.try_set(Some(e.message));
                }
            }
        };
        match saved_server.clone() {
            None => {
                let body = to_create(&form);
                call(
                    move |api| async move {
                        let server = api.create_mcp_server(&body).await?;
                        let id = server.id;
                        Ok(api.check_mcp_server(id).await.unwrap_or(server))
                    },
                    done,
                );
            }
            Some(saved) => {
                let id = saved.id;
                let body = to_patch(&form, &saved);
                call(
                    move |api| async move {
                        let server = api.update_mcp_server(id, &body).await?;
                        Ok(api.check_mcp_server(id).await.unwrap_or(server))
                    },
                    done,
                );
            }
        }
    });

    // Focus the name: an edit's selected, as the endpoint form does.
    let name_ref = fs.name_ref;
    let len = fs.name.get_untracked().encode_utf16().count() as u32;
    let _ = request_animation_frame(move |_| {
        name_ref.focus();
        if len > 0 {
            name_ref.set_selection(0, len);
        }
    });

    let s1 = submit.clone();
    let enter = move || {
        let s = s1.clone();
        move || s()
    };
    let name_row = form_row(
        "Name",
        text_field(fs.name)
            .mono()
            .placeholder("github")
            .invalid(move || fs.error(FormField::Name).is_some())
            .on_change(move |_| fs.clear(FormField::Name))
            .on_submit(enter())
            .element(&fs.name_ref),
    )
    .hint("Letters, digits, _ and -. Clashing tool names are sent as name__tool.")
    .error(fs.err(FormField::Name));
    let transport_row = form_row(
        "Transport",
        segmented(vec![(McpTransport::Stdio, "stdio"), (McpTransport::Http, "HTTP")], fs.transport, "Transport", false),
    )
    .align_end()
    .hint_when(move || {
        (fs.transport.get() == McpTransport::Stdio && !stdio_allowed).then(|| SharedString::from(STDIO_OFF))
    });
    let e2 = enter.clone();
    let transport_rows = move || -> Vec<AnyElement> {
        match fs.transport.get() {
            McpTransport::Stdio => vec![
                form_row(
                    "Command",
                    text_field(fs.command)
                        .mono()
                        .placeholder("npx")
                        .invalid(move || fs.error(FormField::Command).is_some())
                        .on_change(move |_| fs.clear(FormField::Command))
                        .on_submit(e2())
                        .element(&fs.command_ref),
                )
                .error(fs.err(FormField::Command))
                .into_element(),
                form_row(
                    "Arguments",
                    text_field(fs.args)
                        .mono()
                        .placeholder("-y @modelcontextprotocol/server-github")
                        .invalid(move || fs.error(FormField::Args).is_some())
                        .on_change(move |_| fs.clear(FormField::Args))
                        .on_submit(e2()),
                )
                .optional()
                .hint("Separated by spaces. Quote one that has spaces.")
                .error(fs.err(FormField::Args))
                .into_element(),
            ],
            McpTransport::Http => vec![
                form_row(
                    "URL",
                    text_field(fs.url)
                        .mono()
                        .placeholder("http://localhost:3000/mcp")
                        .invalid(move || fs.error(FormField::Url).is_some())
                        .on_change(move |_| fs.clear(FormField::Url))
                        .on_submit(e2())
                        .element(&fs.url_ref),
                )
                .hint("Streamable HTTP. An SSE URL is tried as streamable HTTP.")
                .error(fs.err(FormField::Url))
                .into_element(),
            ],
        }
    };
    let group = form_group(vec![name_row.into_element(), transport_row.into_element()]).dynamic_rows(transport_rows).in_dialog();
    let secrets = dynamic(move || -> AnyElement {
        match fs.transport.get() {
            McpTransport::Stdio => secrets_section(fs, fs.env, fs.env_open, Secrets::Env).into_element(),
            McpTransport::Http => secrets_section(fs, fs.headers, fs.headers_open, Secrets::Headers).into_element(),
        }
    });
    let content = view()
        .gap(px(20.0))
        .child(group)
        .child(secrets)
        .child(dynamic(move || fs.form_error.get().map(|m| error_block(m, None))));
    let words = match &editing {
        Some(s) => format!("Edit {}", s.name),
        None => "Add MCP servers".to_owned(),
    };
    let s2 = submit.clone();
    frame(
        header(state, words, adding),
        content,
        vec![
            button("Cancel").variant(Variant::Secondary).disabled(move || state.busy.get()).on_press(move || state.open.set(false)),
            button(if adding { "Add server" } else { "Save changes" })
                .busy(move || state.busy.get(), "Saving and checking…")
                .on_press(move || s2()),
        ],
    )
}

/// Env variables (stdio) or headers (HTTP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Secrets {
    Env,
    Headers,
}

impl Secrets {
    fn title(self) -> &'static str {
        match self {
            Secrets::Env => "Environment",
            Secrets::Headers => "Headers",
        }
    }

    fn what(self) -> (&'static str, &'static str) {
        match self {
            Secrets::Env => ("variable", "variables"),
            Secrets::Headers => ("header", "headers"),
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Secrets::Env => "GITHUB_TOKEN",
            Secrets::Headers => "Authorization",
        }
    }

    fn field(self) -> FormField {
        match self {
            Secrets::Env => FormField::Env,
            Secrets::Headers => FormField::Headers,
        }
    }
}

/// The rows of env variables or headers in a group that opens, Add under them, the error after.
fn secrets_section(fs: FormState, rows: Signal<Vec<SecretRow>>, open: Signal<bool>, kind: Secrets) -> impl IntoElement {
    // The rows are made again only when one comes or goes, or a saved one is being replaced:
    // typing in a row keeps its fields, and the focus, where they are.
    let shape = computed(move || rows.with(|r| r.iter().map(|x| (x.id, x.saved.is_some(), x.replacing)).collect::<Vec<_>>()));
    let (one, many) = kind.what();
    let make_rows = move || {
        let mut out: Vec<AnyElement> = shape
            .get()
            .into_iter()
            .enumerate()
            .filter_map(|(i, (id, _, _))| rows.with_untracked(|r| r.iter().find(|x| x.id == id).cloned()).map(|row| (i, row)))
            .map(|(i, row)| secret_row(fs, rows, row, i + 1, kind).into_element())
            .collect();
        out.push(
            form_item(
                button(format!("Add {one}"))
                    .variant(Variant::Ghost)
                    .size(Size::Sm)
                    .leading(icon::PLUS)
                    .on_press(move || {
                        let id = fs.next_id.get_untracked() + 1;
                        fs.next_id.set(id);
                        rows.update(|r| r.push(SecretRow { id, name: String::new(), value: String::new(), saved: None, replacing: false }));
                    }),
            )
            .ml(px(-6.0))
            .into_element(),
        );
        out
    };
    view()
        .gap(px(6.0))
        .child(form_disclosure(
            kind.title(),
            move || {
                let n = rows.with(|r| r.iter().filter(|x| !x.name.trim().is_empty()).count());
                if n > 0 { count(n as i64, one, many) } else { "Optional".to_owned() }
            },
            open,
            make_rows,
            true,
        ))
        .child(dynamic(move || fs.error(kind.field()).map(|m| negative(m).px(px(12.0)))))
}

/// One row: a new one's name and value fields; a saved one's name, its hint and Replace (or the new
/// value's field and Keep); Remove at the end.
fn secret_row(fs: FormState, rows: Signal<Vec<SecretRow>>, row: SecretRow, n: usize, kind: Secrets) -> impl IntoElement {
    let id = row.id;
    let field = kind.field();
    let (one, _) = kind.what();
    let update = move |f: &dyn Fn(&mut SecretRow)| {
        rows.update(|r| r.iter_mut().filter(|x| x.id == id).for_each(|x| f(x)));
        fs.clear(field);
    };
    let value = signal(row.value.clone());
    let show = signal(false);
    let eye = dynamic(move || {
        let shown = show.get();
        Button::icon(if shown { icon::EYE_OFF } else { icon::EYE }, if shown { "Hide value" } else { "Show value" })
            .variant(Variant::Ghost)
            .size(Size::IconSm)
            .on_press(move || show.update(|s| *s = !*s))
    });
    let value_ref = element_ref();
    let value_field = move || {
        text_field(value)
            .mono()
            .masked(move || !show.get())
            .placeholder("value")
            .accessibility_label(format!("{} {n} value", if kind == Secrets::Env { "Variable" } else { "Header" }))
            .on_change(move |t| {
                let t = t.to_owned();
                update(&|x: &mut SecretRow| x.value = t.clone());
            })
            .element(&value_ref)
            .trailing(eye)
    };
    let remove = Button::icon(icon::X, format!("Remove {one} {n}"))
        .variant(Variant::Ghost)
        .size(Size::IconSm)
        .on_press(move || {
            rows.update(|r| r.retain(|x| x.id != id));
            fs.clear(field);
        });
    let name_cell: AnyElement = match row.saved {
        Some(_) => mono(text(row.name.clone())).number_of_lines(1).into_element(),
        None => {
            let name = signal(row.name.clone());
            text_field(name)
                .mono()
                .placeholder(kind.placeholder())
                .accessibility_label(format!("{} {n} name", if kind == Secrets::Env { "Variable" } else { "Header" }))
                .on_change(move |t| {
                    let t = t.to_owned();
                    update(&|x: &mut SecretRow| x.name = t.clone());
                })
                .into_element()
        }
    };
    let value_cell: AnyElement = match (&row.saved, row.replacing) {
        (Some(hint), false) => view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .min_w(px(0.0))
            .child(
                meta(text("Saved "))
                    .number_of_lines(1)
                    .flex_shrink(1.0)
                    .min_w(px(0.0))
                    .child(mono(text(hint.clone().unwrap_or_else(|| "(empty)".to_owned()))).text_color(muted_ink)),
            )
            .child(view().flex_1())
            .child(button("Replace").variant(Variant::Ghost).size(Size::Sm).on_press(move || {
                rows.update(|r| r.iter_mut().filter(|x| x.id == id).for_each(|x| x.replacing = true));
            }))
            .into_element(),
        (Some(_), true) => {
            // The new value's field takes the focus as it appears.
            let _ = request_animation_frame(move |_| value_ref.focus());
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .min_w(px(0.0))
                .child(view().flex_1().min_w(px(0.0)).child(value_field()))
                .child(button("Keep").variant(Variant::Ghost).size(Size::Sm).on_press(move || {
                    rows.update(|r| {
                        r.iter_mut().filter(|x| x.id == id).for_each(|x| {
                            x.replacing = false;
                            x.value.clear();
                        })
                    });
                }))
                .into_element()
        }
        (None, _) => value_field().into_element(),
    };
    form_item(
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(view().flex_1().min_w(px(0.0)).child(name_cell))
            .child(view().flex_grow(1.4).basis(px(0.0)).min_w(px(0.0)).child(value_cell))
            .child(remove),
    )
}
