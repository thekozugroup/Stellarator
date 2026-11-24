//! The tool model's code mode: when the request's only tool is a Code Mode tool (one `code`
//! parameter, and a description declaring namespaces, as Cloudflare's `createCodeTool` writes it),
//! `mock-tools` answers the way a model in Code Mode does: it writes a JavaScript async arrow
//! function that calls one or two of the declared methods, then answers from the code's result.
//!
//! The code's shape follows the prompt: `code:plain`, `code:fence` (wrapped in a markdown fence,
//! statements with a trailing expression, for normalizeCode), `code:loop` (a for loop over
//! queries), `code:catch` (a try/catch around a failing tool), `code:parallel` (Promise.all),
//! `code:throw` (an uncaught tool error), `code:slow` (waits a second in `slow`, for the live
//! view), `code:syntax` (invalid JavaScript), `code:infinite`
//! (`while (true) {}`), `code:memory` (allocates until the memory limit). Without one, a hash of
//! the prompt picks among plain, fence, loop, catch and parallel.
//!
//! Given the tool of the durable runtime instead (`createCodemodeRuntime(...).tool()`: a
//! description that lists connectors and declares nothing), it works as that description tells a
//! model to, one execution per request: `codemode.search` with up to three words of the prompt
//! (numbers and words like "the" left out, since the runtime's search wants most of a query's
//! words to match), then `codemode.describe` of the best match (of the first connector when
//! nothing matched), then a call of that method with arguments read from the described types,
//! then the answer ([`write_runtime_code`]). `code:discover` searches and
//! describes in one execution; `code:catch`, `code:throw`, `code:step`, `code:slow`,
//! `code:syntax`, `code:infinite` and `code:memory` shape the call.

use serde_json::{json, Value};

/// The namespaces and methods a Code Mode description declares, in order: the lines
/// `declare const <ns>: {` and, inside, `\t<method>: (input: ...`.
pub fn declared_methods(description: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut inside = false;
    for line in description.lines() {
        if let Some(rest) = line.strip_prefix("declare const ") {
            if let Some(name) = rest.strip_suffix(": {") {
                out.push((name.to_string(), Vec::new()));
                inside = true;
                continue;
            }
        }
        if inside {
            if line == "}" {
                inside = false;
            } else if let Some(rest) = line.strip_prefix('\t') {
                if let Some((method, _)) = rest.split_once(": (input: ") {
                    if let Some(last) = out.last_mut() {
                        last.1.push(method.to_string());
                    }
                }
            }
        }
    }
    out
}

/// True when `tools` is one Code Mode tool.
pub fn is_code_tool(tools: &[(String, Value, String)]) -> bool {
    tools.len() == 1
        && tools[0].1.pointer("/properties/code/type").and_then(Value::as_str) == Some("string")
        && tools[0].2.contains("declare const ")
}

/// The connectors the durable runtime's description lists under `## Available connectors`, or
/// None when `tools` is not one such tool.
pub fn runtime_connectors(tools: &[(String, Value, String)]) -> Option<Vec<String>> {
    if tools.len() != 1 || tools[0].1.pointer("/properties/code/type").and_then(Value::as_str) != Some("string") {
        return None;
    }
    let description = &tools[0].2;
    if !description.contains("codemode.search(") {
        return None;
    }
    let (_, list) = description.split_once("## Available connectors\n")?;
    let names: Vec<String> = list
        .lines()
        .filter_map(|line| line.strip_prefix("- `"))
        .filter_map(|rest| rest.split_once('`'))
        .map(|(name, _)| name.to_string())
        .collect();
    (!names.is_empty()).then_some(names)
}

/// The properties of a described input type, `(name, optional, type)`: the lines four spaces deep
/// in `type <name> = {` of the TypeScript the runtime's `describe` returns.
fn input_properties(types: &str, name: &str) -> Vec<(String, bool, String)> {
    let mut out = Vec::new();
    let mut lines = types.lines().skip_while(|line| *line != format!("type {name} = {{"));
    lines.next();
    for line in lines {
        if line == "}" {
            break;
        }
        let Some(rest) = line.strip_prefix("    ") else { continue };
        if rest.starts_with(' ') || rest.starts_with('/') || rest.starts_with('}') || rest.starts_with('[') {
            continue;
        }
        let Some((key, kind)) = rest.split_once(": ") else { continue };
        let (key, optional) = match key.strip_suffix('?') {
            Some(key) => (key, true),
            None => (key, false),
        };
        out.push((key.trim_matches('"').to_string(), optional, kind.trim_end_matches(';').to_string()));
    }
    out
}

