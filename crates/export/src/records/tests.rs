use super::*;
use chrono::NaiveDate;
use serde_json::json;
use stellarator_core::schemas::ReasoningPlacement as P;

fn prompt() -> Value {
    json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "What is 2+2? é"}])
}

fn run(params: Value) -> RunInfo {
    RunInfo {
        id: 7,
        name: "Qwen3 32B on gsm8k".into(),
        endpoint_name: "spark".into(),
        model: "qwen3-32b".into(),
        prompt_set_id: 1,
        params,
        tools: None,
    }
}

fn sample(reasoning: Option<&str>, response: &str) -> SampleRec {
    SampleRec {
        id: 11,
        run_id: 7,
        prompt_id: 3,
        prompt_idx: 2,
        sample_idx: 0,
        reasoning: reasoning.map(String::from),
        response: Some(response.into()),
        finish_reason: Some("stop".into()),
        prompt_tokens: Some(10),
        completion_tokens: Some(20),
        reasoning_tokens: Some(5),
        latency_ms: Some(100),
        ttft_ms: Some(10),
        completed_at: Some(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(3, 4, 5).unwrap()),
        trace: None,
        tool_calls: 0,
    }
}

fn opts(format: SftFormat, reasoning: P) -> ExportOptions {
    serde_json::from_value(json!({"format": sft_name(format), "reasoning": placement_name(reasoning)})).unwrap()
}

fn sft(format: SftFormat, p: P, s: &SampleRec) -> Value {
    sft_record(&opts(format, p), &run(json!({})), s, &prompt(), Some(&json!({"answer": "4"})))
}

const THINK: &str = "<think>\n2 plus 2 is 4.\n</think>\n\n4";
const ALL: [P; 4] = [P::ThinkTags, P::ReasoningContent, P::Thinking, P::Omit];

#[test]
fn messages_placements() {
    for p in ALL {
        let rec = sft(SftFormat::Messages, p, &sample(Some("2 plus 2 is 4."), "4"));
        let msgs = rec["messages"].as_array().unwrap();
        assert_eq!(&msgs[..2], &prompt().as_array().unwrap()[..]);
        let a = msgs.last().unwrap();
        match p {
            P::ThinkTags => assert_eq!(a, &json!({"role": "assistant", "content": THINK})),
            P::Omit => assert_eq!(a, &json!({"role": "assistant", "content": "4"})),
            _ => {
                let key = outside_key(p).unwrap();
                assert_eq!(a, &json!({"role": "assistant", "content": "4", key: "2 plus 2 is 4."}));
            }
        }
        let a2 = sft(SftFormat::Messages, p, &sample(None, "4"))["messages"][2].clone();
        assert_eq!(a2["content"], "4");
        assert!(a2.get(outside_key(p).unwrap_or("x")).is_none_or(Value::is_null));
    }
}

#[test]
fn sharegpt_alpaca_prompt_completion() {
    for p in ALL {
        let outside = outside_key(p).is_some();
        let text = if p == P::ThinkTags { THINK } else { "4" };
        let s = sample(Some("2 plus 2 is 4."), "4");
        let sg = sft(SftFormat::Sharegpt, p, &s);
        assert_eq!(
            sg["conversations"],
            json!([
                {"from": "system", "value": "Be brief."},
                {"from": "human", "value": "What is 2+2? é"},
                {"from": "gpt", "value": text},
            ])
        );
        assert_eq!(sg.get("reasoning").is_some(), outside);
        if outside {
            assert_eq!(sg["reasoning"], "2 plus 2 is 4.");
        }
        let al = sft(SftFormat::Alpaca, p, &s);
        assert_eq!(al["instruction"], "What is 2+2? é");
        assert_eq!(al["input"], "");
        assert_eq!(al["output"], text);
        assert_eq!(al["system"], "Be brief.");
        assert!(al.get("history").is_none());
        assert_eq!(al.get("reasoning").is_some(), outside);
        assert_eq!(sft(SftFormat::Alpaca, p, &sample(None, "4"))["output"], "4");
        let pc = sft(SftFormat::PromptCompletion, p, &s);
        assert_eq!(pc["prompt"], prompt());
        assert_eq!(pc["completion"][0]["role"], "assistant");
        assert_eq!(pc["completion"][0]["content"], text);
    }
}

