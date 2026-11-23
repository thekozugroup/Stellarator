//! New run's rules, with no UI: the form's values, each field's check and error text, the request
//! body, the first values (a previous run's settings, the prefills, the endpoint's defaults), the
//! summaries (frontend/src/pages/run-new.tsx).
//!
//! Values are text, as the inputs hold them, so typing is never blocked; the checks decide what
//! they mean. Numbers read as JavaScript's `Number()` reads them ([`js_number`]).

use std::collections::{BTreeMap, HashMap};

use crate::api::types::{
    EndpointOut as Endpoint, MaxTokensField, PromptSetOut as PromptSet, PromptSetStatus, RunCreate, RunOut as Run, RunParams,
};
use crate::format::{count, date_time, duration, int};
use crate::ui::stepper::parse_int;

/// A field of the form that has a value and a check (the stream box and the token-limit name have
/// neither).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Endpoint,
    Model,
    PromptSet,
    Concurrency,
    Samples,
    Temperature,
    TopP,
    MaxTokens,
    Seed,
    SystemPrompt,
    UserTemplate,
    ExtraBody,
    ThinkOpen,
    ThinkClose,
    MaxAttempts,
    Timeout,
    PauseAfter,
    Name,
}

impl Field {
    /// The fields in the order they are on screen: the first with an error gets the focus.
    pub const ALL: [Field; 18] = [
        Field::Endpoint,
        Field::Model,
        Field::PromptSet,
        Field::Concurrency,
        Field::Samples,
        Field::Temperature,
        Field::TopP,
        Field::MaxTokens,
        Field::Seed,
        Field::SystemPrompt,
        Field::UserTemplate,
        Field::ExtraBody,
        Field::ThinkOpen,
        Field::ThinkClose,
        Field::MaxAttempts,
        Field::Timeout,
        Field::PauseAfter,
        Field::Name,
    ];

    /// Its place in [`Field::ALL`].
    pub fn index(self) -> usize {
        Field::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// In the Prompt options group, which opens to show an error.
    pub fn in_prompt_options(self) -> bool {
        matches!(self, Field::SystemPrompt | Field::UserTemplate)
    }

    /// In the Advanced group, which opens to show an error.
    pub fn in_advanced(self) -> bool {
        matches!(
            self,
            Field::ExtraBody | Field::ThinkOpen | Field::ThinkClose | Field::MaxAttempts | Field::Timeout | Field::PauseAfter
        )
    }

    /// The field a server message names (a path of `RunCreate`).
    pub fn from_server(path: &str) -> Option<Field> {
        Some(match path {
            "endpoint_id" => Field::Endpoint,
            "model" => Field::Model,
            "prompt_set_id" => Field::PromptSet,
            "concurrency" => Field::Concurrency,
            "name" => Field::Name,
            "params.samples_per_prompt" => Field::Samples,
            "params.temperature" => Field::Temperature,
            "params.top_p" => Field::TopP,
            "params.max_tokens" => Field::MaxTokens,
            "params.seed" => Field::Seed,
            "params.system_prompt" => Field::SystemPrompt,
            "params.user_template" => Field::UserTemplate,
            "params.extra_body" => Field::ExtraBody,
            "params.think_open" => Field::ThinkOpen,
            "params.think_close" => Field::ThinkClose,
            "params.max_attempts" => Field::MaxAttempts,
            "params.request_timeout_s" => Field::Timeout,
            "params.pause_after_failures" => Field::PauseAfter,
            _ => return None,
        })
    }
}

/// The form's values.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    values: [String; 18],
    pub max_tokens_field: MaxTokensField,
    pub stream: bool,
}

impl Default for Form {
    /// The defaults of a run with nothing chosen yet.
    fn default() -> Self {
        let mut form = Form { values: Default::default(), max_tokens_field: MaxTokensField::MaxTokens, stream: true };
        for (field, value) in [
            (Field::Concurrency, "16"),
            (Field::Samples, "1"),
            (Field::Temperature, "0.7"),
            (Field::MaxTokens, "8192"),
            (Field::ThinkOpen, "<think>"),
            (Field::ThinkClose, "</think>"),
            (Field::MaxAttempts, "4"),
            (Field::Timeout, "1800"),
            (Field::PauseAfter, "10"),
        ] {
            form.set(field, value);
        }
        form
    }
}

impl Form {
    pub fn get(&self, field: Field) -> &str {
        &self.values[field.index()]
    }

    pub fn set(&mut self, field: Field, value: impl Into<String>) {
        self.values[field.index()] = value.into();
    }
}

