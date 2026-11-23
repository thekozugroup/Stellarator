//! Words and rules the run screen shares (frontend/src/components/runs/run-data.ts and the helpers
//! of pages/run-detail.tsx), as pure functions.

use crate::api::types::{EndpointOut, RunOut, RunStatus, SampleStatus, SampleSummary};
use crate::format::{DASH, int};
use crate::screens::endpoints::EditFocus;

/// A run that is going: running, or pausing (its in-flight requests still finishing).
pub fn is_active(status: RunStatus) -> bool {
    matches!(status, RunStatus::Running | RunStatus::Pausing)
}

/// How often the run is read again, in seconds: every second while it goes, every 10 s while
/// paused, never once finished (`useRunLive`).
pub fn poll_every(status: RunStatus) -> Option<u32> {
    match status {
        RunStatus::Running | RunStatus::Pausing => Some(1),
        RunStatus::Paused => Some(10),
        RunStatus::Completed => None,
    }
}

/// "#12", or "#12.2" when the run takes several samples per prompt. Prompt numbers are 1-based.
pub fn sample_label(prompt_idx: i64, sample_idx: i64, samples_per_prompt: i64) -> String {
    if samples_per_prompt > 1 { format!("#{}.{}", prompt_idx + 1, sample_idx + 1) } else { format!("#{}", prompt_idx + 1) }
}

/// The finish reason in words, and whether it is a warning (coral). Truncation is explained,
/// never shown as a bare "length".
pub fn finish_reason_text(reason: Option<&str>) -> Option<(String, bool)> {
    let reason = reason.filter(|r| !r.is_empty())?;
    Some(match reason {
        "length" => ("Truncated (max tokens)".into(), true),
        "stop" => ("Stop".into(), false),
        "content_filter" => ("Stopped by the provider's content filter.".into(), true),
        "tool_calls" => ("Stopped to call a tool.".into(), false),
        other => (format!("Stopped with reason “{other}”."), false),
    })
}

/// A message's role in words.
pub fn role_label(role: &str) -> String {
    match role {
        "system" => "System".into(),
        "user" => "User".into(),
        "assistant" => "Assistant".into(),
        other => other.into(),
    }
}

/// The samples table's status filter (its tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleFilter {
    #[default]
    All,
    Done,
    Failed,
    Truncated,
    Pending,
}

impl SampleFilter {
    pub const ALL: [SampleFilter; 5] = [SampleFilter::All, SampleFilter::Done, SampleFilter::Failed, SampleFilter::Truncated, SampleFilter::Pending];

    pub fn label(self) -> &'static str {
        match self {
            SampleFilter::All => "All",
            SampleFilter::Done => "Done",
            SampleFilter::Failed => "Failed",
            SampleFilter::Truncated => "Truncated",
            SampleFilter::Pending => "Pending",
        }
    }

    /// The API's `status` parameter; none for All.
    pub fn param(self) -> Option<&'static str> {
        match self {
            SampleFilter::All => None,
            SampleFilter::Done => Some("done"),
            SampleFilter::Failed => Some("failed"),
            SampleFilter::Truncated => Some("truncated"),
            SampleFilter::Pending => Some("pending"),
        }
    }

    /// How many of the run's samples the tab holds.
    pub fn count(self, run: &RunOut) -> i64 {
        match self {
            SampleFilter::All => run.total,
            SampleFilter::Done => run.done,
            SampleFilter::Failed => run.failed,
            SampleFilter::Truncated => run.truncated,
            SampleFilter::Pending => run.pending,
        }
    }

    /// What an empty tab says; with a search, what found nothing.
    pub fn empty_text(self, q: &str) -> String {
        if !q.is_empty() {
            let which = if self == SampleFilter::All { String::new() } else { format!("{} ", self.label().to_lowercase()) };
            return format!("No {which}samples match “{q}”.");
        }
        match self {
            SampleFilter::Failed => "No failed samples.".into(),
            SampleFilter::Truncated => "No truncated samples.".into(),
            SampleFilter::Pending => "Nothing waiting.".into(),
            SampleFilter::Done => "No finished samples yet.".into(),
            SampleFilter::All => "No samples yet.".into(),
        }
    }
}

/// Rows a page of samples holds.
pub const PAGE_SIZE: i64 = 50;

/// The samples API's query: the filter, the search and the page (1-based).
pub fn samples_query(filter: SampleFilter, q: &str, page: i64) -> String {
    let mut parts = Vec::new();
    if let Some(s) = filter.param() {
        parts.push(format!("status={s}"));
    }
    if !q.is_empty() {
        parts.push(format!("q={}", encode(q)));
    }
    parts.push(format!("offset={}", (page - 1).max(0) * PAGE_SIZE));
    parts.push(format!("limit={PAGE_SIZE}"));
    parts.join("&")
}

