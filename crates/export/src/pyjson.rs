//! JSON text the way Python's `json.dumps(obj, ensure_ascii=False)` writes it: `", "` between
//! items and `": "` after keys, non-ASCII kept as is. Exports match the Python backend's files
//! byte for byte, and `serde_json` is built with `preserve_order` so keys keep insertion order.

use serde::Serialize;
use serde_json::ser::{Formatter, Serializer};
use std::io::{self, Write};

struct PyFormatter;

impl Formatter for PyFormatter {
    fn begin_array_value<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            w.write_all(b", ")
        }
    }
    fn begin_object_key<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            w.write_all(b", ")
        }
    }
    fn begin_object_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(b": ")
    }
}

/// Append `value` to `out` as Python-style JSON.
pub fn write<T: Serialize + ?Sized>(out: &mut Vec<u8>, value: &T) {
    let mut ser = Serializer::with_formatter(&mut *out, PyFormatter);
    // Serializing a `Value` into a Vec cannot fail.
    let _ = value.serialize(&mut ser);
}

pub fn to_string<T: Serialize + ?Sized>(value: &T) -> String {
    let mut buf = Vec::new();
    write(&mut buf, value);
    String::from_utf8(buf).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_python_separators() {
        let v = json!({"b": [1, 2, {"x": null}], "a": "é\n", "t": true, "f": 0.7});
        assert_eq!(to_string(&v), r#"{"b": [1, 2, {"x": null}], "a": "é\n", "t": true, "f": 0.7}"#);
        assert_eq!(to_string(&json!({})), "{}");
        assert_eq!(to_string(&json!([])), "[]");
    }
}
