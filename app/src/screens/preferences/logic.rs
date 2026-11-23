//! The preference screens' pure logic, ported from frontend/src/pages/preference-review.tsx,
//! preferences-list.tsx and components/preferences/ (queries.ts, candidate-card.tsx,
//! prompt-view.tsx, new-set-dialog.tsx, auto-pair-dialog.tsx): letters, the queue's filters, the
//! chosen and rejected marks, how a thinking trace is counted, what the status line says, the run
//! labels and the new set's suggested name. No UI here, so all of it is unit tested.

use std::collections::HashSet;

use serde_json::Value;

use crate::api::types::{Candidate, PreferenceItem, PreferenceSetOut, RunOut, RunRef};
use crate::format::{count, int};

/// "A", "B", … "Z", "AA". The letter is how a candidate is named on screen and by its key.
pub fn letter(n: usize) -> String {
    let mut s = Vec::new();
    let mut i = n as i64;
    loop {
        s.push((b'A' + (i % 26) as u8) as char);
        i = i / 26 - 1;
        if i < 0 {
            break;
        }
    }
    s.iter().rev().collect()
}

/// Drops the organisation prefix of a model id: "Qwen/Qwen3-32B" gives "Qwen3-32B".
pub fn short_model(model: &str) -> &str {
    model.rfind('/').map_or(model, |slash| &model[slash + 1..])
}

// ------------------------------------------------------------------------------------ the queue

/// Which prompts the review walks through (the line tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Filter {
    #[default]
    Undecided,
    Decided,
    All,
}

impl Filter {
    pub const ALL: [Filter; 3] = [Filter::Undecided, Filter::Decided, Filter::All];

    /// The tab's label.
    pub fn label(self) -> &'static str {
        match self {
            Filter::Undecided => "Undecided",
            Filter::Decided => "Decided",
            Filter::All => "All",
        }
    }

    /// The noun after "3 of 12" in the action bar.
    pub fn noun(self) -> &'static str {
        match self {
            Filter::Undecided => "undecided",
            Filter::Decided => "decided",
            Filter::All => "prompts",
        }
    }

    /// The API's `filter` parameter.
    pub fn param(self) -> &'static str {
        match self {
            Filter::Undecided => "undecided",
            Filter::Decided => "decided",
            Filter::All => "all",
        }
    }

    /// The tab's count for `set`.
    pub fn count(self, set: &PreferenceSetOut) -> i64 {
        match self {
            Filter::Undecided => (set.prompt_count - set.decided).max(0),
            Filter::Decided => set.decided,
            Filter::All => set.prompt_count,
        }
    }
}

/// Whether `item` belongs to `filter`'s queue (a decided prompt leaves the undecided queue).
pub fn matches_filter(item: &PreferenceItem, filter: Filter) -> bool {
    match filter {
        Filter::All => true,
        Filter::Decided => item.chosen_sample_id.is_some(),
        Filter::Undecided => item.chosen_sample_id.is_none(),
    }
}

/// Where J or K (or the arrows, or the bar's chevrons) go from position `i` of `total`: the next
/// or previous position, or `None` at an end. When the shown prompt has left the queue (`gone`),
/// the next one has moved up into its place, so "next" stays at `i`.
pub fn step(i: i64, total: i64, gone: bool, forward: bool) -> Option<i64> {
    let target = if forward {
        if gone { i } else { i + 1 }
    } else {
        i - 1
    };
    if target < 0 || (!gone && target >= total) {
        return None;
    }
    Some(target)
}

/// A position past the end of a queue of `total` (after the last prompt left it, or a stale
/// position): the undecided queue wraps to its start, the others go to their last prompt.
pub fn past_end(filter: Filter, total: i64) -> i64 {
    if filter == Filter::Undecided { 0 } else { (total - 1).max(0) }
}

/// How far from the shown prompt a prompt read stays kept: the neighbours read ahead (one each
/// way) and a few more, so J and K back and forth stay instant.
pub const CACHE_REACH: i64 = 3;

/// Whether the prompt read at `entry` (its queue and position) stays kept while `at` shows: the
/// same queue, at most [`CACHE_REACH`] away. The rest is read again when it is next shown.
pub fn keep_cached(entry: (Filter, i64), at: (Filter, i64)) -> bool {
    entry.0 == at.0 && (entry.1 - at.1).abs() <= CACHE_REACH
}