/// `URLSearchParams`' encoding of a value.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The status word a row shows: Truncated for a done sample cut off at max tokens.
pub fn status_word(s: &SampleSummary) -> &'static str {
    if s.status == SampleStatus::Done && s.finish_reason.as_deref() == Some("length") {
        return SampleFilter::Truncated.label();
    }
    match s.status {
        SampleStatus::Running => "In flight",
        SampleStatus::Done => "Done",
        SampleStatus::Failed => "Failed",
        SampleStatus::Pending => "Pending",
    }
}

/// Under a one-status tab every row would repeat the tab's name, so the status column goes.
pub fn show_status(filter: SampleFilter, items: &[SampleSummary]) -> bool {
    filter == SampleFilter::All || items.iter().any(|s| status_word(s) != filter.label())
}

/// A finished sample without a thinking trace (the Thinking column marks those).
pub fn no_trace(s: &SampleSummary) -> bool {
    s.status == SampleStatus::Done && !s.has_reasoning
}

/// "Showing 51–100 of 1,284", and how many pages there are.
pub fn page_span(page: i64, total: i64) -> (i64, i64, i64) {
    let pages = ((total + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    let from = if total > 0 { (page - 1) * PAGE_SIZE + 1 } else { 0 };
    let to = total.min(page * PAGE_SIZE);
    (from, to, pages)
}

/// Elapsed time as a clock for a log row: "0:42" or "1:02:05".
pub fn clock(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{ss:02}") } else { format!("{m}:{ss:02}") }
}

/// A duration as a figure's parts, the numbers with their units ("2 h 14 min" is [(2, "h"), (14,
/// "min")]), for a half-width tile; `None` for "<1 s".
pub fn duration_parts(seconds: f64) -> Option<Vec<(i64, &'static str)>> {
    if seconds < 1.0 {
        return None;
    }
    let s = crate::format::js_round(seconds) as i64;
    let m = s / 60;
    let h = m / 60;
    Some(if s < 60 {
        vec![(s, "s")]
    } else if m < 60 {
        vec![(m, "min"), (s % 60, "s")]
    } else if h < 48 {
        vec![(h, "h"), (m % 60, "min")]
    } else {
        vec![(h / 24, "d"), (h % 24, "h")]
    })
}

/// `n` rounded, as the web UI's `int` writes a float.
pub fn int_f(n: f64) -> String {
    if n.is_finite() { int(crate::format::js_round(n) as i64) } else { DASH.into() }
}

// ---------------------------------------------------------------------------- why a run stopped

/// The words of a problem in a run's reason (`PROBLEM` in run-detail.tsx: fail, error, could not,
/// unreach, refused, timed out, rate limit, 429, 5xx, circuit).
pub fn is_problem(text: &str) -> bool {
    let t = text.to_lowercase();
    ["fail", "error", "unreach", "refused", "rate limit", "429", "circuit"].iter().any(|w| t.contains(w))
        || could_not(&t, "")
        || timed_out(&t)
        || five_hundred(&t)
}

/// The endpoint itself needs fixing: a rejected key, or a server that cannot be reached
/// (`ENDPOINT_PROBLEM`: 401 or 403 as words, api key, could not connect, unreach, refused,
/// resolve).
pub fn is_endpoint_problem(text: &str) -> bool {
    let t = text.to_lowercase();
    word(&t, "401")
        || word(&t, "403")
        || ["api key", "unreach", "refused", "resolve"].iter().any(|w| t.contains(w))
        || could_not(&t, " connect")
}

/// Which edit field fixes the endpoint: the URL when the server did not answer, else the key.
pub fn edit_focus_for(text: &str) -> EditFocus {
    let t = text.to_lowercase();
    if ["could not connect", "unreachable", "timed out", "took too long"].iter().any(|w| t.contains(w)) {
        EditFocus::Url
    } else {
        EditFocus::Key
    }
}

/// `could ?n[o']t` followed by `then` in lowercase `t`.
fn could_not(t: &str, then: &str) -> bool {
    ["could not", "couldnot", "could n't", "couldn't"].iter().any(|w| t.contains(&format!("{w}{then}")))
}

/// `timed? ?out`.
fn timed_out(t: &str) -> bool {
    ["timeout", "time out", "timedout", "timed out"].iter().any(|w| t.contains(w))
}

/// `5\d\d`: a 5 and two digits.
fn five_hundred(t: &str) -> bool {
    let b = t.as_bytes();
    b.windows(3).any(|w| w[0] == b'5' && w[1].is_ascii_digit() && w[2].is_ascii_digit())
}

/// `\b<w>\b` for a word of digits.
fn word(t: &str, w: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    t.match_indices(w).any(|(i, _)| !is_word(t[..i].chars().next_back()) && !is_word(t[i + w.len()..].chars().next()))
}

/// Why the endpoint's last check failed: the words, or `None` when it passed or never ran.
fn trouble(ep: &EndpointOut) -> Option<&'static str> {
    crate::screens::endpoints::status::endpoint_trouble(Some(ep)).map(|t| t.word())
}

/// What the screen knows of a finished run's failures: whether the endpoint caused them
/// (`useEndpointProblem`). `endpoint` and `first_failed` are the answers so far (`None` while a
/// request is out; `Some(None)` for no failed sample).
#[derive(Debug, Clone, PartialEq)]
pub enum EndpointProblem {
    /// Nothing to find out, or the endpoint is not the cause.
    None,
    /// Still finding out.
    Pending,
    /// The endpoint's error.
    Error(String),
}

/// Whether to look for an endpoint problem: a finished run with failures and an endpoint.
pub fn checks_endpoint(run: &RunOut) -> bool {
    run.status == RunStatus::Completed && run.failed > 0 && run.endpoint_id.is_some()
}

/// The endpoint's error when it caused a finished run's failures: its last check failed, or the
/// first failed sample was refused or could not connect and no check has passed since.
pub fn endpoint_problem(run: &RunOut, endpoint: Option<&Option<EndpointOut>>, first_failed: Option<&Option<String>>) -> EndpointProblem {
    if !checks_endpoint(run) {
        return EndpointProblem::None;
    }
    let ep = endpoint.and_then(|e| e.as_ref());
    if let Some(ep) = ep
        && let Some(t) = trouble(ep)
    {
        let error = ep.last_error.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| format!("{t} at last check."));
        return EndpointProblem::Error(error);
    }
    let (Some(_), Some(failed)) = (endpoint, first_failed) else { return EndpointProblem::Pending };
    let Some(error) = failed.as_ref().filter(|e| !e.is_empty() && is_endpoint_problem(e)) else { return EndpointProblem::None };
    let since = run.finished_at.unwrap_or(run.created_at);
    let fixed_since = ep.is_some_and(|ep| ep.last_ok == Some(true) && ep.last_checked_at.is_some_and(|t| t > since));
    if fixed_since { EndpointProblem::None } else { EndpointProblem::Error(error.clone()) }
}

