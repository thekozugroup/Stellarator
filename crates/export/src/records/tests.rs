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
