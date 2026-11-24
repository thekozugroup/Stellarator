//! Code mode's helpers against the outputs of Cloudflare's own package: tests/golden/cloudflare.json
//! holds what @cloudflare/codemode 0.5.6 returns in Node (tests/golden/generate.mjs writes it) for
//! normalizeCode, sanitizeToolName, the code tool's types and description, and truncateResult.
//! tests/golden/v8-json.json holds V8's JSON.parse messages (tests/golden/v8-json.mjs).
//!
//! What the package does on workerd, tool calls included, is in tests/golden/fixtures and compared
//! by crates/engine/tests/conformance.rs.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use stellarator_api::{CodeFormat, CodeModeOptions, CodeNamespaces, McpTool, McpTransport, CODE_TOOL_DESCRIPTION};
use stellarator_codemode::{code_tool, json_parse_error, normalize_code, resend_arguments, sanitize_tool_name, truncate_result, SelectedServer};

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/cloudflare.json")).unwrap()
}

#[test]
fn the_default_description_is_cloudflares() {
    assert_eq!(golden()["default_description"], CODE_TOOL_DESCRIPTION);
}

#[test]
fn normalize_code_matches_cloudflare() {
    let g = golden();
    let cases = g["normalize"].as_array().unwrap();
    assert!(cases.len() > 40);
    for case in cases {
        let (input, want) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
        assert_eq!(normalize_code(input), want, "normalizeCode({input:?})");
    }
    // The README's example.
    assert_eq!(normalize_code("```js\nconst x = 1;\nx\n```"), "async () => {\nconst x = 1;\nreturn (x)\n}");
}

#[test]
fn sanitize_tool_name_matches_cloudflare() {
    for case in golden()["sanitize"].as_array().unwrap() {
        let (input, want) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
        assert_eq!(sanitize_tool_name(input), want, "sanitizeToolName({input:?})");
    }
    assert_eq!(sanitize_tool_name("my-tool"), "my_tool");
    assert_eq!(sanitize_tool_name("3d-render"), "_3d_render");
    assert_eq!(sanitize_tool_name("delete"), "delete_");
}

/// The servers a golden tool set describes: a namespace named `codemode` is one server's tools
/// under `codemode`; any other namespace is a server whose name sanitizes to it.
fn servers(spec: &Value) -> (Vec<SelectedServer>, CodeNamespaces) {
    let mut out = Vec::new();
    let mut mode = CodeNamespaces::Servers;
    for (i, ns) in spec["namespaces"].as_array().unwrap().iter().enumerate() {
        let name = ns["name"].as_str().unwrap();
        if name == "codemode" {
            mode = CodeNamespaces::Codemode;
        }
        let tools = ns["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| McpTool {
                name: t[0].as_str().unwrap().into(),
                description: t[1]["description"].as_str().map(String::from),
                input_schema: t[1]["inputSchema"].clone(),
                output_schema: t[1].get("outputSchema").cloned(),
            })
            .collect();
        out.push(SelectedServer {
            server_id: i as i64 + 1,
            name: name.replace("my_docs", "my-docs"),
            transport: McpTransport::Http,
            tools,
            instructions: None,
        });
    }
    (out, mode)
}

#[test]
fn types_and_description_match_cloudflare() {
    for case in golden()["code_tools"].as_array().unwrap() {
        let (servers, namespaces) = servers(&case["spec"]);
        let options = CodeModeOptions { namespaces, ..CodeModeOptions::default() };
        let tool = code_tool(&servers, &options).unwrap();
        assert_eq!(tool.snapshot.types, case["types"].as_str().unwrap());
        assert_eq!(tool.definition["function"]["description"], case["description"]);
    }
}

#[test]
fn the_readme_example_types() {
    let g = golden();
    let types = g["code_tools"][0]["types"].as_str().unwrap();
    assert_eq!(
        types,
        "type GetWeatherInput = {\n    /** City name */\n    city: string;\n}\ntype GetWeatherOutput = unknown\n\n\
         declare const codemode: {\n\t/**\n\t * Get weather for a city\n\t * @param input.city - City name\n\t */\n\t\
         getWeather: (input: GetWeatherInput) => Promise<GetWeatherOutput>;\n}"
    );
}