#[test]
fn alpaca_history_and_no_system() {
    let msgs = json!([
        {"role": "user", "content": "a"},
        {"role": "assistant", "content": "b"},
        {"role": "user", "content": "c"},
    ]);
    let rec = sft_record(&opts(SftFormat::Alpaca, P::ThinkTags), &run(json!({})), &sample(None, "x"), &msgs, None);
    assert_eq!(rec["history"], json!([["a", "b"]]));
    assert_eq!(rec["instruction"], "c");
    assert!(rec.get("system").is_none());
}

#[test]
fn raw_record_and_key_order() {
    let r = sft_record(
        &opts(SftFormat::Raw, P::ThinkTags),
        &run(json!({"temperature": 0.7})),
        &sample(Some("2 plus 2 is 4."), "4"),
        &prompt(),
        Some(&json!({"answer": "4"})),
    );
    assert_eq!(r["id"], 11);
    assert_eq!(r["prompt_index"], 2);
    assert_eq!(r["endpoint"], "spark");
    assert_eq!(r["usage"], json!({"prompt_tokens": 10, "completion_tokens": 20, "reasoning_tokens": 5}));
    assert_eq!(r["params"], json!({"temperature": 0.7}));
    assert_eq!(r["meta"], json!({"answer": "4"}));
    assert_eq!(r["created_at"], "2026-01-02T03:04:05+00:00");
    let keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "id", "run_id", "prompt_id", "prompt_index", "sample_index", "model", "endpoint", "messages", "reasoning",
            "response", "finish_reason", "usage", "latency_ms", "ttft_ms", "params", "meta", "created_at"
        ]
    );
}

#[test]
fn iso_keeps_microseconds_like_python() {
    let t = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_micro_opt(3, 4, 5, 120).unwrap();
    assert_eq!(iso(Some(t)), "2026-01-02T03:04:05.000120+00:00");
    assert_eq!(iso(None), Value::Null);
}

#[test]
fn params_applied_and_include_system() {
    let r = run(json!({"system_prompt": "Answer in JSON.", "user_template": "Solve:\n{prompt}\nThanks"}));
    let o = opts(SftFormat::Messages, P::Omit);
    let rec = sft_record(&o, &r, &sample(None, "4"), &prompt(), None);
    assert_eq!(rec["messages"][0], json!({"role": "system", "content": "Answer in JSON."}));
    assert_eq!(rec["messages"][1]["content"], "Solve:\nWhat is 2+2? é\nThanks");
    let mut no_sys = o.clone();
    no_sys.include_system = false;
    let rec = sft_record(&no_sys, &r, &sample(None, "4"), &prompt(), None);
    let roles: Vec<&str> = rec["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant"]);
    // a system prompt is added when the prompt has none
    let rec = sft_record(&opts(SftFormat::Messages, P::ThinkTags), &r, &sample(None, "4"), &json!([prompt()[1].clone()]), None);
    assert_eq!(rec["messages"][0]["role"], "system");
    let mut al = opts(SftFormat::Alpaca, P::Omit);
    al.include_system = false;
    assert!(sft_record(&al, &run(json!({})), &sample(None, "4"), &prompt(), None).get("system").is_none());
}

#[test]
fn think_tags_blank_reasoning_and_newline_trim() {
    let rec = sft(SftFormat::Messages, P::ThinkTags, &sample(Some("  \n"), "4"));
    assert_eq!(rec["messages"][2]["content"], "4");
    let rec = sft(SftFormat::Messages, P::ThinkTags, &sample(Some("\n\nwhy\n"), "4"));
    assert_eq!(rec["messages"][2]["content"], "<think>\nwhy\n</think>\n\n4");
}

fn pair() -> (SampleRec, SampleRec) {
    let mut c = sample(Some("2 plus 2 is 4."), "4");
    c.id = 1;
    let mut r = sample(None, "5");
    r.id = 2;
    (c, r)
}

fn popts(v: Value) -> PrefExportOptions {
    serde_json::from_value(v).unwrap()
}