/// "3 of 12 undecided" in the action bar (the position is never shown past the total, nor below 1).
pub fn position_text(i: i64, total: i64, filter: Filter) -> String {
    format!("{} of {} {}", int((i + 1).min(total.max(1))), int(total), filter.noun())
}

// ------------------------------------------------------------------------------------ marks

/// A response's mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    Chosen,
    Rejected,
}

impl Mark {
    pub fn other(self) -> Mark {
        match self {
            Mark::Chosen => Mark::Rejected,
            Mark::Rejected => Mark::Chosen,
        }
    }
}

/// The chosen and rejected responses of one prompt, by sample id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sel {
    pub chosen: Option<i64>,
    pub rejected: Option<i64>,
}

impl Sel {
    /// The pair saved on `item`.
    pub fn of(item: &PreferenceItem) -> Sel {
        Sel { chosen: item.chosen_sample_id, rejected: item.rejected_sample_id }
    }

    pub fn get(&self, mark: Mark) -> Option<i64> {
        match mark {
            Mark::Chosen => self.chosen,
            Mark::Rejected => self.rejected,
        }
    }

    fn set(&mut self, mark: Mark, id: Option<i64>) {
        match mark {
            Mark::Chosen => self.chosen = id,
            Mark::Rejected => self.rejected = id,
        }
    }

    /// Both marks are set: a pair to save.
    pub fn complete(&self) -> bool {
        self.chosen.is_some() && self.rejected.is_some()
    }

    /// Neither mark is set.
    pub fn is_empty(&self) -> bool {
        self.chosen.is_none() && self.rejected.is_none()
    }

    /// The mark of sample `id`.
    pub fn mark_of(&self, id: i64) -> Option<Mark> {
        if self.chosen == Some(id) {
            Some(Mark::Chosen)
        } else if self.rejected == Some(id) {
            Some(Mark::Rejected)
        } else {
            None
        }
    }
}

/// Radio semantics across the cards: one chosen and one rejected per prompt. Marking the response
/// that holds the other mark swaps the two; with exactly two responses the other one takes the
/// opposite mark at once. Returns `sel` itself when nothing changes.
pub fn next_selection(sel: Sel, mark: Mark, id: i64, candidates: &[Candidate]) -> Sel {
    let other = mark.other();
    if sel.get(mark) == Some(id) && (candidates.len() != 2 || sel.get(other).is_some()) {
        return sel;
    }
    let mut next = sel;
    if next.get(other) == Some(id) {
        next.set(other, sel.get(mark));
    }
    next.set(mark, Some(id));
    if candidates.len() == 2 {
        next.set(other, candidates.iter().find(|c| c.sample_id != id).map(|c| c.sample_id));
    }
    next
}

// ------------------------------------------------------------------------------------ cards

/// How a thinking trace's length is counted. The page picks one unit for all the cards it shows:
/// tokens when the server reported them for every trace, otherwise words for all of them, so the
/// cards can be compared at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceUnit {
    Tokens,
    Words,
}

pub fn trace_unit_for(candidates: &[Candidate]) -> TraceUnit {
    if candidates.iter().all(|c| c.reasoning.is_none() || c.reasoning_tokens.is_some()) {
        TraceUnit::Tokens
    } else {
        TraceUnit::Words
    }
}

/// Words, as the web UI counts them (runs of non-space).
pub fn word_count(s: &str) -> i64 {
    s.split_whitespace().count() as i64
}

/// "1,204 tokens" or "312 words".
pub fn trace_size(c: &Candidate, unit: TraceUnit) -> String {
    match (unit, c.reasoning_tokens) {
        (TraceUnit::Tokens, Some(n)) => count(n, "token", "tokens"),
        _ => count(word_count(c.reasoning.as_deref().unwrap_or_default()), "word", "words"),
    }
}

/// The runs that gave more than one of `candidates`: their cards name the sample, not the run.
pub fn runs_with_several(candidates: &[Candidate]) -> HashSet<i64> {
    let mut seen = HashSet::new();
    let mut several = HashSet::new();
    for c in candidates {
        if !seen.insert(c.run_id) {
            several.insert(c.run_id);
        }
    }
    several
}

