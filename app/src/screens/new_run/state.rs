//! The form's live state: one signal per value and per error, the element of each field (to focus
//! the first one with an error), the open groups and the notes under the form.

use nsh_core::prelude::*;

use super::logic::{Field, Form, Initial, check};
use super::tools::ToolsForm;
use crate::api::types::{EndpointOut as Endpoint, MaxTokensField, McpServerOut as Server, PromptSetOut as PromptSet, RunOut as Run};
use crate::data::Load;

/// What the screen loaded.
#[derive(Clone, Copy)]
pub struct Data {
    pub endpoints: Signal<Load<Vec<Endpoint>>>,
    pub sets: Signal<Load<Vec<PromptSet>>>,
    pub runs: Signal<Load<Vec<Run>>>,
    /// The MCP servers the Tools group offers.
    pub servers: Signal<Load<Vec<Server>>>,
}

impl Data {
    /// The endpoints loaded so far (reactive).
    pub fn endpoint_list(&self) -> Vec<Endpoint> {
        self.endpoints.with(|l| l.loaded().cloned().unwrap_or_default())
    }

    /// The MCP servers loaded so far (reactive).
    pub fn server_list(&self) -> Vec<Server> {
        self.servers.with(|l| l.loaded().cloned().unwrap_or_default())
    }

    /// The prompt sets loaded so far (reactive).
    pub fn set_list(&self) -> Vec<PromptSet> {
        self.sets.with(|l| l.loaded().cloned().unwrap_or_default())
    }


    /// The endpoint the form has chosen (reactive).
    pub fn selected_endpoint(&self, state: &State) -> Option<Endpoint> {
        let id = state.get(Field::Endpoint);
        self.endpoints.with(|l| l.loaded().and_then(|v| v.iter().find(|e| e.id.to_string() == id).cloned()))
    }

    /// The prompt set the form has chosen (reactive).
    pub fn selected_set(&self, state: &State) -> Option<PromptSet> {
        let id = state.get(Field::PromptSet);
        self.sets.with(|l| l.loaded().and_then(|v| v.iter().find(|s| s.id.to_string() == id).cloned()))
    }
}

/// The form's signals.
#[derive(Clone, Copy)]
pub struct State {
    values: [Signal<String>; Field::COUNT],
    errors: [Signal<Option<String>>; Field::COUNT],
    elements: [ElementRef; Field::COUNT],
    pub stream: Signal<bool>,
    /// "Use tools", the picks and the tool choice.
    pub tools: Signal<ToolsForm>,
    /// The Tool settings group.
    pub tools_open: Signal<bool>,
    /// What Run again could not copy of the run's tools.
    pub tools_note: Signal<Option<String>>,
    /// The servers whose tools are listed (their rows opened).
    pub expanded: Signal<Vec<i64>>,
    /// The servers being checked from the group.
    pub checking: Signal<Vec<i64>>,
    pub token_field: Signal<MaxTokensField>,
    pub prompt_open: Signal<bool>,
    pub advanced_open: Signal<bool>,
    /// What the server said about the form as a whole, or what went wrong on the way.
    pub form_error: Signal<Option<String>>,
    /// The result of the last Refresh models, under the Model field.
    pub models_note: Signal<Option<String>>,
    pub refreshing: Signal<bool>,
    pub starting: Signal<bool>,
    /// What is typed in the model field.
    pub query: Signal<String>,
}

impl State {
    pub fn new() -> Self {
        State {
            values: std::array::from_fn(|_| signal(String::new())),
            errors: std::array::from_fn(|_| signal(None)),
            elements: std::array::from_fn(|_| element_ref()),
            stream: signal(true),
            tools: signal(ToolsForm::default()),
            tools_open: signal(false),
            tools_note: signal(None),
            expanded: signal(Vec::new()),
            checking: signal(Vec::new()),
            token_field: signal(MaxTokensField::MaxTokens),
            prompt_open: signal(false),
            advanced_open: signal(false),
            form_error: signal(None),
            models_note: signal(None),
            refreshing: signal(false),
            starting: signal(false),
            query: signal(String::new()),
        }
    }

