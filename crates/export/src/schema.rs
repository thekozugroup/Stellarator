//! Arrow schemas for the Parquet and CSV columns of each export format.

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

pub fn sft_schema(o: &ExportOptions) -> Arc<Schema> {
    let extra = outside_key(o.reasoning);
    let outside = extra.is_some();
    let fields = match o.format {
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
    Arc::new(Schema::new(fields))
}

pub fn pref_schema(o: &PrefExportOptions) -> Arc<Schema> {
    let outside = outside_key(o.reasoning).is_some();
    let dpo = o.format == PrefFormat::Dpo;
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