#[test]
fn dpo_and_kto_every_placement() {
    let (c, r) = pair();
    for p in ALL {
        let pn = placement_name(p);
        let outside = outside_key(p).is_some();
        let want = if p == P::ThinkTags { THINK } else { "4" };
        let conv = pref_records(&popts(json!({"reasoning": pn})), &c, &r, &prompt(), &json!({}));
        assert_eq!(conv.len(), 1);
        let keys: Vec<&String> = conv[0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["prompt", "chosen", "rejected"]);
        assert_eq!(conv[0]["prompt"], prompt());
        assert_eq!(conv[0]["rejected"][0]["content"], "5");
        assert_eq!(conv[0]["chosen"][0]["content"], want);

        let text = &pref_records(&popts(json!({"reasoning": pn, "conversational": false})), &c, &r, &prompt(), &json!({}))[0];
        assert_eq!(text["prompt"], "Be brief.\n\nWhat is 2+2? é");
        assert_eq!(text["chosen"], want);
        assert_eq!(text["rejected"], "5");
        assert_eq!(text.get("chosen_reasoning").is_some(), outside);
        if outside {
            assert_eq!(text["chosen_reasoning"], "2 plus 2 is 4.");
            assert_eq!(text["rejected_reasoning"], Value::Null);
        }

        let kto = pref_records(&popts(json!({"format": "kto", "reasoning": pn})), &c, &r, &prompt(), &json!({}));
        assert_eq!(kto[0]["label"], true);
        assert_eq!(kto[1]["label"], false);
        let keys: Vec<&String> = kto[0].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["prompt", "completion", "label"]);
        let kt = pref_records(
            &popts(json!({"format": "kto", "reasoning": pn, "conversational": false})),
            &c,
            &r,
            &prompt(),
            &json!({}),
        );
        assert!(kt[0]["completion"].is_string());
        assert_eq!(kt[1]["label"], false);
        assert_eq!(kt[0].get("reasoning").is_some(), outside);
    }
}

#[test]
fn prompt_text_multi_turn() {
    let m = |r: &str, c: &str| Msg::new(r, c);
    assert_eq!(
        render_prompt_text(&[m("user", "a"), m("assistant", "b"), m("user", "c")]),
        "User: a\n\nAssistant: b\n\nUser: c"
    );
    assert_eq!(render_prompt_text(&[m("user", "x")]), "x");
    assert_eq!(render_prompt_text(&[m("system", "s"), m("user", "x")]), "s\n\nx");
}

#[test]
fn slugs_and_filenames() {
    assert_eq!(filename_for("Qwen3-32B on GSM8K!", "messages", "jsonl"), "qwen3-32b-on-gsm8k-messages.jsonl");
    assert_eq!(slugify("日本語"), "export");
    assert_eq!(slugify("Café déjà vu"), "cafe-deja-vu");
    assert_eq!(slugify("..a__b.."), "a__b");
    assert_eq!(slugify(&"x".repeat(100)).len(), 80);
}

// ------------------------------------------------------------------------------ runs with tools

use stellarator_core::schemas::{
    McpTransport, ToolCall, ToolMode, ToolSnapshot, ToolSnapshotServer, ToolSnapshotTool, TraceAssistant, TraceTool,
};

