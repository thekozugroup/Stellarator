//! Arrow schemas for the Parquet and CSV columns of each export format. Runs with tools add the
//! message fields of tool calls and results, and a `tools` column (JSON text: tool schemas differ
//! from tool to tool, and a Parquet column has one type).

use std::sync::Arc;

use arrow_schema::{DataType, Field, Fields, Schema};
use stellarator_core::schemas::{ExportOptions, PrefExportOptions, PrefFormat, SftFormat};

use crate::records::outside_key;

fn s() -> DataType {
    DataType::Utf8
}
fn i() -> DataType {
    DataType::Int64
}
fn f(name: &str, t: DataType) -> Field {
    Field::new(name, t, true)
}
/// A JSON text column marked with Arrow's canonical `arrow.json` extension type: Hugging Face
/// `datasets` 4.7 and later read it as JSON objects (its `Json()` feature), older readers as text.
fn json_text(name: &str) -> Field {
    let meta = [("ARROW:extension:name".to_string(), "arrow.json".to_string())];
    Field::new(name, DataType::Utf8, true).with_metadata(meta.into_iter().collect())
}
fn list(t: DataType) -> DataType {
    DataType::List(Arc::new(Field::new("item", t, true)))
}
fn structure(fields: Vec<Field>) -> DataType {
    DataType::Struct(Fields::from(fields))
}

/// A chat message; every struct in a column has the same fields (the reasoning field is null on
/// turns that have none) so Parquet readers see one type.
fn msg_type(extra: Option<&str>) -> DataType {
    let mut fields = vec![f("role", s()), f("content", s())];
    if let Some(e) = extra {
        fields.push(f(e, s()));
    }
    structure(fields)
}

/// A chat message that may carry tool calls (assistant) or be a tool result (`tool_call_id`,
/// `name`). Tool arguments are JSON text.
fn tool_msg_type(extra: Option<&str>) -> DataType {
    let function = structure(vec![f("name", s()), json_text("arguments")]);
    let call = structure(vec![f("id", s()), f("type", s()), f("function", function)]);
    let mut fields = vec![f("role", s()), f("content", s())];
    if let Some(e) = extra {
        fields.push(f(e, s()));
    }
    fields.extend([f("tool_calls", list(call)), f("tool_call_id", s()), f("name", s())]);
    structure(fields)
}

pub fn sft_schema(o: &ExportOptions) -> Arc<Schema> {
    sft_schema_for(o, false)
}

/// The columns of an SFT export; `tools`: the run has tools.
pub fn sft_schema_for(o: &ExportOptions, tools: bool) -> Arc<Schema> {
    let extra = outside_key(o.reasoning);
    let outside = extra.is_some();
    if tools {
        match o.format {
            SftFormat::Messages => {
                return Arc::new(Schema::new(vec![f("messages", list(tool_msg_type(extra))), json_text("tools")]))
            }
            SftFormat::Sharegpt => {
                let mut v = vec![
                    f("conversations", list(structure(vec![f("from", s()), f("value", s())]))),
                    f("tools", s()),
                ];
                if outside {
                    v.push(f("reasoning", s()));
                }
                return Arc::new(Schema::new(v));
            }
            _ => {}
        }
    }
    let mut fields = match o.format {
        SftFormat::Messages => vec![f("messages", list(msg_type(extra)))],
        SftFormat::PromptCompletion => {
            vec![f("prompt", list(msg_type(extra))), f("completion", list(msg_type(extra)))]
        }
        SftFormat::Sharegpt => {
            let mut v = vec![f("conversations", list(structure(vec![f("from", s()), f("value", s())])))];
            if outside {
                v.push(f("reasoning", s()));
            }
            v
        }
        SftFormat::Alpaca => {
            let mut v = vec![
                f("instruction", s()),
                f("input", s()),
                f("output", s()),
                f("system", s()),
                f("history", list(list(s()))),
            ];
            if outside {
                v.push(f("reasoning", s()));
            }
            v
        }
        SftFormat::Raw => vec![
            f("id", i()),
            f("run_id", i()),
            f("prompt_id", i()),
            f("prompt_index", i()),
            f("sample_index", i()),
            f("model", s()),
            f("endpoint", s()),
            f("messages", list(msg_type(None))),
            f("reasoning", s()),
            f("response", s()),
            f("finish_reason", s()),
            f(
                "usage",
                structure(vec![f("prompt_tokens", i()), f("completion_tokens", i()), f("reasoning_tokens", i())]),
            ),
            f("latency_ms", i()),
            f("ttft_ms", i()),
            // JSON-encoded in Parquet and CSV
            f("params", s()),
            f("meta", s()),
            f("created_at", s()),
        ],
    };
    if tools && o.format == SftFormat::Raw {
        // JSON-encoded in Parquet and CSV, like params and meta.
        fields.extend([f("tool_calls", i()), json_text("trace"), json_text("tools")]);
    }
    Arc::new(Schema::new(fields))
}

pub fn pref_schema(o: &PrefExportOptions) -> Arc<Schema> {
    pref_schema_for(o, false)
}

/// The columns of a preference export; `tools`: the set has runs with tools (conversational only).
pub fn pref_schema_for(o: &PrefExportOptions, tools: bool) -> Arc<Schema> {
    let outside = outside_key(o.reasoning).is_some();
    let dpo = o.format == PrefFormat::Dpo;
    if tools && o.conversational {
        let msgs = list(tool_msg_type(outside_key(o.reasoning)));
        let fields = if dpo {
            vec![
                f("prompt", list(msg_type(outside_key(o.reasoning)))),
                f("chosen", msgs.clone()),
                f("rejected", msgs),
                json_text("tools"),
            ]
        } else {
            vec![
                f("prompt", list(msg_type(outside_key(o.reasoning)))),
                f("completion", msgs),
                f("label", DataType::Boolean),
                json_text("tools"),
            ]
        };
        return Arc::new(Schema::new(fields));
    }
    let fields = if o.conversational {
        let msgs = list(msg_type(outside_key(o.reasoning)));
        if dpo {
            vec![f("prompt", msgs.clone()), f("chosen", msgs.clone()), f("rejected", msgs)]
        } else {
            vec![f("prompt", msgs.clone()), f("completion", msgs), f("label", DataType::Boolean)]
        }
    } else if dpo {
        let mut v = vec![f("prompt", s()), f("chosen", s()), f("rejected", s())];
        if outside {
            v.push(f("chosen_reasoning", s()));
            v.push(f("rejected_reasoning", s()));
        }
        v
    } else {
        let mut v = vec![f("prompt", s()), f("completion", s()), f("label", DataType::Boolean)];
        if outside {
            v.push(f("reasoning", s()));
        }
        v
    };
    Arc::new(Schema::new(fields))
}