/// The notice above the facts: its words, its tone and its action.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// A plain muted line (a reason that is no problem).
    Plain(String),
    /// A card with a dot: red for a problem, coral for a pause; `fix` offers Edit endpoint (primary
    /// when `primary`) with the field to focus.
    Card { text: String, problem: bool, fix: Option<(i64, EditFocus)>, primary: bool },
}

/// Why the run stopped, and the way out when the endpoint is the cause (`RunNotice`).
pub fn run_notice(run: &RunOut, endpoint_error: Option<&str>) -> Option<Notice> {
    let primary = endpoint_error.is_some();
    let text = endpoint_error.map(str::to_owned).or_else(|| run.status_reason.clone()).filter(|t| !t.is_empty())?;
    let problem = primary || (run.status != RunStatus::Running && is_problem(&text));
    let paused = matches!(run.status, RunStatus::Paused | RunStatus::Pausing);
    let fix = run.endpoint_id.filter(|_| primary || (problem && is_endpoint_problem(&text))).map(|id| (id, edit_focus_for(&text)));
    if !problem && !paused {
        return Some(Notice::Plain(text));
    }
    Some(Notice::Card { text, problem, fix, primary })
}

/// Which button is the run's one primary action (`RunActions`): Resume on a paused run, Export
/// once a finished run has records, Retry failed when it has none; none while it runs, or when Edit
/// endpoint is the primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primary {
    Resume,
    Export,
    Retry,
}

/// What the run's action buttons show and do (`RunActions::buttons`): the fields of the run they
/// read, and no more. While a run goes its counts change every second; these change only when a
/// button would, so the buttons (and an open "…" menu) are not made again each second.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionFacts {
    pub id: i64,
    pub name: String,
    pub status: RunStatus,
    /// It has records to export.
    pub exportable: bool,
    /// Its failed samples where a button uses them: a finished run's Retry failed, a paused run's
    /// Resume (which retries them). 0 while it goes.
    pub failed: i64,
    pub primary: Option<Primary>,
}