fn tool_defs() -> Vec<Value> {
    vec![
        json!({"type": "function", "function": {"name": "lookup", "description": "Look up a term.",
               "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}}),
        json!({"type": "function", "function": {"name": "add", "parameters": {"type": "object",
               "properties": {"a": {"type": "number"}, "b": {"type": "number"}}}}}),
    ]
}

fn tool_run() -> RunInfo {
    RunInfo {
        tools: Some(ToolSnapshot {
            servers: vec![ToolSnapshotServer {
                server_id: 3,
                name: "docs".into(),
                transport: McpTransport::Http,
                tools: vec![
                    ToolSnapshotTool { name: "lookup".into(), server_tool: "lookup".into() },
                    ToolSnapshotTool { name: "add".into(), server_tool: "add".into() },
                ],
            }],
            definitions: tool_defs(),
            mode: ToolMode::Direct,
            code: None,
        }),
        ..run(json!({}))
    }
}

fn turn(reasoning: Option<&str>, content: &str, calls: &[(&str, &str, &str)]) -> TraceMessage {
    TraceMessage::Assistant(TraceAssistant {
        reasoning: reasoning.map(String::from),
        content: content.into(),
        tool_calls: calls
            .iter()
            .map(|(id, name, args)| ToolCall { id: (*id).into(), name: (*name).into(), arguments: (*args).into() })
            .collect(),
        finish_reason: Some(if calls.is_empty() { "stop" } else { "tool_calls" }.into()),
        prompt_tokens: Some(10),
        completion_tokens: Some(5),
        reasoning_tokens: Some(2),
        latency_ms: 50,
    })
}

fn result(id: &str, name: &str, content: &str, is_error: bool) -> TraceMessage {
    TraceMessage::Tool(TraceTool {
        tool_call_id: id.into(),
        name: name.into(),
        server: "docs".into(),
        content: content.into(),
        is_error,
        truncated: false,
        latency_ms: 7,
        code: None,
    })
}

/// Two parallel calls, then an answer.
fn tool_sample() -> SampleRec {
    SampleRec {
        reasoning: Some("Combine them.".into()),
        response: Some("High tide is at 6; 2+2 is 4.".into()),
        trace: Some(vec![
            turn(
                Some("I need the facts."),
                "",
                &[("call_1", "lookup", r#"{"query": "tides"}"#), ("call_2", "add", r#"{"a": 2, "b": 2}"#)],
            ),
            result("call_1", "lookup", "High tide at 6.", false),
            result("call_2", "add", "4", false),
            turn(Some("Combine them."), "High tide is at 6; 2+2 is 4.", &[]),
        ]),
        tool_calls: 2,
        ..sample(None, "")
    }
}

fn tool_opts(format: SftFormat, p: P, file_type: &str) -> ExportOptions {
    serde_json::from_value(json!({"format": sft_name(format), "reasoning": placement_name(p), "file_type": file_type}))
        .unwrap()
}

fn tool_sft(format: SftFormat, p: P, file_type: &str) -> String {
    crate::pyjson::to_string(&sft_record(&tool_opts(format, p, file_type), &tool_run(), &tool_sample(), &prompt(), None))
}

const TOOLS_JSON: &str = r#"[{"type": "function", "function": {"name": "lookup", "description": "Look up a term.", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}}, {"type": "function", "function": {"name": "add", "parameters": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}}}}]"#;
const PROMPT_MSGS: &str = r#"[{"role": "system", "content": "Be brief."}, {"role": "user", "content": "What is 2+2? é"}]"#;
const CALLS_NATIVE: &str = r#"[{"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": {"query": "tides"}}}, {"id": "call_2", "type": "function", "function": {"name": "add", "arguments": {"a": 2, "b": 2}}}]"#;
const RESULTS: &str = r#"{"role": "tool", "tool_call_id": "call_1", "name": "lookup", "content": "High tide at 6."}, {"role": "tool", "tool_call_id": "call_2", "name": "add", "content": "4"}"#;

/// `{"messages": [...]}` with the prompt's two messages spliced in front of `rest`.
fn golden_messages(rest: &str, tools: &str) -> String {
    format!(r#"{{"messages": [{}, {rest}], "tools": {tools}}}"#, &PROMPT_MSGS[1..PROMPT_MSGS.len() - 1])
}

#[test]
fn messages_with_tools_golden() {
    // Think tags: every assistant turn carries its own thinking.
    let want = golden_messages(
        &format!(
            r#"{{"role": "assistant", "content": "<think>\nI need the facts.\n</think>\n\n", "tool_calls": {CALLS_NATIVE}}}, {RESULTS}, {{"role": "assistant", "content": "<think>\nCombine them.\n</think>\n\nHigh tide is at 6; 2+2 is 4."}}"#
        ),
        TOOLS_JSON,
    );
    assert_eq!(tool_sft(SftFormat::Messages, P::ThinkTags, "jsonl"), want);
    // reasoning_content (Qwen3, DeepSeek templates) on every assistant turn.
    let want = golden_messages(
        &format!(
            r#"{{"role": "assistant", "content": "", "reasoning_content": "I need the facts.", "tool_calls": {CALLS_NATIVE}}}, {RESULTS}, {{"role": "assistant", "content": "High tide is at 6; 2+2 is 4.", "reasoning_content": "Combine them."}}"#
        ),
        TOOLS_JSON,
    );
    assert_eq!(tool_sft(SftFormat::Messages, P::ReasoningContent, "jsonl"), want);
    // thinking (gpt-oss template) and omit.
    let thinking = sft_record(&tool_opts(SftFormat::Messages, P::Thinking, "json"), &tool_run(), &tool_sample(), &prompt(), None);
    assert_eq!(thinking["messages"][2]["thinking"], "I need the facts.");
    assert_eq!(thinking["messages"][5]["thinking"], "Combine them.");
    let omit = sft_record(&tool_opts(SftFormat::Messages, P::Omit, "jsonl"), &tool_run(), &tool_sample(), &prompt(), None);
    assert_eq!(omit["messages"][2], json!({"role": "assistant", "content": "", "tool_calls": serde_json::from_str::<Value>(CALLS_NATIVE).unwrap()}));
    // Parquet: arguments and the tool list as JSON text (one type per column).
    let want = golden_messages(
        &format!(
            r#"{{"role": "assistant", "content": "", "tool_calls": [{{"id": "call_1", "type": "function", "function": {{"name": "lookup", "arguments": "{{\"query\": \"tides\"}}"}}}}, {{"id": "call_2", "type": "function", "function": {{"name": "add", "arguments": "{{\"a\": 2, \"b\": 2}}"}}}}]}}, {RESULTS}, {{"role": "assistant", "content": "High tide is at 6; 2+2 is 4."}}"#
        ),
        &crate::pyjson::to_string(&Value::String(TOOLS_JSON.into())),
    );
    assert_eq!(tool_sft(SftFormat::Messages, P::Omit, "parquet"), want);
}

#[test]
fn sharegpt_with_tools_golden() {
    let want = r#"{"conversations": [{"from": "system", "value": "Be brief."}, {"from": "human", "value": "What is 2+2? é"}, {"from": "function_call", "value": "<think>\nI need the facts.\n</think>\n\n[{\"name\": \"lookup\", \"arguments\": {\"query\": \"tides\"}}, {\"name\": \"add\", \"arguments\": {\"a\": 2, \"b\": 2}}]"}, {"from": "observation", "value": "High tide at 6.\n</tool_response>\n<tool_response>\n4"}, {"from": "gpt", "value": "<think>\nCombine them.\n</think>\n\nHigh tide is at 6; 2+2 is 4."}], "tools": "[{\"name\": \"lookup\", \"description\": \"Look up a term.\", \"parameters\": {\"type\": \"object\", \"properties\": {\"query\": {\"type\": \"string\"}}, \"required\": [\"query\"]}}, {\"name\": \"add\", \"parameters\": {\"type\": \"object\", \"properties\": {\"a\": {\"type\": \"number\"}, \"b\": {\"type\": \"number\"}}}}]"}"#;
    assert_eq!(tool_sft(SftFormat::Sharegpt, P::ThinkTags, "jsonl"), want);
    // One call: a single object, not a list. Outside placements keep the final reasoning aside.
    let mut one = tool_sample();
    one.trace = Some(vec![
        turn(Some("Look it up."), "Let me check.", &[("c9", "lookup", r#"{"query": "x"}"#)]),
        result("c9", "lookup", "X is 1.", false),
        turn(None, "X is 1.", &[]),
    ]);
    one.reasoning = None;
    let rec = sft_record(&tool_opts(SftFormat::Sharegpt, P::ReasoningContent, "jsonl"), &tool_run(), &one, &prompt(), None);
    assert_eq!(
        rec["conversations"].as_array().unwrap()[2..],
        [
            json!({"from": "function_call", "value": r#"{"name": "lookup", "arguments": {"query": "x"}}"#}),
            json!({"from": "observation", "value": "X is 1."}),
            json!({"from": "gpt", "value": "X is 1."}),
        ]
    );
    assert_eq!(rec["reasoning"], Value::Null);
    let keys: Vec<&String> = rec.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["conversations", "tools", "reasoning"]);
}

#[test]
fn raw_with_tools_keeps_the_trace() {
    let rec = sft_record(&tool_opts(SftFormat::Raw, P::ThinkTags, "jsonl"), &tool_run(), &tool_sample(), &prompt(), None);
    let keys: Vec<&str> = rec.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "id", "run_id", "prompt_id", "prompt_index", "sample_index", "model", "endpoint", "messages", "reasoning",
            "response", "finish_reason", "usage", "latency_ms", "ttft_ms", "params", "meta", "created_at", "tool_calls",
            "trace", "tools"
        ]
    );
    assert_eq!(rec["tool_calls"], 2);
    assert_eq!(rec["tools"], serde_json::from_str::<Value>(TOOLS_JSON).unwrap());
    assert_eq!(rec["trace"], serde_json::to_value(tool_sample().trace.unwrap()).unwrap());
    assert_eq!(
        crate::pyjson::to_string(&rec["trace"][1]),
        r#"{"role": "tool", "tool_call_id": "call_1", "name": "lookup", "server": "docs", "content": "High tide at 6.", "is_error": false, "truncated": false, "latency_ms": 7}"#
    );
}

#[test]
fn odd_arguments_and_samples_without_a_trace() {
    let mut s = tool_sample();
    s.trace = Some(vec![
        turn(None, "", &[("a", "lookup", r#"{"query": "unterminated"#), ("b", "add", "")]),
        result("a", "lookup", "The arguments are not valid JSON.", true),
        result("b", "add", "0", false),
        turn(None, "Done.", &[]),
    ]);
    let rec = sft_record(&tool_opts(SftFormat::Messages, P::Omit, "jsonl"), &tool_run(), &s, &prompt(), None);
    // Arguments that don't parse stay as the text the model wrote; empty ones are {}.
    assert_eq!(rec["messages"][2]["tool_calls"][0]["function"]["arguments"], r#"{"query": "unterminated"#);
    assert_eq!(rec["messages"][2]["tool_calls"][1]["function"]["arguments"], json!({}));
    // In Parquet every arguments cell is valid JSON text: bad arguments become a JSON string.
    let rec = sft_record(&tool_opts(SftFormat::Messages, P::Omit, "parquet"), &tool_run(), &s, &prompt(), None);
    assert_eq!(rec["messages"][2]["tool_calls"][0]["function"]["arguments"], r#""{\"query\": \"unterminated""#);
    assert_eq!(rec["messages"][2]["tool_calls"][1]["function"]["arguments"], "{}");
    // A sample with no trace exports its final answer with the run's tools.
    let mut plain = tool_sample();
    plain.trace = None;
    let rec = sft_record(&tool_opts(SftFormat::Messages, P::Omit, "jsonl"), &tool_run(), &plain, &prompt(), None);
    assert_eq!(rec["messages"].as_array().unwrap().len(), 3);
    assert_eq!(rec["messages"][2], json!({"role": "assistant", "content": "High tide is at 6; 2+2 is 4."}));
    assert_eq!(rec["tools"].as_array().unwrap().len(), 2);
    // A run without tools exports as before: no tools field.
    let rec = sft_record(&tool_opts(SftFormat::Messages, P::Omit, "jsonl"), &run(json!({})), &plain, &prompt(), None);
    assert!(rec.get("tools").is_none());
}

#[test]
fn formats_that_cannot_hold_tools_say_why() {
    assert!(tools_refusal(SftFormat::Alpaca).unwrap().starts_with("Alpaca records have no place for tool calls"));
    assert!(tools_refusal(SftFormat::PromptCompletion).unwrap().contains("would put the tool results in the completion"));
    for f in [SftFormat::Messages, SftFormat::Sharegpt, SftFormat::Raw] {
        assert_eq!(tools_refusal(f), None);
    }
}

#[test]
fn preference_records_with_tools_golden() {
    let mut r = tool_sample();
    r.id = 2;
    r.trace = Some(vec![turn(None, "No tools needed: 4.", &[])]);
    r.tool_calls = 0;
    let defs = tool_defs();
    let pair = Some(PairTools { chosen: &defs, rejected: &defs[..1] });
    let chosen = format!(
        r#"[{{"role": "assistant", "content": "<think>\nI need the facts.\n</think>\n\n", "tool_calls": {CALLS_NATIVE}}}, {RESULTS}, {{"role": "assistant", "content": "<think>\nCombine them.\n</think>\n\nHigh tide is at 6; 2+2 is 4."}}]"#
    );
    let o: PrefExportOptions = serde_json::from_value(json!({"format": "dpo", "reasoning": "think_tags"})).unwrap();
    let dpo = pref_records_with_tools(&o, &tool_sample(), &r, &prompt(), &json!({}), pair);
    assert_eq!(
        crate::pyjson::to_string(&dpo[0]),
        format!(
            r#"{{"prompt": {PROMPT_MSGS}, "chosen": {chosen}, "rejected": [{{"role": "assistant", "content": "No tools needed: 4."}}], "tools": {TOOLS_JSON}}}"#
        )
    );
    let o: PrefExportOptions = serde_json::from_value(json!({"format": "kto", "reasoning": "think_tags"})).unwrap();
    let kto = pref_records_with_tools(&o, &tool_sample(), &r, &prompt(), &json!({}), pair);
    assert_eq!(
        crate::pyjson::to_string(&kto[0]),
        format!(r#"{{"prompt": {PROMPT_MSGS}, "completion": {chosen}, "label": true, "tools": {TOOLS_JSON}}}"#)
    );
    // Each KTO record carries its own run's tools.
    assert_eq!(kto[1]["tools"].as_array().unwrap().len(), 1);
    assert_eq!(kto[1]["label"], false);
    // DPO tools: both runs' definitions, each name once.
    let other = vec![json!({"type": "function", "function": {"name": "search", "parameters": {}}}), defs[0].clone()];
    let merged = merge_definitions(&defs, &other);
    let names: Vec<&str> = merged.iter().map(|d| d["function"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["lookup", "add", "search"]);
    // Parquet: tools as JSON text.
    let o: PrefExportOptions = serde_json::from_value(json!({"format": "dpo", "file_type": "parquet"})).unwrap();
    assert!(pref_records_with_tools(&o, &tool_sample(), &r, &prompt(), &json!({}), pair)[0]["tools"].is_string());
}

// ------------------------------------------------------------------------------ code mode

use stellarator_core::schemas::{CodeExecution, CodeMethod, CodeNamespace, CodeStatus, CodeToolSnapshot, InnerToolCall};

const CODE_TOOL: &str = r#"{"type": "function", "function": {"name": "codemode", "description": "Execute code to achieve a goal.\n\nAvailable:\ntype AddInput = {\n    a: number;\n    b: number;\n}\ntype AddOutput = unknown\n\ndeclare const docs: {\n\t/**\n\t * Add two numbers.\n\t */\n\tadd: (input: AddInput) => Promise<AddOutput>;\n}", "parameters": {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"code": {"type": "string", "description": "JavaScript async arrow function to execute"}}, "required": ["code"], "additionalProperties": false}}}"#;
const CODE: &str = "async () => {\n  const s = await docs.add({ a: 2, b: 2 });\n  console.log(\"sum\", s);\n  return s;\n}";

fn code_run() -> RunInfo {
    RunInfo {
        tools: Some(ToolSnapshot {
            servers: vec![ToolSnapshotServer {
                server_id: 3,
                name: "docs".into(),
                transport: McpTransport::Http,
                tools: vec![ToolSnapshotTool { name: "docs.add".into(), server_tool: "add".into() }],
            }],
            definitions: vec![serde_json::from_str(CODE_TOOL).unwrap()],
            mode: ToolMode::Code,
            code: Some(CodeToolSnapshot {
                tool_name: "codemode".into(),
                types: "...".into(),
                namespaces: vec![CodeNamespace {
                    name: "docs".into(),
                    methods: vec![CodeMethod { name: "add".into(), server_id: 3, server: "docs".into(), tool: "add".into() }],
                }],
                format: "@cloudflare/codemode 0.5.6".into(),
                ..CodeToolSnapshot::default()
            }),
        }),
        ..run(json!({}))
    }
}

/// The model writes code, the code calls docs.add, and the model answers from the result.
fn code_sample() -> SampleRec {
    let execution = CodeExecution {
        code: CODE.into(),
        status: CodeStatus::Ok,
        calls: vec![InnerToolCall {
            method: "docs.add".into(),
            server: "docs".into(),
            tool: "add".into(),
            arguments: json!({"a": 2, "b": 2}),
            result: "4".into(),
            is_error: false,
            truncated: false,
            latency_ms: 3,
        }],
        logs: vec!["sum 4".into()],
        result: Some("4".into()),
        error: None,
        latency_ms: 12,
    };
    let arguments = serde_json::to_string(&json!({"code": CODE})).unwrap();
    SampleRec {
        reasoning: Some("It is 4.".into()),
        response: Some("2+2 is 4.".into()),
        trace: Some(vec![
            turn(Some("I'll add them in code."), "", &[("call_1", "codemode", &arguments)]),
            TraceMessage::Tool(TraceTool {
                tool_call_id: "call_1".into(),
                name: "codemode".into(),
                server: String::new(),
                content: r#"{"result":4,"logs":["sum 4"]}"#.into(),
                is_error: false,
                truncated: false,
                latency_ms: 15,
                code: Some(execution),
            }),
            turn(Some("It is 4."), "2+2 is 4.", &[]),
        ]),
        tool_calls: 1,
        ..sample(None, "")
    }
}

fn code_sft(format: SftFormat, p: P) -> String {
    crate::pyjson::to_string(&sft_record(&tool_opts(format, p, "jsonl"), &code_run(), &code_sample(), &prompt(), None))
}

#[test]
fn messages_in_code_mode_golden() {
    // The code tool's definition, its call with the code, and the result the model saw; nothing of
    // what ran inside the execution.
    let want = golden_messages(
        r#"{"role": "assistant", "content": "<think>\nI'll add them in code.\n</think>\n\n", "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "codemode", "arguments": {"code": "async () => {\n  const s = await docs.add({ a: 2, b: 2 });\n  console.log(\"sum\", s);\n  return s;\n}"}}}]}, {"role": "tool", "tool_call_id": "call_1", "name": "codemode", "content": "{\"result\":4,\"logs\":[\"sum 4\"]}"}, {"role": "assistant", "content": "<think>\nIt is 4.\n</think>\n\n2+2 is 4."}"#,
        &format!("[{CODE_TOOL}]"),
    );
    assert_eq!(code_sft(SftFormat::Messages, P::ThinkTags), want);
}

#[test]
fn sharegpt_in_code_mode_golden() {
    let want = r#"{"conversations": [{"from": "system", "value": "Be brief."}, {"from": "human", "value": "What is 2+2? é"}, {"from": "function_call", "value": "<think>\nI'll add them in code.\n</think>\n\n{\"name\": \"codemode\", \"arguments\": {\"code\": \"async () => {\\n  const s = await docs.add({ a: 2, b: 2 });\\n  console.log(\\\"sum\\\", s);\\n  return s;\\n}\"}}"}, {"from": "observation", "value": "{\"result\":4,\"logs\":[\"sum 4\"]}"}, {"from": "gpt", "value": "<think>\nIt is 4.\n</think>\n\n2+2 is 4."}], "tools": "[{\"name\": \"codemode\", \"description\": \"Execute code to achieve a goal.\\n\\nAvailable:\\ntype AddInput = {\\n    a: number;\\n    b: number;\\n}\\ntype AddOutput = unknown\\n\\ndeclare const docs: {\\n\\t/**\\n\\t * Add two numbers.\\n\\t */\\n\\tadd: (input: AddInput) => Promise<AddOutput>;\\n}\", \"parameters\": {\"$schema\": \"http://json-schema.org/draft-07/schema#\", \"type\": \"object\", \"properties\": {\"code\": {\"type\": \"string\", \"description\": \"JavaScript async arrow function to execute\"}}, \"required\": [\"code\"], \"additionalProperties\": false}}]"}"#;
    assert_eq!(code_sft(SftFormat::Sharegpt, P::ThinkTags), want);
}

#[test]
fn raw_in_code_mode_keeps_the_inner_calls() {
    let rec = sft_record(&tool_opts(SftFormat::Raw, P::ThinkTags, "jsonl"), &code_run(), &code_sample(), &prompt(), None);
    assert_eq!(rec["tools"], json!([serde_json::from_str::<Value>(CODE_TOOL).unwrap()]));
    assert_eq!(
        crate::pyjson::to_string(&rec["trace"][1]),
        r#"{"role": "tool", "tool_call_id": "call_1", "name": "codemode", "server": "", "content": "{\"result\":4,\"logs\":[\"sum 4\"]}", "is_error": false, "truncated": false, "latency_ms": 15, "code": {"code": "async () => {\n  const s = await docs.add({ a: 2, b: 2 });\n  console.log(\"sum\", s);\n  return s;\n}", "status": "ok", "calls": [{"method": "docs.add", "server": "docs", "tool": "add", "arguments": {"a": 2, "b": 2}, "result": "4", "is_error": false, "truncated": false, "latency_ms": 3}], "logs": ["sum 4"], "result": "4", "error": null, "latency_ms": 12}}"#
    );
    // Conversational preference records hold only what the model saw.
    let o: PrefExportOptions = serde_json::from_value(json!({"format": "kto", "reasoning": "omit"})).unwrap();
    let defs = code_run().tools.unwrap().definitions;
    let pair = Some(PairTools { chosen: &defs, rejected: &defs });
    let kto = pref_records_with_tools(&o, &code_sample(), &code_sample(), &prompt(), &json!({}), pair);
    assert!(kto[0]["completion"][1].get("code").is_none());
    assert_eq!(kto[0]["completion"][1]["content"], r#"{"result":4,"logs":["sum 4"]}"#);
    // The dataset card names the mode.
    assert_eq!(
        crate::hub::tools_line(&code_run()).unwrap(),
        "Tools: Code Mode (@cloudflare/codemode 0.5.6): one tool, `codemode`, whose JavaScript calls docs.add (MCP servers: docs)"
    );
}
