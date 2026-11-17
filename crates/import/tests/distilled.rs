//! v2 "prompts only": datasets shaped like public distilled sets. The prompt never comes from a
//! model-output column, assistant turns after the chosen user turn are dropped, and output columns
//! stay out of meta.

mod common;

use std::sync::Arc;

use arrow_array::builder::{ListBuilder, StringBuilder, StructBuilder};
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Fields};
use common::*;
use serde_json::{json, Value};
use stellarator_core::schemas::{ImportFormat, ImportMapping, ImportPreviewOut};
use stellarator_import::files::{open_path, Opened};
use stellarator_import::prompts::{preview_table, PreviewOptions};

fn preview(path: &std::path::Path, m: &ImportMapping) -> ImportPreviewOut {
    let Opened::Table(mut src) = open_path(path, ImportFormat::Auto, None).unwrap() else { panic!("not a table") };
    preview_table(&mut src, m, PreviewOptions::default()).unwrap()
}

/// Every message text of every preview prompt.
fn texts(pv: &ImportPreviewOut) -> Vec<String> {
    pv.prompts.iter().flatten().map(|m| m["content"].as_str().unwrap_or("").to_string()).collect()
}

/// The final assistant answer of every conversation in a JSONL fixture.
fn final_answers(path: &std::path::Path, column: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|row| {
            let msgs = row[column].as_array()?.clone();
            let last = msgs.last()?;
            let role = last.get("role").or_else(|| last.get("from"))?.as_str()?.to_string();
            (role == "assistant" || role == "gpt").then(|| last.get("content").or_else(|| last.get("value")).and_then(Value::as_str).unwrap_or("").to_string())
        })
        .collect()
}

fn roles(p: &[Value]) -> Vec<&str> {
    p.iter().map(|m| m["role"].as_str().unwrap()).collect()
}

#[test]
fn openai_messages_with_think_answers() {
    let path = fixture("distilled/openai_think.jsonl");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("messages"));
    assert!(pv.has_conversations);
    assert_eq!(pv.dropped, ["assistant turns in messages"]);
    // The final answers never appear; earlier assistant turns of a multi-turn chat are context.
    let finals = final_answers(&path, "messages");
    assert_eq!(finals.len(), 4);
    assert!(!texts(&pv).iter().any(|t| finals.contains(t)), "{:?}", texts(&pv));
    // The repeated question is deduplicated.
    assert_eq!(pv.estimated_prompts, Some(3));
    assert_eq!(roles(&pv.prompts[0]), ["system", "user"]);
    // up_to_last_user keeps earlier turns of the multi-turn conversation...
    assert_eq!(roles(&pv.prompts[2]), ["user", "assistant", "user"]);
    assert_eq!(pv.prompts[2][2]["content"], "Name a bigger one.");
    // ...first_user keeps the system message and the first question only.
    let pv = preview(&path, &mapping(json!({"turns": "first_user"})));
    assert_eq!(roles(&pv.prompts[2]), ["user"]);
    assert_eq!(pv.prompts[2][0]["content"], "Name a prime number.");
    assert!(!texts(&pv).iter().any(|t| t.contains("<think>")));
    // Scalar columns stay as meta.
    let r = prompts_of(&path, &mapping(json!({}))).unwrap();
    assert_eq!(r.prompts[0].meta, vec![("source".to_string(), json!("math")), ("num_tokens".to_string(), json!(812))]);
}

#[test]
fn sharegpt_conversations() {
    let path = fixture("distilled/sharegpt.jsonl");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("conversations"));
    assert!(pv.has_conversations);
    assert_eq!(pv.dropped, ["assistant turns in conversations"]);
    assert_eq!(pv.prompts[0], [json!({"role": "system", "content": "Answer briefly."}), json!({"role": "user", "content": "Capital of France?"})]);
    assert!(!texts(&pv).iter().any(|t| t.contains("Paris") || t.contains("chicken")));
    // A conversation with no user turn is skipped as empty.
    assert_eq!(pv.estimated_prompts, Some(2));
    assert!(pv.warnings.iter().any(|w| w == "1 row had an empty prompt and was skipped."), "{:?}", pv.warnings);
    let pv = preview(&path, &mapping(json!({"turns": "first_user"})));
    assert_eq!(pv.prompts[1], [json!({"role": "user", "content": "Hi!"})]);
}

#[test]
fn openr1_math_problem_not_generations() {
    let path = fixture("distilled/openr1_math.jsonl");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("problem"));
    assert!(!pv.has_conversations);
    // Generations and the conversation column are left out; the reference solution and answer
    // stay in meta.
    assert_eq!(pv.dropped, ["generations", "messages"]);
    assert_eq!(pv.prompts[0], [json!({"role": "user", "content": "Find $x$ if $2x = 8$."})]);
    let outputs: Vec<&str> = pv.columns.iter().filter(|c| c.looks_like_output).map(|c| c.name.as_str()).collect();
    assert_eq!(outputs, ["solution", "answer", "generations"]);
    let r = prompts_of(&path, &mapping(json!({}))).unwrap();
    let keys: Vec<&str> = r.prompts[0].meta.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["solution", "answer", "problem_type", "question_type", "source", "uuid", "correctness_count"]);
}

