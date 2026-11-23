//! Export pairs (frontend/src/components/preferences/export-dialog.tsx), on the export dialogs'
//! shared layout (`screens::export`): the format (DPO or KTO), the shape (message lists or
//! strings), where the thinking trace goes, the system prompt, the file type. The preview is read
//! again at once after each change. The options outlive the dialog, as the web UI's do.
//!
//! A set whose runs used tools compares traces: plain text has no place for tool calls, so that
//! shape is off (the server answers 422 for it) and says why.

use std::rc::Rc;
use std::time::Duration;

use nsh_core::prelude::*;

use crate::api::types::{FileType, PrefExportOptions, PrefFormat, ReasoningPlacement};
use std::rc::Rc as Shared;

use crate::data::{SignalExt, call};
use crate::screens::export::{
    Choice, ExportSpec, check_option, choice_group, choice_group_gated, export_dialog as shell, file_type_group, option_group, options_note,
    placement_label,
};

/// Message lists (TRL's conversational format) or plain strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Conversational,
    Plain,
}

const FORMATS: [Choice<PrefFormat>; 2] = [
    Choice { value: PrefFormat::Dpo, label: "DPO", hint: Some("`prompt`, `chosen`, `rejected`. 1 record per pair.") },
    Choice { value: PrefFormat::Kto, label: "KTO", hint: Some("`prompt`, `completion`, `label`. 2 records per pair.") },
];

const SHAPES: [Choice<Shape>; 2] = [
    Choice { value: Shape::Conversational, label: "Conversational", hint: Some("Message lists.") },
    Choice { value: Shape::Plain, label: "Plain text", hint: Some("Strings.") },
];

/// Where the thinking trace goes: the four placements, without hints.
fn placements() -> Vec<Choice<ReasoningPlacement>> {
    [ReasoningPlacement::ThinkTags, ReasoningPlacement::ReasoningContent, ReasoningPlacement::Thinking, ReasoningPlacement::Omit]
        .into_iter()
        .map(|value| Choice { value, label: placement_label(value), hint: None })
        .collect()
}

/// The web UI's defaults.
pub fn default_options() -> PrefExportOptions {
    PrefExportOptions {
        format: PrefFormat::Dpo,
        file_type: FileType::Jsonl,
        reasoning: ReasoningPlacement::ThinkTags,
        include_system: true,
        conversational: true,
    }
}

/// Why `shape` cannot export a set whose runs used tools, or `None`.
fn off_with_tools(shape: Shape) -> Option<&'static str> {
    (shape == Shape::Plain).then_some("Not for runs with tools: strings have no place for tool calls.")
}

/// The export dialog of preference set `set_id`, shown while `open` holds. `run_ids` are the set's
/// runs: each opening reads whether any used tools.
pub fn export_dialog(set_id: i64, open: Signal<bool>, run_ids: Shared<dyn Fn() -> Vec<i64>>) -> impl IntoElement {
    let options = signal(default_options());
    let tools = signal(false);
    effect(move || {
        if !open.get() {
            return;
        }
        let ids = untracked(|| run_ids());
        call(|api| async move { api.runs().await }, move |r| {
            if let Ok(runs) = r {
                let used = runs.iter().any(|run| ids.contains(&run.id) && run.tools.is_some());
                tools.try_set(used);
                if used && !options.with_untracked(|o| o.conversational) {
                    options.try_update(|o| o.conversational = true);
                }
            }
        });
    });
    let spec = ExportSpec {
        title: "Export pairs",
        base: format!("/api/preference-sets/{set_id}/export"),
        options: Rc::new(move || serde_json::to_value(options.get()).unwrap_or_default()),
        file_type: Rc::new(move || options.with(|o| o.file_type)),
        debounce: Duration::ZERO,
        empty_text: "No pairs yet.",
        repo_example: "your-name/math-dpo-pairs",
    };
    shell(open, spec, move || {
        vec![
            dynamic(move || {
                tools.get().then(|| options_note("These runs used tools: each record’s messages include the calls and their results."))
            })
            .into_element(),
            choice_group("Format", FORMATS.to_vec(), move || options.with(|o| o.format), move |v| options.update(|o| o.format = v)),
            dynamic(move || {
                let gate = if tools.get() { off_with_tools } else { |_: Shape| None };
                choice_group_gated(
                    "Shape",
                    SHAPES.to_vec(),
                    gate,
                    move || if options.with(|o| o.conversational) { Shape::Conversational } else { Shape::Plain },
                    move |v| options.update(|o| o.conversational = v == Shape::Conversational),
                )
            })
            .into_element(),
            choice_group("Thinking trace", placements(), move || options.with(|o| o.reasoning), move |v| {
                options.update(|o| o.reasoning = v)
            }),
            option_group(
                "Prompts",
                vec![check_option("Include system prompt", None, move || options.with(|o| o.include_system), move |v| {
                    options.update(|o| o.include_system = v)
                })],
            ),
            file_type_group(move || options.with(|o| o.file_type), move |v| options.update(|o| o.file_type = v)),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_web_uis_and_serialize_as_the_api_reads_them() {
        let json = serde_json::to_value(default_options()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "format": "dpo", "file_type": "jsonl", "reasoning": "think_tags",
                "include_system": true, "conversational": true
            })
        );
        assert_eq!(placements().len(), 4);
        assert_eq!(placements()[0].label, "Inline `<think>` tags");
    }
}