    /// Fills the form with its first values.
    pub fn apply(&self, initial: &Initial) {
        for field in Field::ALL {
            self.values[field.index()].set(initial.form.get(field).to_owned());
            self.errors[field.index()].set(None);
        }
        self.stream.set(initial.form.stream);
        self.token_field.set(initial.form.max_tokens_field);
        self.prompt_open.set(initial.prompt_open);
        self.advanced_open.set(initial.advanced_open);
        self.tools.set(initial.form.tools.clone());
        self.tools_open.set(initial.tools_open);
        self.tools_note.set(initial.tools_note.clone());
        self.form_error.set(None);
        self.models_note.set(None);
        self.query.set(String::new());
    }

    /// The signal holding `field`'s text.
    pub fn value(&self, field: Field) -> Signal<String> {
        self.values[field.index()]
    }

    /// `field`'s text (reactive).
    pub fn get(&self, field: Field) -> String {
        self.values[field.index()].get()
    }

    /// `field`'s text, read once.
    pub fn peek(&self, field: Field) -> String {
        self.values[field.index()].get_untracked()
    }

    /// The signal holding `field`'s error.
    pub fn error(&self, field: Field) -> Signal<Option<String>> {
        self.errors[field.index()]
    }

    /// Whether `field` has an error (reactive).
    pub fn has_error(&self, field: Field) -> bool {
        self.errors[field.index()].with(Option::is_some)
    }

    /// `field`'s element, to focus it.
    pub fn element(&self, field: Field) -> ElementRef {
        self.elements[field.index()]
    }

    /// Sets `field`'s text and clears its error: the user changed it.
    pub fn set(&self, field: Field, value: impl Into<String>) {
        self.values[field.index()].set(value.into());
        self.errors[field.index()].set(None);
    }

    /// The form as it is (reactive: it reads every value).
    pub fn form(&self) -> Form {
        self.read(|s| s.get())
    }

    /// The form as it is, read once.
    pub fn snapshot(&self) -> Form {
        self.read(|s| s.get_untracked())
    }

    fn read(&self, get: impl Fn(&Signal<String>) -> String) -> Form {
        let mut form = Form::default();
        for field in Field::ALL {
            form.set(field, get(&self.values[field.index()]));
        }
        form.stream = self.stream.get_untracked();
        form.max_tokens_field = self.token_field.get_untracked();
        form.tools = self.tools.get_untracked();
        form
    }

    /// Checks `field` when it loses the focus (the web UI's blur validation).
    pub fn blur(&self, field: Field) {
        let message = check(field, &self.snapshot());
        self.errors[field.index()].set(message);
    }

    /// Shows `found` as the fields' errors (every other field's is cleared), opens the group of the
    /// first one and focuses it, scrolled to the middle of the window.
    pub fn show_errors(&self, found: &[(Field, String)]) {
        for field in Field::ALL {
            let message = found.iter().find(|(f, _)| *f == field).map(|(_, m)| m.clone());
            self.errors[field.index()].set(message);
        }
        let Some(first) = super::logic::first_error(found) else { return };
        if first.in_prompt_options() {
            self.prompt_open.set(true);
        }
        if first.in_advanced() {
            self.advanced_open.set(true);
        }
        if first.in_tool_settings() {
            self.tools_open.set(true);
        }
        let element = self.element(first);
        // After the commit that opened the group, so the field exists.
        let _ = request_animation_frame(move |_| {
            element.focus();
            element.reveal(RevealOptions::default().vertical(ScrollAlignment::Center));
        });
    }

    /// Clears every field's error.
    pub fn clear_errors(&self) {
        for error in self.errors {
            error.set(None);
        }
    }
}