/// `Number(s)` of JavaScript: the whole text read as a decimal number (an empty text is 0); NaN
/// for anything else.
pub fn js_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    // Rust also reads "inf", "nan" and "infinity"; JavaScript reads only the last, capitalized.
    if t.chars().any(|c| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))) {
        return match t {
            "Infinity" | "+Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => f64::NAN,
        };
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// A whole number with an optional minus sign (`/^-?\d+$/` of the trimmed text).
fn is_int(s: &str) -> bool {
    let t = s.trim();
    let digits = t.strip_prefix('-').unwrap_or(t);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// One field's rule: the message when its value is not acceptable. Empty optional fields are fine.
pub fn check(field: Field, form: &Form) -> Option<String> {
    let v = form.get(field).trim();
    let num = js_number(v);
    let int_in = |min: i64, max: i64, what: &str| {
        (!is_int(v) || num < min as f64 || num > max as f64)
            .then(|| format!("{what} must be a whole number from {} to {}.", int(min), int(max)))
    };
    match field {
        Field::Endpoint => v.is_empty().then(|| "Choose an endpoint.".to_owned()),
        Field::Model => v.is_empty().then(|| "Choose a model, or type its id.".to_owned()),
        Field::PromptSet => v.is_empty().then(|| "Choose a prompt set.".to_owned()),
        Field::Concurrency => int_in(1, 4096, "Seqs"),
        Field::Samples => int_in(1, 64, "Samples per prompt"),
        Field::Temperature => {
            if v.is_empty() {
                return None;
            }
            (num.is_nan() || !(0.0..=2.0).contains(&num)).then(|| "Temperature must be a number from 0 to 2.".to_owned())
        }
        Field::TopP => {
            if v.is_empty() {
                return None;
            }
            (num.is_nan() || num <= 0.0 || num > 1.0).then(|| "Top p must be above 0 and at most 1.".to_owned())
        }
        Field::MaxTokens => {
            if v.is_empty() {
                return None;
            }
            (!is_int(v) || num < 1.0).then(|| "Max tokens must be a whole number of 1 or more.".to_owned())
        }
        Field::Seed => {
            if v.is_empty() {
                return None;
            }
            (!is_int(v)).then(|| "Seed must be a whole number.".to_owned())
        }
        Field::UserTemplate => {
            if v.is_empty() {
                return None;
            }
            (!v.contains("{prompt}")).then(|| "Add {prompt} where each prompt should go.".to_owned())
        }
        Field::ExtraBody => {
            if v.is_empty() {
                return None;
            }
            match serde_json::from_str::<serde_json::Value>(v) {
                Ok(serde_json::Value::Object(_)) => None,
                Ok(_) => Some("Enter a JSON object, like {\"key\": \"value\"}.".to_owned()),
                Err(e) => Some(format!("This is not valid JSON: {e}")),
            }
        }
        Field::ThinkOpen | Field::ThinkClose => v.is_empty().then(|| "Enter a tag, for example <think>.".to_owned()),
        Field::MaxAttempts => int_in(1, 20, "Attempts"),
        Field::Timeout => (num.is_nan() || v.is_empty() || num < 10.0).then(|| "The timeout must be at least 10 seconds.".to_owned()),
        Field::PauseAfter => (!is_int(v) || num < 0.0).then(|| "Enter 0 or a whole number of failures.".to_owned()),
        Field::SystemPrompt | Field::Name => None,
    }
}

/// Every field's message, for the fields that have one, in screen order.
pub fn check_all(form: &Form) -> Vec<(Field, String)> {
    Field::ALL.iter().filter_map(|f| check(*f, form).map(|m| (*f, m))).collect()
}

/// The first field of `errors` in screen order.
pub fn first_error(errors: &[(Field, String)]) -> Option<Field> {
    Field::ALL.iter().copied().find(|f| errors.iter().any(|(e, _)| e == f))
}

/// The server's messages for single fields (`ApiError::fields`) as the fields' errors, and what it
/// said about anything else, which the form shows under its fields.
pub fn server_errors(fields: &BTreeMap<String, String>) -> (Vec<(Field, String)>, Vec<String>) {
    let mut mapped = Vec::new();
    let mut other = Vec::new();
    for (path, message) in fields {
        match Field::from_server(path) {
            Some(field) => mapped.push((field, message.clone())),
            None => other.push(message.clone()),
        }
    }
    (mapped, other)
}

/// The request for a valid `form`.
pub fn to_body(form: &Form) -> Result<RunCreate, String> {
    let number = |f: Field| -> Option<f64> {
        let v = form.get(f);
        (!v.trim().is_empty()).then(|| js_number(v))
    };
    let whole = |f: Field| number(f).map(|n| n as i64);
    let text = |f: Field| {
        let v = form.get(f);
        (!v.trim().is_empty()).then(|| v.to_owned())
    };
    let extra_body = match form.get(Field::ExtraBody).trim() {
        "" => Default::default(),
        v => match serde_json::from_str::<serde_json::Value>(v) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => return Err("The extra request body is not a JSON object.".to_owned()),
        },
    };
    let id = |f: Field| js_number(form.get(f)) as i64;
    Ok(RunCreate {
        name: Some(form.get(Field::Name).trim().to_owned()).filter(|n| !n.is_empty()),
        endpoint_id: id(Field::Endpoint),
        model: form.get(Field::Model).trim().to_owned(),
        prompt_set_id: id(Field::PromptSet),
        concurrency: id(Field::Concurrency),
        params: RunParams {
            samples_per_prompt: id(Field::Samples),
            temperature: number(Field::Temperature),
            top_p: number(Field::TopP),
            max_tokens: whole(Field::MaxTokens),
            max_tokens_field: form.max_tokens_field,
            seed: whole(Field::Seed),
            system_prompt: text(Field::SystemPrompt),
            user_template: text(Field::UserTemplate),
            extra_body,
            stream: form.stream,
            think_open: form.get(Field::ThinkOpen).to_owned(),
            think_close: form.get(Field::ThinkClose).to_owned(),
            max_attempts: id(Field::MaxAttempts),
            request_timeout_s: js_number(form.get(Field::Timeout)),
            pause_after_failures: id(Field::PauseAfter),
            tools: None,
        },
    })
}

// ------------------------------------------------------------------ first values

/// What New run opens with: a run to copy (Run again), an endpoint and a prompt set to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prefill {
    pub from: Option<i64>,
    pub endpoint: Option<i64>,
    pub prompt_set: Option<i64>,
}

