//! The API contract's validation. The request and response types live in the `stellarator-api`
//! crate (crates/api), which has no server dependencies so every client (the nsh app, also in its
//! web build) can use them. They are re-exported here, so `stellarator_core::schemas::RunOut` and
//! every other path stay as they were. The checks and their messages match the Python backend
//! this replaced.

pub use stellarator_api::*;

use crate::error::{FieldErrors, Validate};
use crate::no_validation;

fn check_len(errors: &mut FieldErrors, field: &str, value: &str, min: usize, max: usize) {
    let n = value.chars().count();
    if n < min {
        errors.add(
            field,
            if min == 1 {
                "Required.".to_string()
            } else {
                format!("At least {min} characters.")
            },
        );
    } else if n > max {
        errors.add(field, format!("At most {max} characters."));
    }
}

fn check_range_i(errors: &mut FieldErrors, field: &str, v: i64, min: i64, max: i64) {
    if v < min || v > max {
        errors.add(field, format!("Use a whole number from {min} to {max}."));
    }
}

/// Strip and drop a trailing slash; must start with http:// or https://.
pub fn normalize_base_url(errors: &mut FieldErrors, field: &str, url: &mut String) {
    let v = url.trim().trim_end_matches('/').to_string();
    if !(v.starts_with("http://") || v.starts_with("https://")) {
        errors.add(field, "Base URL must start with http:// or https://");
    }
    *url = v;
}

impl Validate for EndpointIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 200);
        check_len(e, "base_url", &self.base_url, 1, 2000);
        normalize_base_url(e, "base_url", &mut self.base_url);
        check_range_i(e, "default_concurrency", self.default_concurrency, 1, 4096);
    }
}

impl Validate for EndpointPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            check_len(e, "name", n, 1, 200);
        }
        if let Some(u) = &mut self.base_url {
            normalize_base_url(e, "base_url", u);
        }
        if let Some(c) = self.default_concurrency {
            check_range_i(e, "default_concurrency", c, 1, 4096);
        }
    }
}

impl Validate for EndpointTestIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        normalize_base_url(e, "base_url", &mut self.base_url);
    }
}

impl Validate for PromptSetPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 300);
    }
}

/// The mapping's own checks (offset, limit).
fn check_mapping(m: &ImportMapping, e: &mut FieldErrors) {
    if m.offset < 0 {
        e.add("mapping.offset", "Use 0 or more.");
    }
    if let Some(l) = m.limit {
        if l < 1 {
            e.add(
                "mapping.limit",
                "Use 1 or more, or leave empty to import all.",
            );
        }
    }
}

impl Validate for ImportPreviewIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_mapping(&self.mapping, e);
        if let ImportSource::Huggingface { dataset, .. } = &self.source {
            if dataset.trim().is_empty() {
                e.add("source.dataset", "Required.");
            }
        }
    }
}

impl Validate for ImportCommitIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_mapping(&self.mapping, e);
        check_len(e, "name", &self.name, 1, 300);
        if let ImportSource::Huggingface { dataset, .. } = &self.source {
            if dataset.trim().is_empty() {
                e.add("source.dataset", "Required.");
            }
        }
    }
}

