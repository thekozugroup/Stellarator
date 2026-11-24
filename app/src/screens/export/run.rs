//! Export a run as a training dataset (frontend/src/components/runs/export-dialog.tsx): the format,
//! where the thinking trace goes, which samples, the file type. Each change reads the preview again
//! after 250 ms.
//!
//! A run with tools exports its traces (each record holds the tool definitions, the calls and their
//! results), so only the conversational formats are offered: Alpaca and Prompt-completion show why
//! they are off, and a note above the options says what the records include.

use std::rc::Rc;
use std::time::Duration;

use nsh_core::prelude::*;

use super::{Choice, ExportSpec, check_option, choice_group, choice_group_gated, export_dialog, file_type_group, option_group, options_note, placement_label};
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

/// Why `format` cannot export a run with tools (the server answers 422 for it), or `None`.
pub fn off_with_tools(format: SftFormat) -> Option<&'static str> {
    match format {
        SftFormat::Alpaca | SftFormat::PromptCompletion => Some("Not for runs with tools: it has no place for tool calls."),
        _ => None,
    }
}

/// The note above a run's options: what a record holds when the run used tools, `None` without
/// tools. In code mode (`code`) the tool is the code tool, and its call and result are what the
/// model wrote and saw: the tool calls the code made are not records of their own.
pub fn tools_note(with_tools: bool, code: bool) -> Option<&'static str> {
    match (with_tools, code) {
        (false, _) => None,
        (true, false) => Some("This run used tools: each record includes the tool definitions, the calls and their results."),
        (true, true) => Some(
            "This run used code mode: each record includes the code tool, the code the model wrote and its results, as the model saw them. The tool calls the code made are not records of their own.",
        ),
    }
}

/// The export dialog of run `run_id`, shown while `open` holds. A run that takes one sample per
/// prompt (`single_sample`) has no others to leave out, so that option is not offered. A run with
/// tools (`with_tools`) offers the conversational formats only; in code mode (`code`) the note says
/// what its records hold.
pub fn run_export_dialog(open: Signal<bool>, run_id: i64, single_sample: bool, with_tools: bool, code: bool) -> impl IntoElement {
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
        // A format the run cannot export (chosen before it had tools) goes back to the default.
        if with_tools && off_with_tools(options.with_untracked(|o| o.format)).is_some() {
            options.update(|o| o.format = SftFormat::Messages);
        }
        let format = if with_tools {
            choice_group_gated("Format", FORMATS.to_vec(), off_with_tools, move || options.with(|o| o.format), move |v| options.update(|o| o.format = v))
        } else {
            choice_group("Format", FORMATS.to_vec(), move || options.with(|o| o.format), move |v| options.update(|o| o.format = v))
        };
        let mut groups = Vec::new();
        if let Some(note) = tools_note(with_tools, code) {
            groups.push(options_note(note));
        }
        groups.extend([
            format,
            choice_group("Thinking trace", placements(), move || options.with(|o| o.reasoning), move |v| {
                options.update(|o| o.reasoning = v)
            }),
            option_group("Samples", samples),
            file_type_group(move || options.with(|o| o.file_type), move |v| options.update(|o| o.file_type = v)),
        ]);
        groups
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_says_what_a_records_tool_calls_are() {
        assert_eq!(tools_note(false, false), None);
        assert!(tools_note(true, false).unwrap().contains("the tool definitions, the calls and their results"));
        let code = tools_note(true, true).unwrap();
        assert!(code.contains("code tool") && code.contains("as the model saw them"));
    }

    #[test]
    fn runs_with_tools_export_conversations_only() {
        assert!(off_with_tools(SftFormat::Alpaca).is_some());
        assert!(off_with_tools(SftFormat::PromptCompletion).is_some());
        assert_eq!(off_with_tools(SftFormat::Messages), None);
        assert_eq!(off_with_tools(SftFormat::Sharegpt), None);
        assert_eq!(off_with_tools(SftFormat::Raw), None);
    }
}