/// The form's first values and which groups start open.
#[derive(Debug, Clone, PartialEq)]
pub struct Initial {
    pub form: Form,
    pub prompt_open: bool,
    pub advanced_open: bool,
}

/// A number as the field shows it (`String(n)`).
fn shown(n: Option<f64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_default()
}

/// A previous run's settings as form values.
fn from_run(run: &Run) -> Form {
    let p = &run.params;
    let mut f = Form::default();
    f.set(Field::Endpoint, run.endpoint_id.map(|id| id.to_string()).unwrap_or_default());
    f.set(Field::Model, run.model.clone());
    f.set(Field::PromptSet, run.prompt_set_id.to_string());
    f.set(Field::Concurrency, run.concurrency.to_string());
    f.set(Field::Samples, p.samples_per_prompt.to_string());
    f.set(Field::Temperature, shown(p.temperature));
    f.set(Field::TopP, shown(p.top_p));
    f.set(Field::MaxTokens, shown(p.max_tokens.map(|n| n as f64)));
    f.max_tokens_field = p.max_tokens_field;
    f.set(Field::Seed, shown(p.seed.map(|n| n as f64)));
    f.set(Field::SystemPrompt, p.system_prompt.clone().unwrap_or_default());
    f.set(Field::UserTemplate, p.user_template.clone().unwrap_or_default());
    f.set(
        Field::ExtraBody,
        if p.extra_body.is_empty() { String::new() } else { serde_json::to_string_pretty(&p.extra_body).unwrap_or_default() },
    );
    f.stream = p.stream;
    f.set(Field::ThinkOpen, p.think_open.clone());
    f.set(Field::ThinkClose, p.think_close.clone());
    f.set(Field::MaxAttempts, p.max_attempts.to_string());
    f.set(Field::Timeout, p.request_timeout_s.to_string());
    f.set(Field::PauseAfter, p.pause_after_failures.to_string());
    f
}

/// The model to prefill for an endpoint: the one its most recent run used, or its only listed
/// model. Never the first of many, which would be a guess.
pub fn suggested_model(endpoint: &Endpoint, runs: Option<&[Run]>) -> String {
    let last = runs.and_then(|runs| runs.iter().find(|r| r.endpoint_id == Some(endpoint.id)));
    // A last-used model the endpoint no longer lists is likely wrong, unless it lists nothing.
    if let Some(last) = last {
        if endpoint.models.is_empty() || endpoint.models.contains(&last.model) {
            return last.model.clone();
        }
    }
    if endpoint.models.len() == 1 { endpoint.models[0].clone() } else { String::new() }
}

/// The form's first values: a previous run's settings (Run again), or the prefills and sensible
/// picks (the only endpoint, the only prompt set, the endpoint's default seqs and model).
pub fn initial_state(endpoints: &[Endpoint], sets: &[PromptSet], runs: Option<&[Run]>, source: Option<&Run>, prefill: Prefill) -> Initial {
    let mut form = Form::default();
    let mut prompt_open = false;
    let mut advanced_open = false;
    if let Some(source) = source {
        form = from_run(source);
        if !endpoints.iter().any(|e| e.id.to_string() == form.get(Field::Endpoint)) {
            form.set(Field::Endpoint, "");
        }
        if !sets.iter().any(|s| s.id.to_string() == form.get(Field::PromptSet) && s.status == PromptSetStatus::Ready) {
            form.set(Field::PromptSet, "");
        }
        let p = &source.params;
        prompt_open = p.system_prompt.as_deref().is_some_and(|s| !s.is_empty()) || p.user_template.as_deref().is_some_and(|s| !s.is_empty());
        let d = Form::default();
        advanced_open = !form.get(Field::ExtraBody).is_empty()
            || !p.stream
            || [Field::ThinkOpen, Field::ThinkClose, Field::MaxAttempts, Field::Timeout, Field::PauseAfter]
                .iter()
                .any(|f| form.get(*f) != d.get(*f));
    } else {
        let picked = prefill.endpoint.and_then(|id| endpoints.iter().find(|e| e.id == id)).or(if endpoints.len() == 1 { endpoints.first() } else { None });
        if let Some(ep) = picked {
            form.set(Field::Endpoint, ep.id.to_string());
            form.set(Field::Concurrency, ep.default_concurrency.to_string());
            form.set(Field::Model, suggested_model(ep, runs));
        }
        let set = prefill.prompt_set.and_then(|id| sets.iter().find(|s| s.id == id)).or(if sets.len() == 1 { sets.first() } else { None });
        if let Some(set) = set.filter(|s| s.status == PromptSetStatus::Ready) {
            form.set(Field::PromptSet, set.id.to_string());
        }
    }
    Initial { form, prompt_open, advanced_open }
}