impl ActionFacts {
    pub fn of(run: &RunOut, primary_elsewhere: bool) -> Self {
        let going = matches!(run.status, RunStatus::Running | RunStatus::Pausing);
        ActionFacts {
            id: run.id,
            name: run.name.clone(),
            status: run.status,
            exportable: run.done > 0,
            failed: if going { 0 } else { run.failed },
            primary: primary_action(run, primary_elsewhere),
        }
    }
}

pub fn primary_action(run: &RunOut, primary_elsewhere: bool) -> Option<Primary> {
    if primary_elsewhere {
        return None;
    }
    match run.status {
        RunStatus::Paused => Some(Primary::Resume),
        RunStatus::Completed if run.done > 0 => Some(Primary::Export),
        RunStatus::Completed if run.failed > 0 => Some(Primary::Retry),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: RunStatus) -> RunOut {
        serde_json::from_value(serde_json::json!({
            "id": 4, "name": "Distill gsm8k", "endpoint_id": 1, "endpoint_name": "Local vLLM", "model": "m",
            "prompt_set_id": 1, "prompt_set_name": "gsm8k", "params": {}, "concurrency": 4, "status": status.as_str(),
            "status_reason": null, "total": 400, "done": 120, "failed": 3, "truncated": 2, "pending": 270,
            "prompt_tokens": 0, "completion_tokens": 0, "reasoning_tokens": 0, "active_seconds": 12.5,
            "created_at": "2026-11-18T21:38:37Z", "started_at": null, "finished_at": "2026-11-18T22:00:00Z"
        }))
        .unwrap()
    }

    #[test]
    fn labels_and_words() {
        assert_eq!(sample_label(11, 0, 1), "#12");
        assert_eq!(sample_label(11, 1, 4), "#12.2");
        assert_eq!(finish_reason_text(Some("length")), Some(("Truncated (max tokens)".into(), true)));
        assert_eq!(finish_reason_text(Some("eos")), Some(("Stopped with reason “eos”.".into(), false)));
        assert_eq!(finish_reason_text(None), None);
        assert_eq!(clock(42_900), "0:42");
        assert_eq!(clock(3_725_000), "1:02:05");
        assert_eq!(clock(-5), "0:00");
        assert_eq!(role_label("tool"), "tool");
    }

    #[test]
    fn durations_split_into_figures_and_units() {
        assert_eq!(duration_parts(0.4), None);
        assert_eq!(duration_parts(42.4), Some(vec![(42, "s")]));
        assert_eq!(duration_parts(1878.0), Some(vec![(31, "min"), (18, "s")]));
        assert_eq!(duration_parts(8040.0), Some(vec![(2, "h"), (14, "min")]));
        assert_eq!(duration_parts(3.0 * 86400.0 + 4.0 * 3600.0), Some(vec![(3, "d"), (4, "h")]));
    }

    #[test]
    fn filters_queries_and_pages() {
        let r = run(RunStatus::Running);
        assert_eq!(SampleFilter::Failed.count(&r), 3);
        assert_eq!(SampleFilter::All.empty_text("x y"), "No samples match “x y”.");
        assert_eq!(SampleFilter::Failed.empty_text("x"), "No failed samples match “x”.");
        assert_eq!(SampleFilter::Pending.empty_text(""), "Nothing waiting.");
        assert_eq!(samples_query(SampleFilter::All, "", 1), "offset=0&limit=50");
        assert_eq!(samples_query(SampleFilter::Failed, "a b", 3), "status=failed&q=a+b&offset=100&limit=50");
        assert_eq!(page_span(2, 120), (51, 100, 3));
        assert_eq!(page_span(1, 0), (0, 0, 1));
        assert_eq!(poll_every(RunStatus::Paused), Some(10));
        assert_eq!(poll_every(RunStatus::Completed), None);
    }

    #[test]
    fn reasons_read_as_the_web_ui_reads_them() {
        assert!(is_problem("Paused after 10 failures in a row"));
        assert!(is_problem("The server answered 503"));
        assert!(is_problem("Couldn't connect to http://x"));
        assert!(is_problem("Request timed out"));
        assert!(!is_problem("Paused by you"));
        assert!(is_endpoint_problem("The server rejected the API key (401): User not found."));
        assert!(is_endpoint_problem("could not connect to the server"));
        assert!(!is_endpoint_problem("HTTP 4010"));
        assert!(!is_endpoint_problem("The server answered 500"));
        assert_eq!(edit_focus_for("Could not connect: connection refused"), EditFocus::Url);
        assert_eq!(edit_focus_for("The server rejected the API key (401)"), EditFocus::Key);
    }

    #[test]
    fn notices_pick_their_tone_and_action() {
        let mut r = run(RunStatus::Paused);
        r.status_reason = Some("Paused after 10 failures in a row: The server rejected the API key (401)".into());
        match run_notice(&r, None) {
            Some(Notice::Card { problem: true, fix: Some((1, EditFocus::Key)), primary: false, .. }) => {}
            other => panic!("{other:?}"),
        }
        r.status_reason = Some("Paused by you".into());
        assert!(matches!(run_notice(&r, None), Some(Notice::Card { problem: false, fix: None, .. })));
        let mut done = run(RunStatus::Completed);
        done.status_reason = Some("Finished".into());
        assert_eq!(run_notice(&done, None), Some(Notice::Plain("Finished".into())));
        assert!(matches!(run_notice(&done, Some("Connection refused")), Some(Notice::Card { primary: true, fix: Some((1, _)), .. })));
        done.status_reason = None;
        assert_eq!(run_notice(&done, None), None);
    }

    #[test]
    fn the_endpoint_is_blamed_only_when_it_is_the_cause() {
        let r = run(RunStatus::Completed);
        assert_eq!(endpoint_problem(&r, None, None), EndpointProblem::Pending);
        // No check failed, the first failure is a refused connection, nothing passed since.
        let ep: EndpointOut = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "x", "base_url": "http://x", "location": "local", "has_api_key": false, "api_key_hint": null,
            "headers": {}, "default_concurrency": 16, "models": [], "last_ok": null, "last_error": null,
            "last_checked_at": null, "created_at": "2026-11-18T21:00:00Z", "run_count": 1
        }))
        .unwrap();
        let refused = Some("Connection refused".to_owned());
        assert_eq!(endpoint_problem(&r, Some(&Some(ep.clone())), Some(&refused)), EndpointProblem::Error("Connection refused".into()));
        assert_eq!(endpoint_problem(&r, Some(&Some(ep.clone())), Some(&Some("bad JSON".into()))), EndpointProblem::None);
        let mut fixed = ep.clone();
        fixed.last_ok = Some(true);
        fixed.last_checked_at = Some("2026-11-19T00:00:00Z".parse().unwrap());
        assert_eq!(endpoint_problem(&r, Some(&Some(fixed)), Some(&refused)), EndpointProblem::None);
        let mut failing = ep;
        failing.last_ok = Some(false);
        failing.last_error = Some("The server rejected the API key (401)".into());
        assert_eq!(
            endpoint_problem(&r, Some(&Some(failing)), None),
            EndpointProblem::Error("The server rejected the API key (401)".into())
        );
        assert_eq!(endpoint_problem(&run(RunStatus::Running), None, None), EndpointProblem::None);
    }

    #[test]
    fn the_actions_change_only_with_what_their_buttons_show() {
        let mut going = run(RunStatus::Running);
        let before = ActionFacts::of(&going, false);
        // A second later: more done, more failed, more tokens.
        going.done += 40;
        going.failed += 2;
        going.completion_tokens += 9000;
        going.active_seconds += 1.0;
        assert_eq!(ActionFacts::of(&going, false), before);
        // What a button shows: the status, the first record, a rename.
        let mut paused = going.clone();
        paused.status = RunStatus::Paused;
        assert_ne!(ActionFacts::of(&paused, false), before);
        assert_eq!(ActionFacts::of(&paused, false).failed, 5);
        let mut first = run(RunStatus::Running);
        first.done = 0;
        assert_ne!(ActionFacts::of(&first, false), before);
        let mut renamed = going.clone();
        renamed.name = "Distill gsm8k again".into();
        assert_ne!(ActionFacts::of(&renamed, false), before);
    }

    #[test]
    fn the_primary_action_follows_the_state() {
        assert_eq!(primary_action(&run(RunStatus::Paused), false), Some(Primary::Resume));
        assert_eq!(primary_action(&run(RunStatus::Completed), false), Some(Primary::Export));
        assert_eq!(primary_action(&run(RunStatus::Completed), true), None);
        let mut none_done = run(RunStatus::Completed);
        none_done.done = 0;
        assert_eq!(primary_action(&none_done, false), Some(Primary::Retry));
        assert_eq!(primary_action(&run(RunStatus::Running), false), None);
    }
}
