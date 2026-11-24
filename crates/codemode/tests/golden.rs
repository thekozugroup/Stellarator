//! Code mode's helpers against the outputs of Cloudflare's own package: tests/golden/cloudflare.json
//! holds what @cloudflare/codemode 0.5.6 returns in Node (tests/golden/generate.mjs writes it) for
//! normalizeCode, sanitizeToolName, the code tool's types and description, and truncateResult.

use serde_json::{json, Value};
use stellarator_api::{CodeModeOptions, CodeNamespaces, McpTool, McpTransport, CODE_TOOL_DESCRIPTION};
use stellarator_codemode::{code_tool, normalize_code, sanitize_tool_name, truncate_result, SelectedServer};

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