/// The model after choosing endpoint `endpoint`: the one already there when the new endpoint also
/// lists it (or lists none, so a typed id stays), else the suggestion for the new endpoint.
pub fn model_after_endpoint(current: &str, endpoint: Option<&Endpoint>, runs: Option<&[Run]>) -> String {
    match endpoint {
        None => current.to_owned(),
        Some(ep) if !current.is_empty() && (ep.models.iter().any(|m| m == current) || ep.models.is_empty()) => current.to_owned(),
        Some(ep) => suggested_model(ep, runs),
    }
}

// ------------------------------------------------------------------ words

/// "30 min", "90 s", "1 h 30 min": a whole timeout in words.
pub fn seconds(s: f64) -> String {
    if s % 3600.0 == 0.0 {
        return format!("{} h", int((s / 3600.0) as i64));
    }
    if s >= 3600.0 && s % 60.0 == 0.0 {
        return format!("{} h {} min", int((s / 3600.0).floor() as i64), int(((s % 3600.0) / 60.0) as i64));
    }
    if s % 60.0 == 0.0 {
        return format!("{} min", int((s / 60.0) as i64));
    }
    duration(Some(s * 1000.0))
}

/// The run's size, under the form: "1,284 requests · 16 seqs". Nothing until a prompt set and a
/// number of samples are chosen.
pub fn run_summary(prompt_set: Option<&PromptSet>, samples: &str, seqs: &str) -> Option<String> {
    let set = prompt_set?;
    let samples = parse_int(samples).filter(|n| *n >= 1)?;
    let requests = count(set.prompt_count * samples, "request", "requests");
    Some(match parse_int(seqs).filter(|n| *n >= 1) {
        Some(n) => format!("{requests} · {} seqs", int(n)),
        None => requests,
    })
}

/// The Advanced group's closed summary: "Streaming, 4 attempts per sample, 30 min timeout, pause
/// after 10 failures in a row".
pub fn advanced_summary(form: &Form) -> String {
    let timeout = js_number(form.get(Field::Timeout));
    let attempts = js_number(form.get(Field::MaxAttempts));
    let attempts = if attempts.is_nan() { 0 } else { attempts as i64 };
    let pause = js_number(form.get(Field::PauseAfter));
    let mut parts = vec![
        if form.stream { "Streaming".to_owned() } else { "Not streaming".to_owned() },
        format!("{} per sample", count(attempts, "attempt", "attempts")),
    ];
    if timeout.is_finite() && timeout > 0.0 {
        parts.push(format!("{} timeout", seconds(timeout)));
    }
    parts.push(if pause > 0.0 {
        format!("pause after {} failures in a row", int(pause as i64))
    } else {
        "never pause on failures".to_owned()
    });
    if !form.get(Field::ExtraBody).trim().is_empty() {
        parts.push("extra request body set".to_owned());
    }
    parts.join(", ")
}

