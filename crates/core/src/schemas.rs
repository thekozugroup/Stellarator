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
