//! Coverage for the SSE event-block parser in `tinker.rs`.
//!
//! The parser is intentionally `pub(crate)`, so we exercise it through a tiny
//! re-export shim in the library. To avoid plumbing extra public surface, the
//! shim simply re-runs the same logic the streaming task uses.

use stellarator_supervisor::tinker::{sse_extract_data_for_test, sse_find_terminator_for_test};

#[test]
fn single_line_event_decodes_payload() {
    let buf = b"data: {\"step\":1,\"name\":\"loss\",\"value\":0.5}\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    let payload = sse_extract_data_for_test(event).expect("payload");
    assert_eq!(payload, "{\"step\":1,\"name\":\"loss\",\"value\":0.5}");
}

#[test]
fn multi_line_data_is_joined_with_newline() {
    let buf = b"data: line1\ndata: line2\ndata: line3\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    let payload = sse_extract_data_for_test(event).expect("payload");
    assert_eq!(payload, "line1\nline2\nline3");
}

#[test]
fn comments_and_other_fields_are_ignored() {
    let buf = b": this is a comment heartbeat\nevent: ignored\nid: 42\ndata: hello\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    let payload = sse_extract_data_for_test(event).expect("payload");
    assert_eq!(payload, "hello");
}

#[test]
fn pure_heartbeat_event_yields_no_payload() {
    let buf = b": keep-alive\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    assert!(sse_extract_data_for_test(event).is_none());
}

#[test]
fn back_to_back_events_split_on_blank_line() {
    let mut buf: Vec<u8> = b"data: a\n\ndata: b\n\n".to_vec();
    let pos1 = sse_find_terminator_for_test(&buf).expect("first terminator");
    let event1: Vec<u8> = buf.drain(..pos1 + 2).collect();
    assert_eq!(sse_extract_data_for_test(&event1).as_deref(), Some("a"));
    let pos2 = sse_find_terminator_for_test(&buf).expect("second terminator");
    let event2: Vec<u8> = buf.drain(..pos2 + 2).collect();
    assert_eq!(sse_extract_data_for_test(&event2).as_deref(), Some("b"));
    assert!(buf.is_empty());
}

#[test]
fn leading_space_after_data_colon_is_stripped_only_once() {
    // Spec: strip exactly one leading space. A second space is preserved.
    let buf = b"data:  hello\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    assert_eq!(
        sse_extract_data_for_test(event).as_deref(),
        Some(" hello"),
        "second space should remain"
    );
}

#[test]
fn cr_suffix_on_data_line_is_stripped() {
    // Test that a trailing CR on a data line is stripped per spec.
    // When parsing "data: hello\r\n\n", after split('\n') we get ["data: hello\r", "", ""].
    // The CR is stripped from "data: hello\r" → "data: hello", then we extract "hello".
    let buf = b"data: hello\r\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    assert_eq!(
        sse_extract_data_for_test(event).as_deref(),
        Some("hello"),
        "CR suffix on data line should be stripped"
    );
}

#[test]
fn multi_line_with_cr_suffixes() {
    // Multi-line data where each line has a CR suffix.
    let buf = b"data: line1\r\ndata: line2\r\ndata: line3\r\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    assert_eq!(
        sse_extract_data_for_test(event).as_deref(),
        Some("line1\nline2\nline3"),
        "CR should be stripped from each line before joining with LF"
    );
}

#[test]
fn empty_input_yields_no_event() {
    let buf = b"";
    assert!(sse_find_terminator_for_test(buf).is_none());
}

#[test]
fn incomplete_event_no_terminator() {
    let buf = b"data: hello\n";
    assert!(sse_find_terminator_for_test(buf).is_none());
}

#[test]
fn no_data_field_yields_none() {
    // Event with only metadata fields.
    let buf = b"event: update\nid: 123\nretry: 5000\n\n";
    let pos = sse_find_terminator_for_test(buf).expect("terminator");
    let event = &buf[..pos + 2];
    assert!(sse_extract_data_for_test(event).is_none());
}

#[test]
fn multiple_events_in_buffer() {
    // Simulate streaming: consume first, then second event.
    let mut buf = b"data: first\n\ndata: second\n\n".to_vec();

    let pos1 = sse_find_terminator_for_test(&buf).expect("first terminator");
    let event1: Vec<u8> = buf.drain(..pos1 + 2).collect();
    assert_eq!(sse_extract_data_for_test(&event1).as_deref(), Some("first"));

    let pos2 = sse_find_terminator_for_test(&buf).expect("second terminator");
    let event2: Vec<u8> = buf.drain(..pos2 + 2).collect();
    assert_eq!(sse_extract_data_for_test(&event2).as_deref(), Some("second"));

    assert!(buf.is_empty());
}
