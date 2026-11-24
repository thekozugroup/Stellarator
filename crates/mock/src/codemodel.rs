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
}
