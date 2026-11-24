//! What the model's code gets from an MCP tool call: Cloudflare's `unwrapMcpResult`
//! (packages/codemode/src/mcp.ts and connectors/mcp.ts), so code sees plain values.

use serde_json::Value;

/// The value the code receives for a `tools/call` result (MCP's JSON form), or the message of the
/// Error the call throws:
/// 1. a compat `toolResult`, as is;
/// 2. `isError`: throws, with the text parts joined by newlines (or "Tool call failed");
/// 3. `structuredContent`, when present;
/// 4. only text parts: their text joined by newlines, parsed as JSON when it is JSON;
/// 5. anything else (images, audio, resources, no content): the whole result object.
pub fn unwrap_mcp_result(result: &Value) -> Result<Value, String> {
    if let Some(compat) = result.get("toolResult") {
        return Ok(compat.clone());
    }
    let content: &[Value] = result.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    let text_of = |c: &Value| c.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let is_text = |c: &Value| c.get("type").and_then(Value::as_str) == Some("text");
    if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
        let msg = content.iter().filter(|c| is_text(c)).map(text_of).collect::<Vec<_>>().join("\n");
        return Err(if msg.is_empty() { "Tool call failed".into() } else { msg });
    }
    if let Some(sc) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return Ok(sc.clone());
    }
    if !content.is_empty() && content.iter().all(is_text) {
        let text = content.iter().map(text_of).collect::<Vec<_>>().join("\n");
        return Ok(parse_json(&text).unwrap_or(Value::String(text)));
    }
    Ok(result.clone())
}

/// `JSON.parse(text)`, or None where it would throw. JSON.parse allows whitespace around the
/// value and nothing else; serde_json reads the same grammar.
fn parse_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn results_unwrap_as_cloudflare_does() {
        let text = |t: &str| json!({"content": [{"type": "text", "text": t}]});
        assert_eq!(unwrap_mcp_result(&text("9")), Ok(json!(9)));
        assert_eq!(unwrap_mcp_result(&text(" {\"a\": [1, 2]} ")), Ok(json!({"a": [1, 2]})));
        assert_eq!(unwrap_mcp_result(&text("High tide at 6.")), Ok(json!("High tide at 6.")));
        let two = json!({"content": [{"type": "text", "text": "[1,"}, {"type": "text", "text": "2]"}]});
        assert_eq!(unwrap_mcp_result(&two), Ok(json!([1, 2])));
        let err = json!({"content": [{"type": "text", "text": "locked"}, {"type": "image", "data": "x"}], "isError": true});
        assert_eq!(unwrap_mcp_result(&err), Err("locked".into()));
        assert_eq!(unwrap_mcp_result(&json!({"content": [], "isError": true})), Err("Tool call failed".into()));
        let structured = json!({"content": [{"type": "text", "text": "x"}], "structuredContent": {"n": 1}});
        assert_eq!(unwrap_mcp_result(&structured), Ok(json!({"n": 1})));
        let mixed = json!({"content": [{"type": "text", "text": "Chart:"}, {"type": "image", "data": "AA", "mimeType": "image/png"}]});
        assert_eq!(unwrap_mcp_result(&mixed), Ok(mixed.clone()));
        assert_eq!(unwrap_mcp_result(&json!({"content": []})), Ok(json!({"content": []})));
        assert_eq!(unwrap_mcp_result(&json!({"toolResult": "old"})), Ok(json!("old")));
    }
}