/// Whether the set compares more than one model (cards then name the model).
pub fn compares_models(set: &PreferenceSetOut) -> bool {
    let models: HashSet<&str> = set.runs.iter().map(|r| r.model.as_str()).collect();
    models.len() > 1
}

/// What a card's header calls its response, and the line under it: only what tells the cards
/// apart (the model when the set compares several, else the sample, else the run), and the sample
/// under the model when one run gave several.
pub fn card_identity(c: &Candidate, show_model: bool, show_sample: bool) -> (String, Option<String>) {
    let sample = format!("Sample {}", int(c.sample_idx + 1));
    let identity = if show_model {
        c.model.clone()
    } else if show_sample {
        sample.clone()
    } else {
        c.run_name.clone()
    };
    (identity, (show_model && show_sample).then_some(sample))
}

// ------------------------------------------------------------------------------------ the prompt

/// A message's role as its data tag.
pub fn role_label(role: &str) -> String {
    match role {
        "system" => "System".into(),
        "user" => "User".into(),
        "assistant" => "Assistant".into(),
        other => other.into(),
    }
}

/// A message's text: a string as it is; OpenAI content parts by their text, the others named
/// ("[image_url]"); anything else as indented JSON.
pub fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|part| match part.get("text") {
                Some(Value::String(t)) => t.clone(),
                Some(other) if part.is_object() => other.to_string(),
                _ => format!("[{}]", part.get("type").and_then(Value::as_str).unwrap_or("part")),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// The (role, text) of each message of a prompt.
pub fn messages(item: &PreferenceItem) -> Vec<(String, String)> {
    item.messages
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(Value::as_str).unwrap_or_default().to_owned();
            let text = content_text(m.get("content").unwrap_or(&Value::Null));
            (role, text)
        })
        .collect()
}

/// Characters before a prompt is shown folded.
pub const LONG_PROMPT: usize = 1400;

/// Whether the prompt is long enough to fold (its text, in UTF-16 units as the web UI counts).
pub fn is_long_prompt(messages: &[(String, String)]) -> bool {
    messages.iter().map(|(_, t)| t.encode_utf16().count()).sum::<usize>() > LONG_PROMPT
}

// ------------------------------------------------------------------------------------ status line

/// What a save or a clear is doing for one prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Saving,
    Clearing,
    Saved,
    Cleared,
    /// It failed: a save (`clear` false) or a clear, with the server's message.
    Error { clear: bool, message: String },
}

/// The status line at the start of the action bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusLine {
    None,
    Error { clear: bool, message: String },
    Saving,
    Clearing,
    Cleared,
    /// "A chosen over B", with the Saved pill, and ", paired by auto-pair" for a rule's pair.
    Pair { chosen: String, rejected: String, saved: bool, by_rule: bool },
    /// "A is chosen. Pick a rejected response."
    ChosenOnly(String),
    /// "B is rejected. Pick a chosen response."
    RejectedOnly(String),
}

/// The status line for `item` shown with `sel` while `status` holds for it.
pub fn status_line(item: &PreferenceItem, sel: Sel, status: Option<&SaveState>) -> StatusLine {
    let name = |id: Option<i64>| {
        item.candidates.iter().position(|c| Some(c.sample_id) == id).map(letter).unwrap_or_else(|| "?".into())
    };
    match status {
        Some(SaveState::Error { clear, message }) => return StatusLine::Error { clear: *clear, message: message.clone() },
        Some(SaveState::Saving) => return StatusLine::Saving,
        Some(SaveState::Clearing) => return StatusLine::Clearing,
        Some(SaveState::Cleared) => return StatusLine::Cleared,
        _ => {}
    }
    match (sel.chosen, sel.rejected) {
        (Some(_), Some(_)) => StatusLine::Pair {
            chosen: name(sel.chosen),
            rejected: name(sel.rejected),
            saved: status == Some(&SaveState::Saved),
            by_rule: item.source.as_deref() == Some("rule") && sel == Sel::of(item),
        },
        (Some(_), None) => StatusLine::ChosenOnly(name(sel.chosen)),
        (None, Some(_)) => StatusLine::RejectedOnly(name(sel.rejected)),
        (None, None) => StatusLine::None,
    }
}