/// The input type's name of a method the described types declare (`\tadd: (input: AddInput) => ...`).
fn input_type_of(types: &str, method: &str) -> Option<String> {
    let (_, rest) = types.split_once(&format!("\t{method}: (input: "))?;
    Some(rest.split_once(')')?.0.to_string())
}

/// An object literal for a call, from the described input type: the required properties, and
/// `limit` and `seconds` when the type has them.
fn runtime_args(types: &str, type_name: &str, words: &str, nums: &[f64], slow: bool) -> String {
    let mut nums = nums.iter().copied().chain([2.0, 3.0]);
    let mut parts: Vec<String> = Vec::new();
    for (key, optional, kind) in input_properties(types, type_name) {
        if optional && key != "limit" && key != "seconds" {
            continue;
        }
        let value = match (key.as_str(), kind.as_str()) {
            ("limit", _) => json!(3),
            ("seconds", _) => json!(if slow { 1.0 } else { 0.2 }),
            ("times" | "id", "number") => json!(nums.next().unwrap_or(2.0) as i64),
            (_, "number") => num(nums.next().unwrap_or(2.0)),
            (_, "boolean") => json!(true),
            (_, "string") => json!(words),
            (_, kind) if kind.starts_with('"') => json!(kind[1..].split('"').next().unwrap_or("")),
            (_, kind) if kind.ends_with("[]") => json!([]),
            _ => json!({}),
        };
        let plain = key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') && !key.starts_with(|c: char| c.is_ascii_digit());
        parts.push(if plain { format!("{key}: {value}") } else { format!("{}: {value}", json!(key)) });
    }
    if parts.is_empty() {
        "{}".into()
    } else {
        format!("{{ {} }}", parts.join(", "))
    }
}

/// What a tool message of the runtime's tool holds: `(completed, result)`.
fn runtime_result(content: &str) -> Option<(bool, Value)> {
    let v: Value = serde_json::from_str(content).ok()?;
    let completed = v.get("status")?.as_str()? == "completed";
    Some((completed, v.get("result").cloned().unwrap_or(Value::Null)))
}