/// The Prompt options group's closed summary.
pub fn prompt_summary(form: &Form) -> String {
    let system = !form.get(Field::SystemPrompt).trim().is_empty();
    let template = !form.get(Field::UserTemplate).trim().is_empty();
    if !system && !template {
        return "None".to_owned();
    }
    [system.then_some("System prompt replaced"), template.then_some("prompt template set")].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// The name a run gets when the field is empty: "<model> on <prompt set>"; empty until both are
/// chosen.
pub fn default_name(model: &str, prompt_set: Option<&PromptSet>) -> String {
    match prompt_set {
        Some(set) if !model.is_empty() => format!("{model} on {}", set.name),
        _ => String::new(),
    }
}

/// How many prompt sets carry each name: same-named sets are told apart by their import date.
pub fn shared_names(sets: &[PromptSet]) -> HashMap<String, usize> {
    let mut seen = HashMap::new();
    for set in sets {
        *seen.entry(set.name.clone()).or_insert(0) += 1;
    }
    seen
}

/// What a prompt set's row says after its name: "1,284 prompts", "Importing" or "Failed", and for
/// a ready set that shares its name the import date.
pub fn prompt_set_note(set: &PromptSet, same_name: usize) -> String {
    match set.status {
        PromptSetStatus::Importing => "Importing".to_owned(),
        PromptSetStatus::Failed => "Failed".to_owned(),
        PromptSetStatus::Ready => {
            let n = count(set.prompt_count, "prompt", "prompts");
            if same_name > 1 { format!("{n} · {}", date_time(set.created_at)) } else { n }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn endpoint(id: i64, models: &[&str], concurrency: i64, ok: Option<bool>) -> Endpoint {
        serde_json::from_value(json!({
            "id": id, "name": format!("Endpoint {id}"), "base_url": "http://localhost:8000/v1",
            "has_api_key": false, "api_key_hint": null, "location": "local",
            "default_concurrency": concurrency, "headers": {}, "models": models,
            "last_checked_at": null, "last_ok": ok, "last_error": null,
            "created_at": "2026-11-01T00:00:00Z", "run_count": 0
        }))
        .unwrap()
    }

    fn set(id: i64, name: &str, prompts: i64, status: &str) -> PromptSet {
        serde_json::from_value(json!({
            "id": id, "name": name, "source_kind": "file", "source": {}, "prompt_count": prompts,
            "created_at": "2026-11-01T00:00:00Z", "run_count": 0, "status": status,
            "status_detail": null, "rows_read": prompts, "rows_total": null
        }))
        .unwrap()
    }

    fn run(endpoint_id: i64, model: &str, params: serde_json::Value) -> Run {
        serde_json::from_value(json!({
            "id": 7, "name": "r", "endpoint_id": endpoint_id, "endpoint_name": "e", "model": model,
            "prompt_set_id": 2, "prompt_set_name": "p", "params": params, "concurrency": 8,
            "status": "completed", "status_reason": null, "total": 10, "done": 10, "failed": 0,
            "truncated": 0, "pending": 0, "prompt_tokens": 0, "completion_tokens": 0,
            "reasoning_tokens": 0, "active_seconds": 1.0, "created_at": "2026-11-01T00:00:00Z",
            "started_at": null, "finished_at": null
        }))
        .unwrap()
    }

    fn with(fields: &[(Field, &str)]) -> Form {
        let mut f = Form::default();
        for (field, v) in fields {
            f.set(*field, *v);
        }
        f
    }

    #[test]
    fn numbers_read_as_javascript_reads_them() {
        assert_eq!(js_number(""), 0.0);
        assert_eq!(js_number("  12 "), 12.0);
        assert_eq!(js_number("1e3"), 1000.0);
        assert_eq!(js_number(".5"), 0.5);
        assert!(js_number("abc").is_nan());
        assert!(js_number("inf").is_nan());
        assert!(js_number("0x10").is_nan());
        assert!(js_number("1.2.3").is_nan());
        assert!(is_int("-4") && is_int(" 12 ") && !is_int("1.5") && !is_int("") && !is_int("-"));
    }

    #[test]
    fn defaults_pass_once_the_choices_are_made() {
        let mut f = Form::default();
        let errors = check_all(&f);
        let fields: Vec<_> = errors.iter().map(|(f, _)| *f).collect();
        assert_eq!(fields, [Field::Endpoint, Field::Model, Field::PromptSet]);
        assert_eq!(errors[0].1, "Choose an endpoint.");
        assert_eq!(errors[1].1, "Choose a model, or type its id.");
        assert_eq!(errors[2].1, "Choose a prompt set.");
        f.set(Field::Endpoint, "1");
        f.set(Field::Model, "m");
        f.set(Field::PromptSet, "2");
        assert!(check_all(&f).is_empty());
    }

    #[test]
    fn whole_number_ranges_say_their_bounds() {
        let msg = |field, v: &str| check(field, &with(&[(field, v)]));
        assert_eq!(msg(Field::Concurrency, "0").as_deref(), Some("Seqs must be a whole number from 1 to 4,096."));
        assert_eq!(msg(Field::Concurrency, "4097").as_deref(), Some("Seqs must be a whole number from 1 to 4,096."));
        assert_eq!(msg(Field::Concurrency, "4096"), None);
        assert_eq!(msg(Field::Concurrency, "1.5").as_deref(), Some("Seqs must be a whole number from 1 to 4,096."));
        assert_eq!(msg(Field::Concurrency, "").as_deref(), Some("Seqs must be a whole number from 1 to 4,096."));
        assert_eq!(msg(Field::Samples, "65").as_deref(), Some("Samples per prompt must be a whole number from 1 to 64."));
        assert_eq!(msg(Field::MaxAttempts, "21").as_deref(), Some("Attempts must be a whole number from 1 to 20."));
        assert_eq!(msg(Field::MaxAttempts, "20"), None);
    }

    #[test]
    fn sampling_fields_may_be_empty_but_not_wrong() {
        let msg = |field, v: &str| check(field, &with(&[(field, v)]));
        assert_eq!(msg(Field::Temperature, ""), None);
        assert_eq!(msg(Field::Temperature, "0"), None);
        assert_eq!(msg(Field::Temperature, "2"), None);
        assert_eq!(msg(Field::Temperature, "2.1").as_deref(), Some("Temperature must be a number from 0 to 2."));
        assert_eq!(msg(Field::Temperature, "hot").as_deref(), Some("Temperature must be a number from 0 to 2."));
        assert_eq!(msg(Field::TopP, "0").as_deref(), Some("Top p must be above 0 and at most 1."));
        assert_eq!(msg(Field::TopP, "1"), None);
        assert_eq!(msg(Field::TopP, "1.01").as_deref(), Some("Top p must be above 0 and at most 1."));
        assert_eq!(msg(Field::MaxTokens, "0").as_deref(), Some("Max tokens must be a whole number of 1 or more."));
        assert_eq!(msg(Field::MaxTokens, ""), None);
        assert_eq!(msg(Field::Seed, "-5"), None);
        assert_eq!(msg(Field::Seed, "5.5").as_deref(), Some("Seed must be a whole number."));
    }

    #[test]
    fn the_rest_of_the_rules() {
        let msg = |field, v: &str| check(field, &with(&[(field, v)]));
        assert_eq!(msg(Field::UserTemplate, "Solve {prompt}"), None);
        assert_eq!(msg(Field::UserTemplate, "Solve").as_deref(), Some("Add {prompt} where each prompt should go."));
        assert_eq!(msg(Field::ExtraBody, ""), None);
        assert_eq!(msg(Field::ExtraBody, "{\"a\": {\"b\": 1}}"), None);
        assert_eq!(msg(Field::ExtraBody, "[1]").as_deref(), Some("Enter a JSON object, like {\"key\": \"value\"}."));
        assert_eq!(msg(Field::ExtraBody, "null").as_deref(), Some("Enter a JSON object, like {\"key\": \"value\"}."));
        assert!(msg(Field::ExtraBody, "{oops").unwrap().starts_with("This is not valid JSON: "));
        assert_eq!(msg(Field::ThinkOpen, " ").as_deref(), Some("Enter a tag, for example <think>."));
        assert_eq!(msg(Field::Timeout, "9").as_deref(), Some("The timeout must be at least 10 seconds."));
        assert_eq!(msg(Field::Timeout, "").as_deref(), Some("The timeout must be at least 10 seconds."));
        assert_eq!(msg(Field::Timeout, "10"), None);
        assert_eq!(msg(Field::Timeout, "10.5"), None);
        assert_eq!(msg(Field::PauseAfter, "0"), None);
        assert_eq!(msg(Field::PauseAfter, "-1").as_deref(), Some("Enter 0 or a whole number of failures."));
        assert_eq!(msg(Field::PauseAfter, "").as_deref(), Some("Enter 0 or a whole number of failures."));
    }

    #[test]
    fn the_first_error_is_the_first_on_screen() {
        let errors = vec![(Field::Name, "x".to_owned()), (Field::Timeout, "y".to_owned()), (Field::Model, "z".to_owned())];
        assert_eq!(first_error(&errors), Some(Field::Model));
        assert_eq!(first_error(&[]), None);
        assert!(Field::UserTemplate.in_prompt_options() && !Field::Name.in_prompt_options());
        assert!(Field::Timeout.in_advanced() && !Field::Seed.in_advanced());
    }

    #[test]
    fn the_server_names_fields_by_their_path() {
        let mut fields = BTreeMap::new();
        fields.insert("params.temperature".to_owned(), "too hot".to_owned());
        fields.insert("endpoint_id".to_owned(), "gone".to_owned());
        fields.insert("params.nope".to_owned(), "odd".to_owned());
        let (mapped, other) = server_errors(&fields);
        assert_eq!(mapped.len(), 2);
        assert!(mapped.contains(&(Field::Temperature, "too hot".to_owned())));
        assert!(mapped.contains(&(Field::Endpoint, "gone".to_owned())));
        assert_eq!(other, ["odd"]);
    }

    #[test]
    fn the_body_has_the_forms_values() {
        let mut f = with(&[
            (Field::Endpoint, "3"),
            (Field::Model, " m "),
            (Field::PromptSet, "5"),
            (Field::Concurrency, "32"),
            (Field::Samples, "2"),
            (Field::TopP, ""),
            (Field::Seed, "42"),
            (Field::SystemPrompt, "  "),
            (Field::UserTemplate, "Q: {prompt}"),
            (Field::ExtraBody, "{\"k\": 1}"),
            (Field::Name, "  Round 2 "),
        ]);
        f.max_tokens_field = MaxTokensField::MaxCompletionTokens;
        f.stream = false;
        let body = to_body(&f).unwrap();
        assert_eq!(body.name.as_deref(), Some("Round 2"));
        assert_eq!((body.endpoint_id, body.model.as_str(), body.prompt_set_id, body.concurrency), (3, "m", 5, 32));
        let p = body.params;
        assert_eq!(p.samples_per_prompt, 2);
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.top_p, None);
        assert_eq!(p.max_tokens, Some(8192));
        assert_eq!(p.max_tokens_field, MaxTokensField::MaxCompletionTokens);
        assert_eq!(p.seed, Some(42));
        assert_eq!(p.system_prompt, None);
        assert_eq!(p.user_template.as_deref(), Some("Q: {prompt}"));
        assert_eq!(p.extra_body.get("k"), Some(&json!(1)));
        assert!(!p.stream);
        assert_eq!((p.think_open.as_str(), p.think_close.as_str()), ("<think>", "</think>"));
        assert_eq!((p.max_attempts, p.request_timeout_s, p.pause_after_failures), (4, 1800.0, 10));
        // No name, no extra body.
        let plain = to_body(&with(&[(Field::Endpoint, "1"), (Field::PromptSet, "1")])).unwrap();
        assert_eq!(plain.name, None);
        assert!(plain.params.extra_body.is_empty());
    }

    #[test]
    fn the_only_endpoint_and_prompt_set_are_chosen_with_their_defaults() {
        let eps = [endpoint(1, &["a"], 64, Some(true))];
        let sets = [set(2, "gsm8k", 100, "ready")];
        let first = initial_state(&eps, &sets, None, None, Prefill::default());
        assert_eq!(first.form.get(Field::Endpoint), "1");
        assert_eq!(first.form.get(Field::Concurrency), "64");
        assert_eq!(first.form.get(Field::Model), "a");
        assert_eq!(first.form.get(Field::PromptSet), "2");
        assert!(!first.prompt_open && !first.advanced_open);
    }

    #[test]
    fn with_several_nothing_is_guessed_but_the_prefills_are_used() {
        let eps = [endpoint(1, &["a", "b"], 16, None), endpoint(2, &["c", "d"], 8, None)];
        let sets = [set(5, "one", 10, "ready"), set(6, "two", 20, "importing"), set(7, "three", 30, "ready")];
        let none = initial_state(&eps, &sets, None, None, Prefill::default());
        assert_eq!(none.form.get(Field::Endpoint), "");
        assert_eq!(none.form.get(Field::PromptSet), "");
        let some = initial_state(&eps, &sets, None, None, Prefill { from: None, endpoint: Some(2), prompt_set: Some(7) });
        assert_eq!(some.form.get(Field::Endpoint), "2");
        assert_eq!(some.form.get(Field::Concurrency), "8");
        // Two listed models: a guess is never made.
        assert_eq!(some.form.get(Field::Model), "");
        assert_eq!(some.form.get(Field::PromptSet), "7");
        // A prompt set still importing cannot be started from.
        let importing = initial_state(&eps, &sets, None, None, Prefill { from: None, endpoint: None, prompt_set: Some(6) });
        assert_eq!(importing.form.get(Field::PromptSet), "");
    }

    #[test]
    fn the_model_is_the_one_the_last_run_used() {
        let ep = endpoint(1, &["a", "b"], 16, None);
        let runs = [run(1, "b", json!({})), run(1, "a", json!({}))];
        assert_eq!(suggested_model(&ep, Some(&runs)), "b");
        // A model the endpoint no longer lists is not suggested, unless it lists none.
        let gone = [run(1, "zzz", json!({}))];
        assert_eq!(suggested_model(&ep, Some(&gone)), "");
        let bare = endpoint(1, &[], 16, None);
        assert_eq!(suggested_model(&bare, Some(&gone)), "zzz");
        // Another endpoint's runs do not count.
        let other = [run(9, "a", json!({}))];
        assert_eq!(suggested_model(&ep, Some(&other)), "");
        assert_eq!(suggested_model(&endpoint(1, &["only"], 16, None), None), "only");
    }

    #[test]
    fn choosing_another_endpoint_keeps_a_model_it_also_lists() {
        let listed = endpoint(2, &["a", "b"], 8, None);
        assert_eq!(model_after_endpoint("a", Some(&listed), None), "a");
        assert_eq!(model_after_endpoint("typed", Some(&listed), None), "");
        // An endpoint that lists nothing keeps a typed id.
        assert_eq!(model_after_endpoint("typed", Some(&endpoint(3, &[], 8, None)), None), "typed");
        assert_eq!(model_after_endpoint("typed", None, None), "typed");
    }

    #[test]
    fn run_again_copies_the_settings_and_opens_the_groups_that_differ() {
        let eps = [endpoint(1, &["a"], 16, None)];
        let sets = [set(2, "gsm8k", 100, "ready")];
        let params = json!({
            "samples_per_prompt": 3, "temperature": 1.0, "top_p": 0.9, "max_tokens": null,
            "max_tokens_field": "max_completion_tokens", "seed": 7, "system_prompt": "Be brief.",
            "user_template": null, "extra_body": {"a": 1}, "stream": false, "think_open": "<t>",
            "think_close": "</t>", "max_attempts": 2, "request_timeout_s": 90.0, "pause_after_failures": 0
        });
        let source = run(1, "a", params);
        let first = initial_state(&eps, &sets, None, Some(&source), Prefill { from: Some(7), ..Prefill::default() });
        let f = &first.form;
        assert_eq!(f.get(Field::Endpoint), "1");
        assert_eq!(f.get(Field::PromptSet), "2");
        assert_eq!(f.get(Field::Concurrency), "8");
        assert_eq!(f.get(Field::Samples), "3");
        assert_eq!(f.get(Field::Temperature), "1");
        assert_eq!(f.get(Field::TopP), "0.9");
        assert_eq!(f.get(Field::MaxTokens), "");
        assert_eq!(f.max_tokens_field, MaxTokensField::MaxCompletionTokens);
        assert_eq!(f.get(Field::Seed), "7");
        assert_eq!(f.get(Field::SystemPrompt), "Be brief.");
        assert_eq!(f.get(Field::ExtraBody), "{\n  \"a\": 1\n}");
        assert_eq!(f.get(Field::Timeout), "90");
        assert!(!f.stream);
        assert!(first.prompt_open && first.advanced_open);
        // An endpoint that was deleted, or a set that is not ready, is left for the user to choose.
        let gone = initial_state(&[], &[set(2, "gsm8k", 100, "failed")], None, Some(&source), Prefill::default());
        assert_eq!(gone.form.get(Field::Endpoint), "");
        assert_eq!(gone.form.get(Field::PromptSet), "");
        // A run with the default settings opens neither group.
        let plain = run(1, "a", json!({}));
        let calm = initial_state(&eps, &sets, None, Some(&plain), Prefill::default());
        assert!(!calm.prompt_open && !calm.advanced_open);
    }

    #[test]
    fn timeouts_in_words() {
        assert_eq!(seconds(1800.0), "30 min");
        assert_eq!(seconds(3600.0), "1 h");
        assert_eq!(seconds(5400.0), "1 h 30 min");
        assert_eq!(seconds(90.0), "1 min 30 s");
        assert_eq!(seconds(60.0), "1 min");
        assert_eq!(seconds(45.0), "45.0 s");
        assert_eq!(seconds(0.5), "500 ms");
    }

    #[test]
    fn the_summary_counts_requests_and_seqs() {
        let s = set(1, "gsm8k", 1284, "ready");
        assert_eq!(run_summary(Some(&s), "2", "16").as_deref(), Some("2,568 requests · 16 seqs"));
        assert_eq!(run_summary(Some(&set(1, "x", 1, "ready")), "1", "8").as_deref(), Some("1 request · 8 seqs"));
        assert_eq!(run_summary(Some(&s), "1", "").as_deref(), Some("1,284 requests"));
        assert_eq!(run_summary(Some(&s), "1", "0").as_deref(), Some("1,284 requests"));
        assert_eq!(run_summary(Some(&s), "0", "16"), None);
        assert_eq!(run_summary(Some(&s), "", "16"), None);
        assert_eq!(run_summary(None, "1", "16"), None);
    }

    #[test]
    fn the_group_summaries_say_what_is_set() {
        let f = Form::default();
        assert_eq!(advanced_summary(&f), "Streaming, 4 attempts per sample, 30 min timeout, pause after 10 failures in a row");
        let mut g = with(&[(Field::MaxAttempts, "1"), (Field::Timeout, ""), (Field::PauseAfter, "0"), (Field::ExtraBody, "{}")]);
        g.stream = false;
        assert_eq!(advanced_summary(&g), "Not streaming, 1 attempt per sample, never pause on failures, extra request body set");
        assert_eq!(prompt_summary(&f), "None");
        assert_eq!(prompt_summary(&with(&[(Field::SystemPrompt, "x")])), "System prompt replaced");
        assert_eq!(prompt_summary(&with(&[(Field::SystemPrompt, "x"), (Field::UserTemplate, "{prompt}")])), "System prompt replaced, prompt template set");
        assert_eq!(prompt_summary(&with(&[(Field::UserTemplate, "{prompt}")])), "prompt template set");
    }

    #[test]
    fn names_and_notes() {
        let s = set(1, "gsm8k", 10, "ready");
        assert_eq!(default_name("m", Some(&s)), "m on gsm8k");
        assert_eq!(default_name("", Some(&s)), "");
        assert_eq!(default_name("m", None), "");
        let sets = [set(1, "a", 1, "ready"), set(2, "a", 2, "ready"), set(3, "b", 3, "ready")];
        let names = shared_names(&sets);
        assert_eq!((names["a"], names["b"]), (2, 1));
        assert_eq!(prompt_set_note(&sets[2], 1), "3 prompts");
        assert_eq!(prompt_set_note(&set(4, "c", 0, "importing"), 1), "Importing");
        assert_eq!(prompt_set_note(&set(5, "c", 0, "failed"), 2), "Failed");
        assert!(prompt_set_note(&sets[0], 2).starts_with("1 prompt · "));
    }
}