/// The value a golden truncation case describes (tests/golden/generate.mjs, `make`).
fn make(gen: &Value) -> Value {
    match gen["kind"].as_str().unwrap() {
        "big" => {
            let n = gen["n"].as_u64().unwrap();
            let items: Vec<Value> = (0..n)
                .map(|i| json!({"id": i, "title": format!("Item {i} {}", "x".repeat((i % 50) as usize)), "tags": ["a", "b"]}))
                .collect();
            json!({"items": items, "note": "y".repeat(gen["note"].as_u64().unwrap() as usize)})
        }
        "repeat" => json!(gen["text"].as_str().unwrap().repeat(gen["n"].as_u64().unwrap() as usize)),
        "range" => {
            let step = gen["step"].as_u64().unwrap();
            json!((0..gen["n"].as_u64().unwrap()).map(|i| i * step).collect::<Vec<_>>())
        }
        _ => gen["value"].clone(),
    }
}

#[test]
fn truncate_result_matches_cloudflare() {
    for case in golden()["truncate"].as_array().unwrap() {
        let value = make(&case["gen"]);
        let got = truncate_result(&value.to_string(), case["maxChars"].as_i64(), case["maxTokens"].as_i64().unwrap());
        assert_eq!(got, case["out"].as_str().unwrap(), "{}", case["gen"]);
    }
}

