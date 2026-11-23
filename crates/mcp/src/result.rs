//! A tool result as text for the model: text parts joined, a short placeholder for each image,
//! audio clip or binary resource, and cut to the run's limit with a marker.

use serde_json::Value;

/// Collapse whitespace runs into single spaces and trim.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Approximate decoded size of base64 data, for placeholders.
fn size_of_base64(data: &str) -> String {
    let bytes = data.trim_end_matches('=').len() * 3 / 4;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} bytes")
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The text of one content block (MCP's JSON form).
pub fn block_text(block: &Value) -> String {
    match str_of(block, "type") {
        "text" => str_of(block, "text").to_string(),
        "image" => {
            let mime = str_of(block, "mimeType");
            format!("[image: {}, {}]", if mime.is_empty() { "unknown type" } else { mime }, size_of_base64(str_of(block, "data")))
        }
        "audio" => {
            let mime = str_of(block, "mimeType");
            format!("[audio: {}, {}]", if mime.is_empty() { "unknown type" } else { mime }, size_of_base64(str_of(block, "data")))
        }
        "resource" => {
            let r = block.get("resource").unwrap_or(&Value::Null);
            if let Some(t) = r.get("text").and_then(Value::as_str) {
                t.to_string()
            } else {
                let mime = str_of(r, "mimeType");
                let blob = str_of(r, "blob");
                format!(
                    "[resource: {}{}, {}]",
                    str_of(r, "uri"),
                    if mime.is_empty() { String::new() } else { format!(" ({mime})") },
                    size_of_base64(blob)
                )
            }
        }
        "resource_link" => {
            let name = str_of(block, "name");
            let uri = str_of(block, "uri");
            if name.is_empty() {
                format!("[resource link: {uri}]")
            } else {
                format!("[resource link: {name}, {uri}]")
            }
        }
        other => format!("[{} content]", if other.is_empty() { "unknown" } else { other }),
    }
}

/// (text, is_error) of a `tools/call` result in MCP's JSON form. Text parts are joined with a
/// newline; with no content at all, the structured content is used as JSON text.
pub fn result_text(result: &Value) -> (String, bool) {
    let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
    let parts: Vec<String> = result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().map(block_text).collect())
        .unwrap_or_default();
    let mut text = parts.join("\n");
    if parts.is_empty() {
        if let Some(sc) = result.get("structuredContent").filter(|v| !v.is_null()) {
            text = sc.to_string();
        }
    }
    (text, is_error)
}

/// `text` cut to `max_chars` characters with a marker saying how much was left out. Returns the
/// text and whether it was cut.
pub fn truncate(text: String, max_chars: usize) -> (String, bool) {
    let count = text.chars().count();
    if count <= max_chars {
        return (text, false);
    }
    let cut: String = text.chars().take(max_chars).collect();
    let more = count - max_chars;
    (format!("{cut}\n[truncated: {more} more characters]"), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_parts_join_and_others_become_placeholders() {
        let r = json!({"content": [
            {"type": "text", "text": "first"},
            {"type": "image", "data": "AAAA".repeat(1024), "mimeType": "image/png"},
            {"type": "text", "text": "second"},
            {"type": "audio", "data": "AAAA", "mimeType": "audio/wav"},
            {"type": "resource", "resource": {"uri": "file:///a.txt", "text": "inline"}},
            {"type": "resource", "resource": {"uri": "file:///b.bin", "mimeType": "application/zip", "blob": "AAAA"}},
            {"type": "resource_link", "uri": "file:///c", "name": "c"},
        ], "isError": false});
        let (text, err) = result_text(&r);
        assert!(!err);
        assert_eq!(
            text,
            "first\n[image: image/png, 3 KB]\nsecond\n[audio: audio/wav, 3 bytes]\ninline\n\
             [resource: file:///b.bin (application/zip), 3 bytes]\n[resource link: c, file:///c]"
        );
        assert_eq!(result_text(&json!({"content": [{"type": "text", "text": "no"}], "isError": true})), ("no".into(), true));
        assert_eq!(result_text(&json!({"content": [], "structuredContent": {"a": 1}})).0, r#"{"a":1}"#);
        assert_eq!(result_text(&json!({})), (String::new(), false));
    }

    #[test]
    fn truncation_marks_what_was_cut() {
        assert_eq!(truncate("héllo".into(), 5), ("héllo".into(), false));
        assert_eq!(truncate("héllo world".into(), 5), ("héllo\n[truncated: 6 more characters]".into(), true));
    }
}