/// A run's parameters, under `prefix` ("params.").
pub fn check_run_params(params: &mut RunParams, prefix: &str, e: &mut FieldErrors) {
    let f = |name: &str| format!("{prefix}{name}");
    check_range_i(
        e,
        &f("samples_per_prompt"),
        params.samples_per_prompt,
        1,
        64,
    );
    if let Some(t) = params.temperature {
        if !(0.0..=2.0).contains(&t) {
            e.add(&f("temperature"), "Use a number from 0 to 2.");
        }
    }
    if let Some(p) = params.top_p {
        if !(p > 0.0 && p <= 1.0) {
            e.add(&f("top_p"), "Use a number above 0, at most 1.");
        }
    }
    if let Some(m) = params.max_tokens {
        if m < 1 {
            e.add(&f("max_tokens"), "Use 1 or more.");
        }
    }
    check_range_i(e, &f("max_attempts"), params.max_attempts, 1, 20);
    if params.request_timeout_s < 10.0 {
        e.add(&f("request_timeout_s"), "Use at least 10 seconds.");
    }
    if params.pause_after_failures < 0 {
        e.add(&f("pause_after_failures"), "Use 0 or more.");
    }
    // An empty template means none.
    if let Some(t) = &params.user_template {
        if t.trim().is_empty() {
            params.user_template = None;
        } else if !t.contains("{prompt}") {
            e.add(
                &f("user_template"),
                "The prompt template must contain {prompt}",
            );
        }
    }
    if params
        .system_prompt
        .as_deref()
        .is_some_and(|s| s.is_empty())
    {
        params.system_prompt = None;
    }
    if let Some(tools) = &mut params.tools {
        check_run_tools(tools, &f("tools."), e);
    }
}

/// A run's tools section, under `prefix` ("params.tools."). Whether the servers and tools exist is
/// checked when the run is created (it needs the database).
pub fn check_run_tools(tools: &mut RunTools, prefix: &str, e: &mut FieldErrors) {
    let f = |name: &str| format!("{prefix}{name}");
    if tools.servers.is_empty() {
        e.add(&f("servers"), "Pick at least one MCP server, or turn tools off.");
    }
    let mut seen = std::collections::HashSet::new();
    for (i, s) in tools.servers.iter_mut().enumerate() {
        if !seen.insert(s.server_id) {
            e.add(&f(&format!("servers.{i}.server_id")), "This server is picked twice.");
        }
        if let Some(names) = &mut s.tools {
            names.retain(|n| !n.trim().is_empty());
            let mut uniq = std::collections::HashSet::new();
            names.retain(|n| uniq.insert(n.clone()));
            if names.is_empty() {
                e.add(
                    &f(&format!("servers.{i}.tools")),
                    "Pick at least one tool of this server, or leave the list out to offer all of them.",
                );
            }
        }
    }
    check_range_i(e, &f("max_rounds"), tools.max_rounds, 1, 32);
    if !(1.0..=3600.0).contains(&tools.tool_timeout_s) {
        e.add(&f("tool_timeout_s"), "Use a number of seconds from 1 to 3600.");
    }
    check_range_i(e, &f("max_result_chars"), tools.max_result_chars, 1000, 1_000_000);
    if tools.mode == ToolMode::Code {
        if let Some(code) = &mut tools.code {
            check_code_options(code, &f("code."), e);
        }
    }
}