/// The error sentence of a failed save or clear.
pub fn error_sentence(clear: bool, message: &str) -> String {
    let what = if clear { "Couldn't clear this pair." } else { "Couldn't save this pair." };
    format!("{what} {message} Your choice is kept here.")
}

/// The last key of "1–N" in the shortcuts dialog: the number of responses, 2 to 9.
pub fn last_key(candidates: usize) -> String {
    candidates.clamp(2, 9).to_string()
}

// ------------------------------------------------------------------------------------ the set

/// A run's chip on the review: its model, unless two runs share a model or the run was named by
/// hand; and whether that label is the model (set in mono).
pub fn chip_label(run: &RunRef, set: &PreferenceSetOut) -> (String, bool) {
    let default_name = run.name == format!("{} on {}", run.model, set.prompt_set_name);
    let shared = set.runs.iter().filter(|o| o.model == run.model).count() > 1;
    let name = if default_name && !shared { run.model.clone() } else { run.name.clone() };
    let mono = name == run.model;
    (name, mono)
}

/// A run in the auto-pair pop-up buttons: "Distill gsm8k (Qwen3-32B)".
pub fn auto_pair_label(run: &RunRef) -> String {
    format!("{} ({})", run.name, short_model(&run.model))
}

/// What the empty review says, and which action it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Empty {
    /// No prompt has two finished responses yet: Open runs.
    NothingToCompare,
    /// Nothing decided yet: Show undecided.
    NoneDecided,
    /// Every prompt is decided: Review decided prompts.
    AllDecided,
}

pub fn empty_kind(set: &PreferenceSetOut, filter: Filter) -> Empty {
    if set.prompt_count == 0 || filter == Filter::All {
        Empty::NothingToCompare
    } else if filter == Filter::Decided {
        Empty::NoneDecided
    } else {
        Empty::AllDecided
    }
}

/// "The one prompt is decided." or "All 12 prompts are decided."
pub fn all_decided_text(prompts: i64) -> String {
    if prompts == 1 { "The one prompt is decided.".into() } else { format!("All {} prompts are decided.", int(prompts)) }
}

/// The toast after auto-pair: `None` is "No new pairs" (a plain notice).
pub fn auto_pair_message(created: i64, skipped: i64) -> Option<String> {
    if created == 0 {
        return None;
    }
    let kept = if skipped > 0 { format!(" Kept {} picked by hand.", count(skipped, "pair", "pairs")) } else { String::new() };
    Some(format!("Paired {}.{kept}", count(created, "prompt", "prompts")))
}

// ------------------------------------------------------------------------------------ new set

/// The runs on one prompt set, in the new set dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub prompt_set_id: i64,
    pub prompt_set_name: String,
    pub runs: Vec<RunOut>,
}

/// `runs` grouped by prompt set. Runs come newest first, so groups are ordered by their newest run.
pub fn group_runs(runs: &[RunOut]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for run in runs {
        match groups.iter_mut().find(|g| g.prompt_set_id == run.prompt_set_id) {
            Some(g) => g.runs.push(run.clone()),
            None => groups.push(Group {
                prompt_set_id: run.prompt_set_id,
                prompt_set_name: run.prompt_set_name.clone(),
                runs: vec![run.clone()],
            }),
        }
    }
    groups
}

/// The name the dialog suggests for `selected` runs: "Qwen3-32B samples" for one run, the models
/// joined by " vs " when they differ, else the run names.
pub fn suggest_name(selected: &[&RunOut]) -> String {
    match selected {
        [] => String::new(),
        [one] => format!("{} samples", short_model(&one.model)),
        _ => {
            let mut models: Vec<&str> = Vec::new();
            for r in selected {
                let m = short_model(&r.model);
                if !models.contains(&m) {
                    models.push(m);
                }
            }
            if models.len() > 1 {
                models.join(" vs ")
            } else {
                selected.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(" vs ")
            }
        }
    }
}

