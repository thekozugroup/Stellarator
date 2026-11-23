//! Export a run as a training dataset (frontend/src/components/runs/export-dialog.tsx): the format,
//! where the thinking trace goes, which samples, the file type. Each change reads the preview again
//! after 250 ms.

use std::rc::Rc;
use std::time::Duration;

use nsh_core::prelude::*;

use super::{Choice, ExportSpec, check_option, choice_group, export_dialog, file_type_group, option_group, placement_label};
use crate::api::types::{ExportOptions, FileType, ReasoningPlacement, SftFormat};

const FORMATS: [Choice<SftFormat>; 5] = [
    Choice { value: SftFormat::Messages, label: "Chat messages", hint: Some("For TRL, Axolotl, Unsloth.") },
    Choice { value: SftFormat::Sharegpt, label: "ShareGPT", hint: Some("For LLaMA-Factory, Axolotl.") },
    Choice { value: SftFormat::Alpaca, label: "Alpaca", hint: Some("Instruction, input and output fields.") },
    Choice { value: SftFormat::PromptCompletion, label: "Prompt-completion", hint: Some("For TRL.") },
    Choice { value: SftFormat::Raw, label: "Raw", hint: Some("Every field, with tokens and timings.") },
];

fn placements() -> Vec<Choice<ReasoningPlacement>> {
    vec![
        Choice {
            value: ReasoningPlacement::ThinkTags,
            label: placement_label(ReasoningPlacement::ThinkTags),
            hint: Some("Before the answer, in the assistant text."),
        },
        Choice {
            value: ReasoningPlacement::ReasoningContent,
            label: placement_label(ReasoningPlacement::ReasoningContent),
            hint: Some("As vLLM and DeepSeek return it."),
        },
        Choice {
            value: ReasoningPlacement::Thinking,
            label: placement_label(ReasoningPlacement::Thinking),
            hint: Some("For `gpt-oss` templates."),
        },
        Choice { value: ReasoningPlacement::Omit, label: placement_label(ReasoningPlacement::Omit), hint: None },
    ]
}

/// The web UI's defaults.
fn default_options() -> ExportOptions {
    ExportOptions {
        format: SftFormat::Messages,
        file_type: FileType::Jsonl,
        reasoning: ReasoningPlacement::ThinkTags,
        include_system: true,
        exclude_truncated: true,
        require_reasoning: false,
        first_sample_only: false,
    }
}

/// The export dialog of run `run_id`, shown while `open` holds. A run that takes one sample per
/// prompt (`single_sample`) has no others to leave out, so that option is not offered.
pub fn run_export_dialog(open: Signal<bool>, run_id: i64, single_sample: bool) -> impl IntoElement {
    let options = signal(default_options());
    let spec = ExportSpec {
        title: "Export dataset",
        base: format!("/api/runs/{run_id}/export"),
        options: Rc::new(move || serde_json::to_value(options.get()).unwrap_or_default()),
        file_type: Rc::new(move || options.with(|o| o.file_type)),
        debounce: Duration::from_millis(250),
        empty_text: "No samples match these options.",
        repo_example: "your-name/qwen3-math-distill",
    };
    export_dialog(open, spec, move || {
        let mut samples = vec![
            check_option("Skip truncated responses", None, move || options.with(|o| o.exclude_truncated), move |v| {
                options.update(|o| o.exclude_truncated = v)
            }),
            check_option("Only samples with a thinking trace", None, move || options.with(|o| o.require_reasoning), move |v| {
                options.update(|o| o.require_reasoning = v)
            }),
        ];
        if !single_sample {
            samples.push(check_option(
                "First sample per prompt only",
                None,
                move || options.with(|o| o.first_sample_only),
                move |v| options.update(|o| o.first_sample_only = v),
            ));
        }
        samples.push(check_option("Include system prompt", None, move || options.with(|o| o.include_system), move |v| {
            options.update(|o| o.include_system = v)
        }));
        vec![
            choice_group("Format", FORMATS.to_vec(), move || options.with(|o| o.format), move |v| options.update(|o| o.format = v)),
            choice_group("Thinking trace", placements(), move || options.with(|o| o.reasoning), move |v| {
                options.update(|o| o.reasoning = v)
            }),
            option_group("Samples", samples),
            file_type_group(move || options.with(|o| o.file_type), move |v| options.update(|o| o.file_type = v)),
        ]
    })
}