/// Code mode settings, under `prefix` ("params.tools.code.").
pub fn check_code_options(code: &mut CodeModeOptions, prefix: &str, e: &mut FieldErrors) {
    let f = |name: &str| format!("{prefix}{name}");
    code.tool_name = code.tool_name.trim().to_string();
    let name_ok = (1..=64).contains(&code.tool_name.len())
        && code.tool_name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !name_ok {
        e.add(&f("tool_name"), "Use 1 to 64 letters, digits, _ or -.");
    }
    // An empty description means Cloudflare's default.
    if code.description.as_deref().is_some_and(|d| d.trim().is_empty()) {
        code.description = None;
    }
    if code.description.as_deref().is_some_and(|d| d.chars().count() > 100_000) {
        e.add(&f("description"), "At most 100,000 characters.");
    }
    if !(1.0..=3600.0).contains(&code.timeout_s) {
        e.add(&f("timeout_s"), "Use a number of seconds from 1 to 3600.");
    }
    if code.max_result_tokens.is_some_and(|t| t != 0 && !(100..=1_000_000).contains(&t)) {
        e.add(&f("max_result_tokens"), "Use 0 (no truncation) or a number from 100 to 1,000,000.");
    }
    if let Some(chars) = code.max_result_chars {
        check_range_i(e, &f("max_result_chars"), chars, 1000, 4_000_000);
    }
    check_range_i(e, &f("memory_limit_mb"), code.memory_limit_mb, 8, 1024);
    match code.format {
        CodeFormat::CodeTool => {
            // The runtime's settings mean nothing here: refuse them instead of dropping them.
            if !code.connector_hints.is_empty() {
                e.add(&f("connector_hints"), "Connector hints belong to the runtime format. Set format to \"runtime\", or leave them out.");
            }
        }
        CodeFormat::Runtime => {
            if code.namespaces == CodeNamespaces::Codemode {
                e.add(
                    &f("namespaces"),
                    "The runtime format has one connector per server, and `codemode` is its own namespace. Use \"servers\".",
                );
            }
            code.connector_hints = std::mem::take(&mut code.connector_hints)
                .into_iter()
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .filter(|(_, v)| !v.is_empty())
                .collect();
            if code.connector_hints.keys().any(String::is_empty) {
                e.add(&f("connector_hints"), "Name the server each hint is for.");
            } else if code.connector_hints.values().any(|v| v.chars().count() > 2000 || v.contains('\n')) {
                e.add(&f("connector_hints"), "A hint is one line of at most 2,000 characters.");
            }
        }
    }
}

/// MCP server names: letters, digits, `_` and `-`, 1 to 64 characters. They prefix tool names when
/// two servers have a tool of the same name, and tool names allow only these characters.
pub fn check_mcp_name(errors: &mut FieldErrors, field: &str, name: &str) {
    if name.is_empty() {
        errors.add(field, "Required.");
    } else if name.chars().count() > 64 {
        errors.add(field, "At most 64 characters.");
    } else if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        errors.add(field, "Use only letters, digits, _ and -.");
    }
}

/// Environment variable names: no `=` or NUL, not empty.
pub fn check_env_names<'a>(errors: &mut FieldErrors, field: &str, names: impl Iterator<Item = &'a String>) {
    for n in names {
        if n.is_empty() || n.contains('=') || n.contains('\0') {
            errors.add(field, format!("\"{n}\" is not a valid environment variable name."));
        }
    }
}

/// HTTP header names: the token characters of RFC 9110.
pub fn check_header_names<'a>(errors: &mut FieldErrors, field: &str, names: impl Iterator<Item = &'a String>) {
    for n in names {
        let ok = !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        if !ok {
            errors.add(field, format!("\"{n}\" is not a valid HTTP header name."));
        }
    }
}

fn check_mcp_url(errors: &mut FieldErrors, field: &str, url: &mut String) {
    let v = url.trim().to_string();
    if !(v.starts_with("http://") || v.starts_with("https://")) {
        errors.add(field, "The URL must start with http:// or https://");
    }
    *url = v;
}

impl Validate for McpServerIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_mcp_name(e, "name", &self.name);
        self.command = self.command.take().map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        self.url = self.url.take().map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
        match self.transport {
            McpTransport::Stdio => {
                if self.command.is_none() {
                    e.add("command", "Enter the command that starts the server, for example npx.");
                }
            }
            McpTransport::Http => match &mut self.url {
                None => e.add("url", "Enter the server's URL, for example https://example.com/mcp."),
                Some(u) => check_mcp_url(e, "url", u),
            },
        }
        check_env_names(e, "env", self.env.keys());
        check_header_names(e, "headers", self.headers.keys());
    }
}

impl Validate for McpServerPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            check_mcp_name(e, "name", n);
        }
        if let Some(c) = &mut self.command {
            *c = c.trim().to_string();
        }
        if let Some(u) = &mut self.url {
            if !u.trim().is_empty() {
                check_mcp_url(e, "url", u);
            }
        }
        if let Some(env) = &self.env {
            check_env_names(e, "env", env.keys());
        }
        if let Some(h) = &self.headers {
            check_header_names(e, "headers", h.keys());
        }
    }
}