/// The new set's form errors before it is sent: (name, runs).
pub fn new_set_errors(name: &str, selected: &[&RunOut]) -> (Option<String>, Option<String>) {
    let name_error = name.trim().is_empty().then(|| "Enter a name for the preference set.".to_owned());
    let runs_error = match selected {
        [] => Some("Pick at least one run.".to_owned()),
        [one] if one.params.samples_per_prompt < 2 => {
            Some("This run has one response per prompt. Pick a second run on the same prompt set to compare.".to_owned())
        }
        _ => None,
    };
    (name_error, runs_error)
}

/// The line under the runs: what is selected, or the rule.
pub fn runs_hint(selected: &[&RunOut]) -> String {
    match selected.first() {
        Some(first) => format!("{} on {} selected.", count(selected.len() as i64, "run", "runs"), first.prompt_set_name),
        None => "Only runs on the same prompt set can be combined.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_keeps_the_prompts_near_the_shown_one() {
        let at = (Filter::Undecided, 10);
        assert!(keep_cached((Filter::Undecided, 10), at));
        assert!(keep_cached((Filter::Undecided, 11), at));
        assert!(keep_cached((Filter::Undecided, 7), at));
        assert!(keep_cached((Filter::Undecided, 13), at));
        assert!(!keep_cached((Filter::Undecided, 14), at));
        assert!(!keep_cached((Filter::Undecided, 0), at));
        // Another queue's positions name other prompts.
        assert!(!keep_cached((Filter::All, 10), at));
    }

    fn cand(sample_id: i64, run_id: i64) -> Candidate {
        Candidate {
            sample_id,
            run_id,
            run_name: format!("Run {run_id}"),
            model: format!("org/model-{run_id}"),
            sample_idx: 0,
            reasoning: None,
            response: Some("ok".into()),
            finish_reason: Some("stop".into()),
            completion_tokens: Some(3),
            reasoning_tokens: None,
        }
    }

    fn item(candidates: Vec<Candidate>, chosen: Option<i64>, rejected: Option<i64>) -> PreferenceItem {
        PreferenceItem {
            prompt_id: 7,
            prompt_idx: 6,
            messages: vec![serde_json::json!({"role": "user", "content": "2+2?"})],
            candidates,
            chosen_sample_id: chosen,
            rejected_sample_id: rejected,
            source: chosen.map(|_| "manual".to_owned()),
        }
    }

    fn set(runs: Vec<(&str, &str)>) -> PreferenceSetOut {
        PreferenceSetOut {
            id: 1,
            name: "Set".into(),
            prompt_set_id: 2,
            prompt_set_name: "gsm8k".into(),
            run_ids: (1..=runs.len() as i64).collect(),
            runs: runs
                .into_iter()
                .enumerate()
                .map(|(i, (name, model))| RunRef { id: i as i64 + 1, name: name.into(), model: model.into() })
                .collect(),
            prompt_count: 12,
            decided: 5,
            created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn letters_run_on_past_z() {
        assert_eq!(letter(0), "A");
        assert_eq!(letter(1), "B");
        assert_eq!(letter(25), "Z");
        assert_eq!(letter(26), "AA");
        assert_eq!(letter(27), "AB");
        assert_eq!(letter(701), "ZZ");
        assert_eq!(letter(702), "AAA");
    }

    #[test]
    fn short_models_drop_the_organisation() {
        assert_eq!(short_model("Qwen/Qwen3-32B"), "Qwen3-32B");
        assert_eq!(short_model("mock-reasoning"), "mock-reasoning");
        assert_eq!(short_model("a/b/c"), "c");
    }

    #[test]
    fn two_responses_take_opposite_marks_at_once() {
        let two = vec![cand(10, 1), cand(11, 2)];
        let s = next_selection(Sel::default(), Mark::Chosen, 10, &two);
        assert_eq!(s, Sel { chosen: Some(10), rejected: Some(11) });
        // Choosing the rejected one swaps them.
        let s = next_selection(s, Mark::Chosen, 11, &two);
        assert_eq!(s, Sel { chosen: Some(11), rejected: Some(10) });
        // Pressing the mark it already has changes nothing.
        assert_eq!(next_selection(s, Mark::Chosen, 11, &two), s);
        // Rejecting first works the same way.
        let s = next_selection(Sel::default(), Mark::Rejected, 10, &two);
        assert_eq!(s, Sel { chosen: Some(11), rejected: Some(10) });
    }

    #[test]
    fn three_responses_keep_one_of_each_mark() {
        let three = vec![cand(10, 1), cand(11, 1), cand(12, 2)];
        let s = next_selection(Sel::default(), Mark::Chosen, 10, &three);
        assert_eq!(s, Sel { chosen: Some(10), rejected: None });
        assert_eq!(next_selection(s, Mark::Chosen, 10, &three), s);
        let s = next_selection(s, Mark::Rejected, 12, &three);
        assert_eq!(s, Sel { chosen: Some(10), rejected: Some(12) });
        // Rejecting the chosen one swaps the marks.
        let s = next_selection(s, Mark::Rejected, 10, &three);
        assert_eq!(s, Sel { chosen: Some(12), rejected: Some(10) });
        // Choosing another moves the chosen mark only.
        let s = next_selection(s, Mark::Chosen, 11, &three);
        assert_eq!(s, Sel { chosen: Some(11), rejected: Some(10) });
        assert_eq!(s.mark_of(10), Some(Mark::Rejected));
        assert_eq!(s.mark_of(12), None);
    }

    #[test]
    fn a_half_pair_with_two_responses_completes_on_the_same_press() {
        // A draft with only the chosen mark (left from a three-response layout) and two responses:
        // pressing the chosen one again fills the other side.
        let two = vec![cand(10, 1), cand(11, 2)];
        let s = Sel { chosen: Some(10), rejected: None };
        assert_eq!(next_selection(s, Mark::Chosen, 10, &two), Sel { chosen: Some(10), rejected: Some(11) });
    }

    #[test]
    fn steps_through_the_queue() {
        assert_eq!(step(0, 5, false, true), Some(1));
        assert_eq!(step(4, 5, false, true), None);
        assert_eq!(step(0, 5, false, false), None);
        assert_eq!(step(3, 5, false, false), Some(2));
        // The shown prompt left the queue: the next one is at the same position.
        assert_eq!(step(4, 5, true, true), Some(4));
        assert_eq!(step(0, 1, true, true), Some(0));
        assert_eq!(past_end(Filter::Undecided, 4), 0);
        assert_eq!(past_end(Filter::Decided, 4), 3);
        assert_eq!(past_end(Filter::All, 0), 0);
        assert_eq!(position_text(2, 12, Filter::Undecided), "3 of 12 undecided");
        assert_eq!(position_text(0, 0, Filter::All), "1 of 0 prompts");
        assert_eq!(position_text(9, 3, Filter::Decided), "3 of 3 decided");
    }

    #[test]
    fn filters_and_counts() {
        let decided = item(vec![cand(10, 1), cand(11, 2)], Some(10), Some(11));
        let open = item(vec![cand(10, 1), cand(11, 2)], None, None);
        assert!(matches_filter(&decided, Filter::Decided));
        assert!(!matches_filter(&decided, Filter::Undecided));
        assert!(matches_filter(&open, Filter::Undecided));
        assert!(matches_filter(&open, Filter::All));
        let s = set(vec![("a", "m1")]);
        assert_eq!(Filter::Undecided.count(&s), 7);
        assert_eq!(Filter::Decided.count(&s), 5);
        assert_eq!(Filter::All.count(&s), 12);
        assert_eq!(Filter::All.param(), "all");
    }

    #[test]
    fn traces_count_tokens_only_when_every_trace_has_them() {
        let mut a = cand(1, 1);
        a.reasoning = Some("one two  three\nfour".into());
        a.reasoning_tokens = Some(1204);
        let mut b = cand(2, 2);
        b.reasoning = Some("x y".into());
        let c = cand(3, 2); // no trace at all
        assert_eq!(trace_unit_for(&[a.clone(), c.clone()]), TraceUnit::Tokens);
        assert_eq!(trace_unit_for(&[a.clone(), b.clone()]), TraceUnit::Words);
        assert_eq!(trace_size(&a, TraceUnit::Tokens), "1,204 tokens");
        assert_eq!(trace_size(&a, TraceUnit::Words), "4 words");
        assert_eq!(trace_size(&b, TraceUnit::Tokens), "2 words");
        let mut one = cand(4, 1);
        one.reasoning = Some("  single ".into());
        assert_eq!(trace_size(&one, TraceUnit::Words), "1 word");
        assert_eq!(word_count("   "), 0);
    }

    #[test]
    fn cards_name_what_tells_them_apart() {
        let mut c = cand(1, 4);
        c.sample_idx = 2;
        assert_eq!(card_identity(&c, true, true), ("org/model-4".into(), Some("Sample 3".into())));
        assert_eq!(card_identity(&c, true, false), ("org/model-4".into(), None));
        assert_eq!(card_identity(&c, false, true), ("Sample 3".into(), None));
        assert_eq!(card_identity(&c, false, false), ("Run 4".into(), None));
        let several = runs_with_several(&[cand(1, 1), cand(2, 1), cand(3, 2)]);
        assert!(several.contains(&1) && !several.contains(&2));
        assert!(compares_models(&set(vec![("a", "m1"), ("b", "m2")])));
        assert!(!compares_models(&set(vec![("a", "m1"), ("b", "m1")])));
    }

    #[test]
    fn prompt_text_from_strings_parts_and_json() {
        assert_eq!(content_text(&serde_json::json!("Hi")), "Hi");
        let parts = serde_json::json!([{"type": "text", "text": "Look"}, {"type": "image_url", "image_url": {}}, 3]);
        assert_eq!(content_text(&parts), "Look\n[image_url]\n[part]");
        assert_eq!(content_text(&serde_json::json!({"a": 1})), "{\n  \"a\": 1\n}");
        let it = item(vec![], None, None);
        assert_eq!(messages(&it), vec![("user".to_owned(), "2+2?".to_owned())]);
        assert_eq!(role_label("system"), "System");
        assert_eq!(role_label("tool"), "tool");
        assert!(!is_long_prompt(&[("user".into(), "x".repeat(1400))]));
        assert!(is_long_prompt(&[("user".into(), "x".repeat(1000)), ("system".into(), "y".repeat(401))]));
    }

    #[test]
    fn the_status_line_says_what_the_marks_are() {
        let cands = vec![cand(10, 1), cand(11, 2), cand(12, 2)];
        let it = item(cands.clone(), None, None);
        assert_eq!(status_line(&it, Sel::default(), None), StatusLine::None);
        assert_eq!(status_line(&it, Sel { chosen: Some(11), rejected: None }, None), StatusLine::ChosenOnly("B".into()));
        assert_eq!(status_line(&it, Sel { chosen: None, rejected: Some(12) }, None), StatusLine::RejectedOnly("C".into()));
        let pair = Sel { chosen: Some(12), rejected: Some(10) };
        assert_eq!(
            status_line(&it, pair, Some(&SaveState::Saved)),
            StatusLine::Pair { chosen: "C".into(), rejected: "A".into(), saved: true, by_rule: false }
        );
        assert_eq!(status_line(&it, pair, Some(&SaveState::Saving)), StatusLine::Saving);
        let failed = SaveState::Error { clear: true, message: "Not found.".into() };
        assert_eq!(status_line(&it, pair, Some(&failed)), StatusLine::Error { clear: true, message: "Not found.".into() });
        assert_eq!(error_sentence(false, "Boom."), "Couldn't save this pair. Boom. Your choice is kept here.");
        // A pair auto-pair made, shown as saved.
        let mut ruled = item(cands, Some(10), Some(11));
        ruled.source = Some("rule".into());
        assert_eq!(
            status_line(&ruled, Sel::of(&ruled), None),
            StatusLine::Pair { chosen: "A".into(), rejected: "B".into(), saved: false, by_rule: true }
        );
        // Changed by hand, it is no longer the rule's.
        let changed = Sel { chosen: Some(11), rejected: Some(10) };
        assert!(matches!(status_line(&ruled, changed, None), StatusLine::Pair { by_rule: false, .. }));
        assert_eq!(last_key(2), "2");
        assert_eq!(last_key(1), "2");
        assert_eq!(last_key(14), "9");
    }

    #[test]
    fn run_chips_and_labels() {
        let s = set(vec![("qwen3 on gsm8k", "qwen3"), ("Hand named", "llama"), ("llama on gsm8k", "llama")]);
        assert_eq!(chip_label(&s.runs[0], &s), ("qwen3".into(), true));
        assert_eq!(chip_label(&s.runs[1], &s), ("Hand named".into(), false));
        // Two runs share the model: the run's own name.
        assert_eq!(chip_label(&s.runs[2], &s), ("llama on gsm8k".into(), false));
        let r = RunRef { id: 1, name: "Distill".into(), model: "Qwen/Qwen3-32B".into() };
        assert_eq!(auto_pair_label(&r), "Distill (Qwen3-32B)");
    }

    #[test]
    fn empty_reviews_and_toasts() {
        let mut s = set(vec![("a", "m")]);
        assert_eq!(empty_kind(&s, Filter::Undecided), Empty::AllDecided);
        assert_eq!(empty_kind(&s, Filter::Decided), Empty::NoneDecided);
        assert_eq!(empty_kind(&s, Filter::All), Empty::NothingToCompare);
        s.prompt_count = 0;
        assert_eq!(empty_kind(&s, Filter::Decided), Empty::NothingToCompare);
        assert_eq!(all_decided_text(1), "The one prompt is decided.");
        assert_eq!(all_decided_text(1200), "All 1,200 prompts are decided.");
        assert_eq!(auto_pair_message(0, 3), None);
        assert_eq!(auto_pair_message(1, 0).as_deref(), Some("Paired 1 prompt."));
        assert_eq!(auto_pair_message(40, 2).as_deref(), Some("Paired 40 prompts. Kept 2 pairs picked by hand."));
    }

    fn run(id: i64, set_id: i64, model: &str, samples: i64) -> RunOut {
        let json = serde_json::json!({
            "id": id, "name": format!("Run {id}"), "endpoint_id": 1, "endpoint_name": "Local",
            "model": model, "prompt_set_id": set_id, "prompt_set_name": format!("Set {set_id}"),
            "params": {"samples_per_prompt": samples}, "concurrency": 4, "status": "completed",
            "status_reason": null, "total": 10, "done": 10, "failed": 0, "truncated": 0, "pending": 0,
            "prompt_tokens": 0, "completion_tokens": 0, "reasoning_tokens": 0, "active_seconds": 1.0,
            "created_at": "2026-11-18T21:38:37Z", "started_at": null, "finished_at": null
        });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn new_sets_group_runs_and_suggest_a_name() {
        let runs = vec![run(5, 2, "Qwen/Qwen3-32B", 1), run(4, 1, "llama", 2), run(3, 2, "meta/llama", 1)];
        let groups = group_runs(&runs);
        assert_eq!(groups.iter().map(|g| g.prompt_set_id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(groups[0].runs.iter().map(|r| r.id).collect::<Vec<_>>(), vec![5, 3]);
        assert_eq!(suggest_name(&[]), "");
        assert_eq!(suggest_name(&[&runs[0]]), "Qwen3-32B samples");
        assert_eq!(suggest_name(&[&runs[0], &runs[2]]), "Qwen3-32B vs llama");
        let same = run(6, 2, "x/Qwen3-32B", 1);
        assert_eq!(suggest_name(&[&runs[0], &same]), "Run 5 vs Run 6");
        assert_eq!(runs_hint(&[]), "Only runs on the same prompt set can be combined.");
        assert_eq!(runs_hint(&[&runs[0], &runs[2]]), "2 runs on Set 2 selected.");
    }

    #[test]
    fn new_sets_check_the_name_and_runs() {
        let one = run(1, 1, "m", 1);
        let two = run(2, 1, "m", 2);
        assert_eq!(new_set_errors(" ", &[]), (Some("Enter a name for the preference set.".into()), Some("Pick at least one run.".into())));
        assert_eq!(
            new_set_errors("x", &[&one]).1.as_deref(),
            Some("This run has one response per prompt. Pick a second run on the same prompt set to compare.")
        );
        assert_eq!(new_set_errors("x", &[&two]), (None, None));
        assert_eq!(new_set_errors("x", &[&one, &two]), (None, None));
    }
}