#[test]
fn the_code_tool_definition() {
    let g = golden();
    let (servers, _) = servers(&g["code_tools"][1]["spec"]);
    let tool = code_tool(&servers, &CodeModeOptions::default()).unwrap();
    assert_eq!(
        tool.definition["function"]["parameters"],
        json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
               "properties": {"code": {"type": "string", "description": "JavaScript async arrow function to execute"}},
               "required": ["code"], "additionalProperties": false})
    );
    // Key order as the AI SDK sends them.
    assert!(tool.definition.to_string().starts_with(r#"{"type":"function","function":{"name":"codemode","description":"Execute code"#));
    assert!(tool.definition["function"]["parameters"].to_string().starts_with(r#"{"$schema":"#));
    assert_eq!(tool.snapshot.namespaces[0].name, "mock");
    let methods: Vec<&str> = tool.snapshot.namespaces[0].methods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(methods, ["lookup", "add", "fail", "slow", "chart", "repeat"]);
    assert_eq!(tool.servers[0].tools[0].name, "mock.lookup");
    let custom = CodeModeOptions { tool_name: "run_js".into(), description: Some("Tools: {{types}} and {{types}}".into()), ..CodeModeOptions::default() };
    let tool = code_tool(&servers, &custom).unwrap();
    let d = tool.definition["function"]["description"].as_str().unwrap();
    // Only the first {{types}} is replaced, as String.replace does.
    assert!(d.starts_with("Tools: type LookupInput") && d.ends_with(" and {{types}}"), "{d}");
    assert_eq!(tool.definition["function"]["name"], "run_js");
}

#[test]
fn names_that_clash_or_shadow_are_refused() {
    let tool = |name: &str| McpTool { name: name.into(), description: None, input_schema: json!({"type": "object"}), output_schema: None };
    let server = |id: i64, name: &str, tools: &[&str]| SelectedServer {
        server_id: id,
        name: name.into(),
        transport: McpTransport::Http,
        tools: tools.iter().map(|t| tool(t)).collect(),
        instructions: None,
    };
    let o = CodeModeOptions::default();
    let err = code_tool(&[server(1, "my-docs", &["a"]), server(2, "my_docs", &["b"])], &o).unwrap_err();
    assert!(err.contains("`my_docs`"), "{err}");
    let err = code_tool(&[server(1, "console", &["a"])], &o).unwrap_err();
    assert!(err.contains("`console`"), "{err}");
    let err = code_tool(&[server(1, "gh", &["list-issues", "list_issues"])], &o).unwrap_err();
    assert!(err.contains("`gh.list_issues`"), "{err}");
    // Under codemode, tools of the same name on two servers get the server's name in front.
    let both = [server(1, "github", &["search"]), server(2, "docs", &["search", "read"])];
    let t = code_tool(&both, &CodeModeOptions { namespaces: CodeNamespaces::Codemode, ..o.clone() }).unwrap();
    let methods: Vec<(&str, &str)> =
        t.snapshot.namespaces[0].methods.iter().map(|m| (m.name.as_str(), m.tool.as_str())).collect();
    assert_eq!(methods, [("github__search", "search"), ("docs__search", "search"), ("read", "read")]);
    // Per server they need nothing.
    let t = code_tool(&both, &o).unwrap();
    assert_eq!(t.snapshot.namespaces.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["github", "docs"]);
    assert!(code_tool(&[server(1, "a", &[])], &o).unwrap_err().contains("no tools"));
}

#[test]
fn json_parse_errors_are_worded_as_v8_words_them() {
    let g: Value = serde_json::from_str(include_str!("golden/v8-json.json")).unwrap();
    let cases = g["cases"].as_array().unwrap();
    assert!(cases.len() > 400);
    for case in cases {
        let text = case[0].as_str().unwrap();
        assert_eq!(json_parse_error(text).as_deref(), case[1].as_str(), "JSON.parse({text:?})");
    }
    assert_eq!(json_parse_error("not json").as_deref(), Some("Unexpected token 'o', \"not json\" is not valid JSON"));
    assert_eq!(json_parse_error("{\"a\": 1}"), None);
}

#[test]
fn arguments_are_sent_back_as_the_ai_sdk_sends_them() {
    // A call the code tool accepted: the validated input, `code` alone.
    assert_eq!(resend_arguments(true, "{ \"code\" : \"async () => 3\", \"note\": \"x\" }"), r#"{"code":"async () => 3"}"#);
    assert_eq!(resend_arguments(true, "{\"code\": \"a\", \"code\": \"b\"}"), r#"{"code":"b"}"#);
    assert_eq!(resend_arguments(true, "{\"code\": \"\\u00e9 \\/\"}"), "{\"code\":\"é /\"}");
    // A refused one: the parsed input when it is an object, an array or null; else {}.
    assert_eq!(resend_arguments(false, "{\"code\": 5, \"n\": 1.0, \"big\": 12345678901234567890}"), r#"{"code":5,"n":1,"big":12345678901234567000}"#);
    assert_eq!(resend_arguments(false, "[\"x\"]"), r#"["x"]"#);
    assert_eq!(resend_arguments(false, "null"), "null");
    for refused in ["\"a string\"", "42", "true", "{\"code\": \"cut", "", "  ", "not json"] {
        assert_eq!(resend_arguments(false, refused), "{}", "{refused:?}");
    }
}

fn runtime_servers() -> Vec<SelectedServer> {
    let tool = |name: &str, description: &str, schema: Value, out: Option<Value>| McpTool {
        name: name.into(),
        description: Some(description.into()),
        input_schema: schema,
        output_schema: out,
    };
    vec![
        SelectedServer {
            server_id: 1,
            name: "mock".into(),
            transport: McpTransport::Http,
            tools: vec![tool("add", "Add two numbers.", json!({"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}), None)],
            instructions: None,
        },
        SelectedServer {
            server_id: 2,
            name: "issue-tracker".into(),
            transport: McpTransport::Http,
            tools: vec![
                tool("list-issues", "List issues.", json!({"type": "object", "properties": {"limit": {"type": "integer"}}}), Some(json!({"type": "object", "properties": {"total": {"type": "integer"}}}))),
                tool("delete", "Delete an issue.", json!({}), None),
            ],
            instructions: Some("Issue ids are integers.".into()),
        },
    ]
}

#[test]
fn the_runtime_format_lists_connectors_and_keeps_their_tools() {
    let options = CodeModeOptions { format: CodeFormat::Runtime, ..CodeModeOptions::default() };
    let tool = code_tool(&runtime_servers(), &options).unwrap();
    let f = &tool.definition["function"];
    // The input schema of the runtime's own tool: no $schema, no description.
    assert_eq!(
        tool.definition.to_string().split("\"parameters\":").nth(1).unwrap(),
        r#"{"type":"object","properties":{"code":{"type":"string"}},"required":["code"],"additionalProperties":false}}}"#
    );
    let description = f["description"].as_str().unwrap();
    assert!(description.starts_with("Execute JavaScript in a sandbox with access to connector SDKs.\n\n## Workflow\n\n1. `const matches = await codemode.search(\"short intent phrase\");`"), "{description}");
    assert!(description.contains("- The ONLY globals are `mock`, `issue_tracker` and `codemode` (plus standard JavaScript)."), "{description}");
    assert!(description.ends_with("## Available connectors\n\n- `mock`\n- `issue_tracker`"), "{description}");
    // The snapshot: the connectors as the runtime describes them, and each method's MCP tool.
    let s = &tool.snapshot;
    assert_eq!((s.kind, s.tool_name.as_str()), (CodeFormat::Runtime, "codemode"));
    assert_eq!(s.namespaces.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["mock", "issue_tracker"]);
    let methods: Vec<(&str, &str)> = s.namespaces[1].methods.iter().map(|m| (m.name.as_str(), m.tool.as_str())).collect();
    assert_eq!(methods, [("list_issues", "list-issues"), ("delete_", "delete")]);
    assert_eq!(tool.servers[1].tools[1].name, "issue_tracker.delete_");
    assert_eq!(s.connectors.len(), 2);
    assert_eq!((s.connectors[0].instructions.as_deref(), s.connectors[1].instructions.as_deref()), (None, Some("Issue ids are integers.")));
    // A schema without a type becomes an open object, as a connector's default.
    assert_eq!(s.connectors[1].tools[1].input_schema, json!({"type": "object"}));
    assert!(s.types.contains("declare const mock: {") && s.types.contains("Issue ids are integers.\n\ntype ListIssuesInput = {"), "{}", s.types);
    assert!(s.types.contains("delete_: (input: Delete_Input) => Promise<Delete_Output>;"), "{}", s.types);

    // Hints go next to the names; the servers' instructions can be left out; a description replaces the whole text.
    let hints: BTreeMap<String, String> = [("issue-tracker".to_string(), "Issues of the project.".to_string())].into();
    let hinted = code_tool(&runtime_servers(), &CodeModeOptions { connector_hints: hints, server_instructions: false, ..options.clone() }).unwrap();
    assert!(hinted.definition["function"]["description"].as_str().unwrap().ends_with("- `mock`\n- `issue_tracker` — Issues of the project."));
    assert_eq!(hinted.snapshot.connectors[1].hint.as_deref(), Some("Issues of the project."));
    assert_eq!(hinted.snapshot.connectors[1].instructions, None);
    let custom = code_tool(&runtime_servers(), &CodeModeOptions { description: Some("Run JavaScript.".into()), tool_name: "run_js".into(), ..options.clone() }).unwrap();
    assert_eq!((custom.definition["function"]["description"].as_str(), custom.definition["function"]["name"].as_str()), (Some("Run JavaScript."), Some("run_js")));

    // Names the runtime cannot take.
    let unknown: BTreeMap<String, String> = [("github".to_string(), "x".to_string())].into();
    let err = code_tool(&runtime_servers(), &CodeModeOptions { connector_hints: unknown, ..options.clone() }).unwrap_err();
    assert!(err.contains("\"github\"") && err.contains("mock, issue-tracker"), "{err}");
    let mut reserved = runtime_servers();
    reserved[0].name = "codemode".into();
    assert!(code_tool(&reserved, &options).unwrap_err().contains("`codemode` is the runtime's own namespace"));
    let mut clash = runtime_servers();
    clash[0].name = "issue_tracker".into();
    assert!(code_tool(&clash, &options).unwrap_err().contains("both be the connector `issue_tracker`"));
}