/// The next execution for the durable runtime's tool, and what the reasoning says about it; None
/// when the work is done (or failed) and the answer is due. `results` are the tool messages since
/// the question; `keep_going` is the model that never answers.
pub fn write_runtime_code(connectors: &[String], prompt: &str, results: &[String], hash: u64, keep_going: bool) -> Option<(String, String)> {
    let shape = ["plain", "discover", "catch", "throw", "step", "slow", "syntax", "infinite", "memory"]
        .into_iter()
        .find(|s| prompt.contains(&format!("code:{s}")))
        .unwrap_or(["plain", "logged"][(hash % 2) as usize]);
    let words: String = prompt
        .split_whitespace()
        .filter(|w| !w.starts_with("code:") && !w.starts_with("tool:") && *w != "badargs")
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['"', '\\'], "'");
    let nums: Vec<f64> = prompt
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|w| w.trim_matches('.').parse::<f64>().ok())
        .collect();
    // A short intent phrase for the search.
    const SMALL_WORDS: [&str; 32] = [
        "a", "an", "the", "and", "or", "of", "to", "is", "are", "what", "which", "please", "for", "in", "on", "with", "me", "my", "it", "at", "by",
        "all", "some", "this", "that", "how", "do", "does", "can", "you", "i", "be",
    ];
    let intent: String = prompt
        .split_whitespace()
        .filter(|w| !w.starts_with("code:") && !w.starts_with("tool:") && *w != "badargs")
        .flat_map(|w| w.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')))
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()) && !SMALL_WORDS.contains(&w.to_lowercase().as_str()))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let intent = if intent.is_empty() { words.clone() } else { intent };

    // Where the conversation stands: the last search, the last description, and the calls after it.
    let mut searched: Option<Option<String>> = None;
    let mut described: Option<(String, String, bool)> = None;
    let mut called = 0;
    for content in results {
        let Some((completed, result)) = runtime_result(content) else {
            if described.is_some() {
                called += 1;
            }
            continue;
        };
        let is_description = result.get("types").is_some_and(Value::is_string) && result.get("path").is_some_and(Value::is_string);
        if completed && is_description && described.is_none() {
            let path = result["path"].as_str().unwrap_or("").to_string();
            let types = result["types"].as_str().unwrap_or("").to_string();
            let found = !types.ends_with(" not found.");
            described = Some((path, types, found));
        } else if completed && result.get("results").is_some_and(Value::is_array) && described.is_none() {
            searched = Some(result.pointer("/results/0/path").and_then(Value::as_str).map(str::to_string));
        } else if described.is_some() {
            called += 1;
        } else if !keep_going {
            // An execution failed before anything was found: the answer says so.
            return None;
        }
    }

    let Some((path, types, found)) = described else {
        return Some(match searched {
            None if shape == "discover" => (
                format!(
                    "async () => {{\n  const matches = await codemode.search(\"{intent}\");\n  if (matches.results.length === 0) return matches;\n  return codemode.describe(matches.results[0].path);\n}}"
                ),
                "I have not used these connectors yet. I'll search for a method and read its types in one pass.".to_string(),
            ),
            None => (
                format!("async () => codemode.search(\"{intent}\")"),
                "I have not used these connectors in this conversation, so I'll search for a method first.".to_string(),
            ),
            Some(Some(path)) => (
                format!("async () => codemode.describe(\"{path}\")"),
                format!("The search found {path}. I'll read its types before I call it."),
            ),
            Some(None) => (
                format!("async () => codemode.describe(\"{}\")", connectors[0]),
                format!("The search found nothing. I'll read what the connector {} offers.", connectors[0]),
            ),
        });
    };
    if !found || (called > 0 && !keep_going) {
        return None;
    }
    // The method to call: the described one, or the first a described connector declares.
    let (connector, method, type_name) = match path.split_once('.') {
        Some((connector, method)) => (connector.to_string(), method.to_string(), types.strip_prefix("type ").and_then(|t| t.split_once(" = ")).map(|(n, _)| n.to_string())),
        None => {
            let declared = declared_methods(&types);
            let method = declared.iter().find(|(name, _)| *name == path).and_then(|(_, methods)| methods.first())?.clone();
            let type_name = input_type_of(&types, &method);
            (path.clone(), method, type_name)
        }
    };
    let args = type_name.map(|n| runtime_args(&types, &n, &words, &nums, shape == "slow")).unwrap_or_else(|| "{}".into());
    let call = format!("{connector}.{method}({args})");
    let code = match shape {
        "catch" => format!(
            "async () => {{\n  let note = null;\n  try {{\n    await {connector}.missing_tool({{}});\n  }} catch (e) {{\n    note = e.message;\n    console.log(\"failed as expected:\", e.message);\n  }}\n  const value = await {call};\n  return {{ value, note }};\n}}"
        ),
        "throw" => format!("async () => {{\n  const value = await {connector}.missing_tool({{}});\n  return value;\n}}"),
        "step" => format!(
            "async () => {{\n  const day = await codemode.step(\"day\", () => new Date().toISOString().slice(0, 10));\n  const result = await {call};\n  return {{ day, result }};\n}}"
        ),
        "syntax" => format!("async () => {{\n  const value = await {call}\n  return value +;\n}}"),
        "infinite" => "async () => {\n  let n = 0;\n  while (true) {\n    n++;\n  }\n}".to_string(),
        "memory" => "async () => {\n  const parts = [];\n  for (;;) parts.push(\"x\".repeat(1 << 16) + parts.length);\n}".to_string(),
        "logged" => format!("async () => {{\n  const result = await {call};\n  console.log(\"called {connector}.{method}\");\n  return result;\n}}"),
        _ => format!("async () => {{\n  const result = await {call};\n  return result;\n}}"),
    };
    Some((code, format!("Now I know the input of {connector}.{method}. I'll call it and return what I need ({shape}).")))
}