#[test]
fn prompt_response_reasoning_csv() {
    let path = fixture("distilled/prompt_response_reasoning.csv");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("prompt"));
    assert_eq!(pv.dropped, ["reasoning", "response"]);
    let r = prompts_of(&path, &mapping(json!({}))).unwrap();
    assert_eq!(r.users(), ["Explain photosynthesis in one sentence.", "Is 91 prime?"]);
    assert_eq!(r.prompts[1].meta, vec![("id".to_string(), json!("2")), ("model".to_string(), json!("r1"))]);
    // An explicit choice is honoured, even an output column.
    let r = prompts_of(&path, &mapping(json!({"prompt_field": "response"}))).unwrap();
    assert_eq!(r.users()[1], "No: 91 = 7 x 13.");
}

#[test]
fn dpo_prompt_chosen_rejected() {
    let path = fixture("distilled/dpo.jsonl");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("prompt"));
    assert!(!pv.has_conversations);
    assert_eq!(pv.dropped, ["chosen", "rejected"]);
    assert_eq!(pv.prompts[0], [json!({"role": "user", "content": "How do I boil an egg?"})]);
    // Scores about the responses are kept as meta; the responses are not.
    let r = prompts_of(&path, &mapping(json!({}))).unwrap();
    assert_eq!(
        r.prompts[0].meta,
        vec![("prompt_id".to_string(), json!("a1b2c3")), ("score_chosen".to_string(), json!(9.0)), ("score_rejected".to_string(), json!(2.0))]
    );
}

#[test]
fn trl_conversational_prompt_completion() {
    let path = fixture("distilled/trl_prompt_completion.jsonl");
    let pv = preview(&path, &mapping(json!({})));
    assert_eq!(pv.prompt_field.as_deref(), Some("prompt"));
    assert!(pv.has_conversations);
    // The prompt lists hold no assistant turns; the completion column is the output.
    assert_eq!(pv.dropped, ["completion"]);
    assert_eq!(pv.prompts[0], [json!({"role": "system", "content": "Be terse."}), json!({"role": "user", "content": "2+2?"})]);
}

#[test]
fn parquet_messages_column_with_response_columns() {
    // A Parquet distilled set: list<struct<role, content>> messages plus a huge response column.
    let fields = Fields::from(vec![Field::new("role", DataType::Utf8, true), Field::new("content", DataType::Utf8, true)]);
    let mut b = ListBuilder::new(StructBuilder::from_fields(fields, 8));
    for i in 0..4 {
        let s = b.values();
        for (role, content) in [("user", format!("Question {i}?")), ("assistant", format!("<think>long</think>Answer {i}"))] {
            s.field_builder::<StringBuilder>(0).unwrap().append_value(role);
            s.field_builder::<StringBuilder>(1).unwrap().append_value(content);
            s.append(true);
        }
        b.append(true);
    }
    let batch = RecordBatch::try_from_iter(vec![
        ("conversation_id", Arc::new(StringArray::from(vec!["c0", "c1", "c2", "c3"])) as ArrayRef),
        ("messages", Arc::new(b.finish()) as ArrayRef),
        ("r1_generation", Arc::new(StringArray::from(vec!["x".repeat(5000); 4])) as ArrayRef),
    ])
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("distilled.parquet");
    std::fs::write(&path, parquet_bytes(&[batch], 2)).unwrap();
    let pv = preview(&path, &mapping(json!({"turns": "first_user"})));
    assert_eq!(pv.prompt_field.as_deref(), Some("messages"));
    assert_eq!(pv.dropped, ["assistant turns in messages", "r1_generation"]);
    assert_eq!(pv.prompts.len(), 4);
    assert_eq!(pv.prompts[3], [json!({"role": "user", "content": "Question 3?"})]);
}

#[tokio::test]
async fn commit_stores_no_assistant_text() {
    let app = app().await;
    for (file, turns) in [("openai_think.jsonl", "first_user"), ("openai_think.jsonl", "up_to_last_user"), ("sharegpt.jsonl", "first_user"), ("openr1_math.jsonl", "up_to_last_user")] {
        let bytes = std::fs::read(fixture(&format!("distilled/{file}"))).unwrap();
        let up = app.upload_ok(file, &bytes).await;
        let set = app
            .import(json!({"source": {"kind": "file", "upload_id": up["upload_id"]}, "mapping": {"turns": turns}, "name": file}))
            .await;
        assert_eq!(serde_json::to_value(set.status).unwrap(), "ready", "{:?}", set.status_detail);
        let stored = app.stored(set.id).await;
        assert_eq!(stored.len() as i64, set.prompt_count);
        let column = if file.starts_with("sharegpt") { "conversations" } else { "messages" };
        let finals = final_answers(&fixture(&format!("distilled/{file}")), column);
        for (messages, meta) in &stored {
            let all = format!("{messages}{meta}");
            // No final answer and no generations anywhere in the stored prompts.
            assert!(!all.contains("boxed"), "{file} {turns}: {all}");
            let msgs = messages.as_array().unwrap();
            assert!(msgs.iter().all(|m| !finals.iter().any(|f| m["content"] == f.as_str())), "{file} {turns}: {all}");
            assert_eq!(msgs.last().unwrap()["role"], "user");
            if turns == "first_user" {
                assert!(msgs.iter().all(|m| m["role"] != "assistant"));
                assert!(!all.contains("<think>"));
            }
        }
        if set.source["prompt_field"] == "messages" || set.source["prompt_field"] == "conversations" {
            assert_eq!(set.source["turns"], turns);
        }
    }
}