impl Validate for McpImportIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        if self.text.trim().is_empty() {
            e.add("text", "Paste the JSON of one or more MCP servers.");
        }
    }
}

impl Validate for RunCreate {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            if n.trim().is_empty() {
                self.name = None;
            } else {
                check_len(e, "name", n, 0, 300);
            }
        }
        check_len(e, "model", &self.model, 1, 500);
        check_range_i(e, "concurrency", self.concurrency, 1, 4096);
        check_run_params(&mut self.params, "params.", e);
    }
}

impl Validate for RunPatch {
    fn validate(&mut self, e: &mut FieldErrors) {
        if let Some(n) = &self.name {
            check_len(e, "name", n, 1, 300);
        }
        if let Some(c) = self.concurrency {
            check_range_i(e, "concurrency", c, 1, 4096);
        }
    }
}

no_validation!(ExportOptions, PrefExportOptions);

/// A Hugging Face push target, under `prefix`.
pub fn check_hub_target(h: &HubPushIn, prefix: &str, e: &mut FieldErrors) {
    check_len(e, &format!("{prefix}repo_id"), &h.repo_id, 3, 200);
}

impl Validate for RunHubIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_hub_target(&self.target, "target.", e);
    }
}

impl Validate for PrefHubIn {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_hub_target(&self.target, "target.", e);
    }
}

impl Validate for PreferenceSetCreate {
    fn validate(&mut self, e: &mut FieldErrors) {
        check_len(e, "name", &self.name, 1, 300);
        if self.run_ids.is_empty() {
            e.add("run_ids", "Pick at least one run.");
        }
    }
}

no_validation!(PairIn);

no_validation!(AutoPairIn);