/// A number as JavaScript writes it: 4, not 4.0.
fn num(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn args_for(method: &str, words: &str, n: (f64, f64)) -> String {
    let v = match method {
        "lookup" => json!({"query": words}),
        "add" => json!({"a": num(n.0), "b": num(n.1)}),
        "fail" => json!({"reason": "checking"}),
        "slow" => json!({"seconds": 0.2}),
        "repeat" => json!({"text": "ab", "times": 3}),
        _ => json!({}),
    };
    // Object literal style a model writes: { query: "..." }.
    let Value::Object(map) = v else { return "{}".into() };
    if map.is_empty() {
        return "{}".into();
    }
    let parts: Vec<String> = map.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    format!("{{ {} }}", parts.join(", "))
}

/// The code for this prompt, and what the reasoning says about it.
pub fn write_code(description: &str, prompt: &str, hash: u64) -> Option<(String, String)> {
    let declared = declared_methods(description);
    let (ns, methods) = declared.iter().find(|(_, m)| !m.is_empty())?;
    let has = |m: &str| methods.iter().any(|x| x == m);
    let first = if has("lookup") { "lookup".to_string() } else { methods[0].clone() };
    let second = if has("add") && first != "add" {
        "add".to_string()
    } else {
        methods.iter().find(|m| **m != first).cloned().unwrap_or_else(|| first.clone())
    };
    let words: String = prompt
        .split_whitespace()
        .filter(|w| !w.starts_with("code:") && !w.starts_with("tool:"))
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'");
    let nums: Vec<f64> = prompt
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|w| w.trim_matches('.').parse::<f64>().ok())
        .collect();
    let n = (nums.first().copied().unwrap_or(2.0), nums.get(1).copied().unwrap_or(3.0));
    let a1 = args_for(&first, &words, n);
    let a2 = args_for(&second, &words, n);
    let shape = ["plain", "fence", "loop", "catch", "parallel", "throw", "slow", "syntax", "infinite", "memory"]
        .into_iter()
        .find(|s| prompt.contains(&format!("code:{s}")))
        .unwrap_or(["plain", "fence", "loop", "catch", "parallel"][(hash % 5) as usize]);
    let failing = if has("fail") { format!("{ns}.fail({{ reason: \"checking\" }})") } else { format!("{ns}.missing_tool({{}})") };
    let code = match shape {
        "fence" => format!(
            "```js\nconst first = await {ns}.{first}({a1});\nconst second = await {ns}.{second}({a2});\n({{ first, second }})\n```"
        ),
        "loop" => {
            let call = if first == "lookup" { format!("{ns}.lookup({{ query: q }})") } else { format!("{ns}.{first}({a1})") };
            format!(
                "async () => {{\n  const results = [];\n  for (const q of [\"{words}\", \"{words} history\"]) {{\n    results.push(await {call});\n  }}\n  console.log(\"looked up\", results.length);\n  return results;\n}}"
            )
        }
        "catch" => format!(
            "async () => {{\n  let note = null;\n  try {{\n    await {failing};\n  }} catch (e) {{\n    note = e.message;\n    console.log(\"failed as expected:\", e.message);\n  }}\n  const value = await {ns}.{first}({a1});\n  return {{ value, note }};\n}}"
        ),
        "parallel" => format!(
            "async () => {{\n  const [a, b] = await Promise.all([\n    {ns}.{first}({a1}),\n    {ns}.{second}({a2}),\n  ]);\n  return {{ a, b }};\n}}"
        ),
        "throw" => format!("async () => {{\n  const value = await {failing};\n  return value;\n}}"),
        "slow" if has("slow") => format!("async () => {{\n  const waited = await {ns}.slow({{ seconds: 1 }});\n  return waited;\n}}"),
        "syntax" => format!("async () => {{\n  const value = await {ns}.{first}({a1})\n  return value +;\n}}"),
        "infinite" => "async () => {\n  let n = 0;\n  while (true) {\n    n++;\n  }\n}".to_string(),
        "memory" => "async () => {\n  const parts = [];\n  for (;;) parts.push(\"x\".repeat(1 << 16) + parts.length);\n}".to_string(),
        _ => format!("async () => {{\n  const result = await {ns}.{first}({a1});\n  return result;\n}}"),
    };
    let reasoning = format!("I'll write code that calls {ns}.{first} and returns what I need ({shape}).");
    Some((code, reasoning))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIPTION: &str = "Execute code to achieve a goal.\n\nAvailable:\ntype LookupInput = {\n    query: string;\n}\n\
        type LookupOutput = unknown\n\ndeclare const mock: {\n\t/**\n\t * Look up.\n\t */\n\tlookup: (input: LookupInput) => \
        Promise<LookupOutput>;\n\n\t/**\n\t * Add.\n\t */\n\tadd: (input: AddInput) => Promise<AddOutput>;\n\n\t/**\n\t * Fail.\n\t \
        */\n\tfail: (input: FailInput) => Promise<FailOutput>;\n}\n\nWrite an async arrow function";

    #[test]
    fn methods_come_from_the_declarations() {
        assert_eq!(declared_methods(DESCRIPTION), [("mock".to_string(), vec!["lookup".to_string(), "add".into(), "fail".into()])]);
    }

    #[test]
    fn code_follows_the_prompt() {
        let (code, _) = write_code(DESCRIPTION, "What is 4 + 5? code:parallel", 0).unwrap();
        assert!(code.contains("mock.lookup({ query: \"What is 4 +\" })") && code.contains("mock.add({ a: 4, b: 5 })"), "{code}");
        let (code, _) = write_code(DESCRIPTION, "tides code:fence", 0).unwrap();
        assert!(code.starts_with("```js\n") && code.ends_with("({ first, second })\n```"), "{code}");
        let (code, _) = write_code(DESCRIPTION, "x code:catch", 0).unwrap();
        assert!(code.contains("try {\n    await mock.fail({ reason: \"checking\" });"), "{code}");
        assert!(write_code("no declarations", "x", 0).is_none());
    }

    const RUNTIME: &str = "Execute JavaScript in a sandbox with access to connector SDKs.\n\n## Workflow\n\n1. `const matches = await \
        codemode.search(\"short intent phrase\");`\n\n## Available connectors\n\n- `mock`\n- `tracker` \u{2014} Issues.";

    fn runtime_tool(description: &str) -> Vec<(String, Value, String)> {
        vec![("codemode".to_string(), json!({"type": "object", "properties": {"code": {"type": "string"}}}), description.to_string())]
    }

    #[test]
    fn the_runtime_tool_is_told_from_the_code_tool() {
        assert_eq!(runtime_connectors(&runtime_tool(RUNTIME)), Some(vec!["mock".to_string(), "tracker".to_string()]));
        assert_eq!(runtime_connectors(&runtime_tool(DESCRIPTION)), None);
        assert_eq!(runtime_connectors(&runtime_tool("Run JavaScript.")), None);
        assert!(is_code_tool(&runtime_tool(DESCRIPTION)) && !is_code_tool(&runtime_tool(RUNTIME)));
    }

    const LIST_TYPES: &str = "type ListIssuesInput = {\n    /** Which issues to list (default open). */\n    state?: \"open\" | \"closed\" | \"all\";\n    \
        /** At most this many. */\n    limit?: number;\n}\ntype ListIssuesOutput = {\n    issues: {\n        id: number;\n        title: string;\n    }[];\n    total: number;\n}";

    fn completed(result: Value) -> String {
        json!({"status": "completed", "executionId": "exec_0001795544686083_4bdda14a-b02c-4c72-a2fa-5eb5117028b9", "result": result}).to_string()
    }

    #[test]
    fn runtime_code_searches_describes_then_calls() {
        let connectors = ["mock".to_string(), "tracker".to_string()];
        let prompt = "list the open issues, please code:plain";
        let (code, _) = write_runtime_code(&connectors, prompt, &[], 0, false).unwrap();
        assert_eq!(code, "async () => codemode.search(\"list open issues\")");
        assert_eq!(write_runtime_code(&connectors, "What is 4 + 5?", &[], 0, false).unwrap().0, "async () => codemode.search(\"What is 4 +\")");
        assert_eq!(write_runtime_code(&connectors, "Add 4 and 5.5, please", &[], 0, false).unwrap().0, "async () => codemode.search(\"Add\")");
        let search = completed(json!({"results": [{"path": "tracker.list_issues", "connector": "tracker", "method": "list_issues", "kind": "method", "score": 295}], "total": 1, "truncated": false}));
        let (code, reasoning) = write_runtime_code(&connectors, prompt, &[search.clone()], 0, false).unwrap();
        assert_eq!(code, "async () => codemode.describe(\"tracker.list_issues\")");
        assert!(reasoning.contains("tracker.list_issues"));
        let describe = completed(json!({"path": "tracker.list_issues", "description": "List issues.", "types": LIST_TYPES, "kind": "method"}));
        let (code, _) = write_runtime_code(&connectors, prompt, &[search.clone(), describe.clone()], 0, false).unwrap();
        assert_eq!(code, "async () => {\n  const result = await tracker.list_issues({ limit: 3 });\n  return result;\n}");
        // After the call: the answer, unless the model never answers.
        let called = completed(json!({"issues": [], "total": 0}));
        let all = [search.clone(), describe.clone(), called];
        assert!(write_runtime_code(&connectors, prompt, &all, 0, false).is_none());
        assert!(write_runtime_code(&connectors, prompt, &all, 0, true).unwrap().0.contains("tracker.list_issues({ limit: 3 })"));
        // An execution that failed before anything was found ends the work.
        let failed = json!({"status": "error", "executionId": "exec_1", "error": "Code execution failed: boom"}).to_string();
        assert!(write_runtime_code(&connectors, prompt, &[failed], 0, false).is_none());
    }

    #[test]
    fn runtime_code_reads_arguments_from_the_described_types() {
        let connectors = ["mock".to_string()];
        let none = completed(json!({"results": [], "total": 0, "truncated": false}));
        let (code, _) = write_runtime_code(&connectors, "what is 4 + 5.5 code:catch", &[none.clone()], 0, false).unwrap();
        assert_eq!(code, "async () => codemode.describe(\"mock\")");
        let types = "type AddInput = {\n    a: number;\n    b: number;\n}\ntype AddOutput = unknown\ntype LookupInput = {\n    /** The term. */\n    query: string;\n}\n\
            type LookupOutput = unknown\n\ndeclare const mock: {\n\t/**\n\t * Add two numbers.\n\t */\n\tadd: (input: AddInput) => Promise<AddOutput>;\n\n\t/**\n\t * Look up.\n\t */\n\t\
            lookup: (input: LookupInput) => Promise<LookupOutput>;\n}";
        let connector = completed(json!({"path": "mock", "types": types, "kind": "connector"}));
        let (code, _) = write_runtime_code(&connectors, "what is 4 + 5.5 code:catch", &[none.clone(), connector], 0, false).unwrap();
        assert!(code.contains("await mock.missing_tool({});") && code.contains("const value = await mock.add({ a: 4, b: 5.5 });"), "{code}");
        // One pass of discovery, then a method described on its own.
        let (code, _) = write_runtime_code(&connectors, "tides code:discover", &[], 0, false).unwrap();
        assert!(code.contains("codemode.search(\"tides\")") && code.contains("return codemode.describe(matches.results[0].path);"), "{code}");
        let method = completed(json!({"path": "mock.lookup", "types": "type LookupInput = {\n    /** The term. */\n    query: string;\n}\ntype LookupOutput = unknown", "kind": "method"}));
        let (code, _) = write_runtime_code(&connectors, "tides code:discover", &[method], 1, false).unwrap();
        assert_eq!(code, "async () => {\n  const result = await mock.lookup({ query: \"tides\" });\n  return result;\n}");
        // A path the runtime does not know: nothing to call.
        let unknown = completed(json!({"path": "mock.nope", "types": "\"mock.nope\" not found.", "kind": "method"}));
        assert!(write_runtime_code(&connectors, "x", &[unknown], 0, false).is_none());
    }
}