no_validation!(SettingsIn);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::parse_json;

    #[test]
    fn run_params_defaults_match_python() {
        let p = RunParams::default();
        assert_eq!(p.samples_per_prompt, 1);
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.max_tokens, Some(8192));
        assert!(p.stream);
        assert_eq!(p.think_open, "<think>");
        assert_eq!(p.max_attempts, 4);
        assert_eq!(p.pause_after_failures, 10);
        // An explicit null clears a defaulted field.
        let p: RunParams = serde_json::from_str(r#"{"temperature": null}"#).unwrap();
        assert_eq!(p.temperature, None);
    }

    #[test]
    fn endpoint_validation_reports_fields() {
        let mut v: EndpointIn =
            parse_json(br#"{"name": "x", "base_url": "localhost:8000"}"#).unwrap();
        let mut e = FieldErrors::new();
        v.validate(&mut e);
        let err = e.finish().unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert_eq!(
            err.fields["base_url"],
            "Base URL must start with http:// or https://"
        );
    }

    #[test]
    fn code_mode_options_are_checked() {
        let mut tools: RunTools = serde_json::from_value(serde_json::json!({
            "servers": [{"server_id": 1}], "mode": "code",
            "code": {"tool_name": "run code", "description": "  ", "timeout_s": 0.5, "max_result_tokens": 50,
                     "max_result_chars": 10, "memory_limit_mb": 4}
        }))
        .unwrap();
        let mut e = FieldErrors::new();
        check_run_tools(&mut tools, "params.tools.", &mut e);
        let err = e.finish().unwrap_err();
        let mut fields: Vec<&str> = err.fields.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "params.tools.code.max_result_chars",
                "params.tools.code.max_result_tokens",
                "params.tools.code.memory_limit_mb",
                "params.tools.code.timeout_s",
                "params.tools.code.tool_name",
            ]
        );
        assert_eq!(tools.code.unwrap().description, None);
        // The runtime format: one connector per server, hints of one line, trimmed, empty ones dropped.
        let check = |code: serde_json::Value| {
            let mut t: RunTools =
                serde_json::from_value(serde_json::json!({"servers": [{"server_id": 1}], "mode": "code", "code": code})).unwrap();
            let mut e = FieldErrors::new();
            check_run_tools(&mut t, "params.tools.", &mut e);
            (t, e.finish().err().map(|x| x.fields.keys().cloned().collect::<Vec<_>>()).unwrap_or_default())
        };
        let (t, errors) = check(serde_json::json!({"format": "runtime", "connector_hints": {" github ": " Issues. ", "docs": "  "}}));
        assert!(errors.is_empty(), "{errors:?}");
        let code = t.code.unwrap();
        assert_eq!(code.format, CodeFormat::Runtime);
        assert_eq!(code.connector_hints.into_iter().collect::<Vec<_>>(), [("github".to_string(), "Issues.".to_string())]);
        assert_eq!(check(serde_json::json!({"format": "runtime", "namespaces": "codemode"})).1, ["params.tools.code.namespaces"]);
        assert_eq!(
            check(serde_json::json!({"format": "runtime", "connector_hints": {"github": "two\nlines"}})).1,
            ["params.tools.code.connector_hints"]
        );
        assert_eq!(check(serde_json::json!({"connector_hints": {"github": "x"}})).1, ["params.tools.code.connector_hints"]);
        assert!(serde_json::from_value::<RunTools>(serde_json::json!({"servers": [], "code": {"format": "durable"}})).is_err());
        // 0 tokens turns truncation off; direct mode ignores the code settings.
        let mut ok: RunTools = serde_json::from_value(serde_json::json!({
            "servers": [{"server_id": 1}], "mode": "code", "code": {"max_result_tokens": 0}
        }))
        .unwrap();
        let mut e = FieldErrors::new();
        check_run_tools(&mut ok, "params.tools.", &mut e);
        assert!(e.finish().is_ok());
        let mut direct: RunTools = serde_json::from_value(serde_json::json!({
            "servers": [{"server_id": 1}], "code": {"memory_limit_mb": 1}
        }))
        .unwrap();
        let mut e = FieldErrors::new();
        check_run_tools(&mut direct, "params.tools.", &mut e);
        assert!(e.finish().is_ok());
    }

    #[test]
    fn missing_field_has_path() {
        let err =
            parse_json::<RunCreate>(br#"{"endpoint_id": 1, "prompt_set_id": 2}"#).unwrap_err();
        assert_eq!(
            err.fields.get("model").map(String::as_str),
            Some("Field required.")
        );
    }

    #[test]
    fn import_source_tagged() {
        let s: ImportSource = serde_json::from_str(
            r#"{"kind":"huggingface","dataset":"openai/gsm8k","all_splits":true}"#,
        )
        .unwrap();
        assert!(matches!(
            s,
            ImportSource::Huggingface {
                all_splits: true,
                ..
            }
        ));
    }

    #[test]
    fn missing_mapping_keeps_dedupe_on() {
        let v: ImportPreviewIn =
            serde_json::from_str(r#"{"source":{"kind":"paste","text":"a"}}"#).unwrap();
        assert!(v.mapping.dedupe);
        assert!(ImportMapping::default().dedupe);
    }

    #[test]
    fn query_errors_name_the_parameter() {
        let err = crate::error::parse_query::<ExportOptions>("format=nope").unwrap_err();
        assert_eq!(err.status.as_u16(), 422);
        assert!(err.fields.contains_key("format"), "{:?}", err.fields);
        assert_eq!(
            err.detail,
            "\"nope\" is not one of: messages, sharegpt, alpaca, prompt_completion, raw."
        );
        let ok: ExportOptions =
            crate::error::parse_query("format=sharegpt&exclude_truncated=false").unwrap();
        assert!(!ok.exclude_truncated);
    }

    #[test]
    fn export_options_from_query_strings() {
        let o: ExportOptions =
            serde_json::from_value(serde_json::json!({"exclude_truncated": "false"})).unwrap();
        assert!(!o.exclude_truncated);
    }
}
