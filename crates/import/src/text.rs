//! Split plain text and Markdown into prompts (a port of the Python `split_text`).

use std::sync::LazyLock;

use regex::Regex;
use stellarator_core::schemas::SplitMode;

// Python used a backreference (`([-*_])(?:[ \t]*\1){2,}`); the regex crate has none, so each
// character gets its own branch.
static SEP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ {0,3}(?:-(?:[ \t]*-){2,}|\*(?:[ \t]*\*){2,}|_(?:[ \t]*_){2,})[ \t]*$").expect("regex")
});
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}(#{1,6})[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$").expect("regex"));
static ITEM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:[-*+]|\d{1,9}[.)])[ \t]+(.*)$").expect("regex"));
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ {0,3}(```|~~~)").expect("regex"));
static FRONT_MATTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\A---[ \t]*\n(.*?)\n(?:---|\.\.\.)[ \t]*(?:\n|\z)").expect("regex"));
static FRONT_KEY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[\w.-]+[ \t]*:").expect("regex"));
static BLANK_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]*\n").expect("regex"));

/// YAML front matter is dropped when it has at least one `key:` line.
pub fn strip_front_matter(text: &str) -> &str {
    if let Some(c) = FRONT_MATTER_RE.captures(text) {
        let body = c.get(1).map_or("", |m| m.as_str());
        if FRONT_KEY_RE.is_match(body) {
            return &text[c.get(0).map_or(0, |m| m.end())..];
        }
    }
    text
}

/// (line, inside a code fence). Structural markers inside fences are ignored.
fn fenced_lines(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for line in text.split('\n') {
        let m = FENCE_RE.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str());
        match fence {
            None => {
                if let Some(f) = m {
                    fence = Some(f);
                    out.push((line, true));
                } else {
                    out.push((line, false));
                }
            }
            Some(open) => {
                out.push((line, true));
                if m == Some(open) {
                    fence = None;
                }
            }
        }
    }
    out
}

fn clean<I: IntoIterator<Item = S>, S: AsRef<str>>(pieces: I) -> Vec<String> {
    pieces
        .into_iter()
        .filter_map(|p| {
            let t = p.as_ref().trim();
            (!t.is_empty()).then(|| t.to_string())
        })
        .collect()
}

fn split_separator(lines: &[(&str, bool)]) -> Vec<String> {
    let mut pieces: Vec<Vec<&str>> = vec![Vec::new()];
    for &(line, code) in lines {
        if !code && SEP_RE.is_match(line) {
            pieces.push(Vec::new());
        } else if let Some(last) = pieces.last_mut() {
            last.push(line);
        }
    }
    clean(pieces.iter().map(|p| p.join("\n")))
}

fn split_heading(lines: &[(&str, bool)]) -> Vec<String> {
    // (level, heading text or None for the preamble, body lines)
    let mut sections: Vec<(usize, Option<String>, Vec<&str>)> = vec![(0, None, Vec::new())];
    for &(line, code) in lines {
        let m = if code { None } else { HEADING_RE.captures(line) };
        match m {
            Some(c) => {
                let level = c.get(1).map_or(1, |m| m.as_str().len());
                let title = c.get(2).map_or("", |m| m.as_str()).trim().to_string();
                sections.push((level, Some(title), Vec::new()));
            }
            None => {
                if let Some(last) = sections.last_mut() {
                    last.2.push(line);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, (level, title, body)) in sections.iter().enumerate() {
        let body_text = body.join("\n");
        let body_text = body_text.trim();
        let Some(title) = title else {
            if !body_text.is_empty() {
                out.push(body_text.to_string());
            }
            continue;
        };
        if body_text.is_empty() {
            if let Some(next) = sections.get(i + 1) {
                if next.0 > *level {
                    continue; // a container heading with no text of its own
                }
            }
        }
        out.push(if body_text.is_empty() { title.clone() } else { format!("{title}\n\n{body_text}") });
    }
    clean(out)
}

fn dedent_continuation(lines: &[&str]) -> String {
    let indents: Vec<usize> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .collect();
    let cut = indents.iter().copied().min().unwrap_or(0);
    let joined: Vec<&str> = lines.iter().map(|l| if l.trim().is_empty() { "" } else { &l[cut.min(l.len())..] }).collect();
    joined.join("\n").trim_matches('\n').to_string()
}

fn split_list_items(lines: &[(&str, bool)]) -> Vec<String> {
    let mut items: Vec<Vec<&str>> = Vec::new();
    for &(line, code) in lines {
        let m = if code { None } else { ITEM_RE.captures(line) };
        if let Some(c) = m {
            items.push(vec![c.get(1).map_or("", |m| m.as_str())]);
        } else if let Some(last) = items.last_mut() {
            last.push(line);
        }
    }
    let out = items.iter().map(|item| {
        let rest = dedent_continuation(&item[1..]);
        if rest.trim().is_empty() {
            item[0].to_string()
        } else {
            format!("{}\n{}", item[0], rest)
        }
    });
    clean(out)
}

fn auto_mode(text: &str, lines: &[(&str, bool)], markdown: Option<bool>) -> SplitMode {
    let plain: Vec<&str> = lines.iter().filter(|(_, code)| !code).map(|(l, _)| *l).collect();
    if plain.iter().any(|l| SEP_RE.is_match(l)) {
        return SplitMode::Separator;
    }
    if markdown != Some(false) && plain.iter().filter(|l| HEADING_RE.is_match(l)).count() >= 2 {
        return SplitMode::Heading;
    }
    let nonblank: Vec<&str> = plain.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    let items = nonblank.iter().filter(|l| ITEM_RE.is_match(l)).count();
    if items >= 2 {
        let other = nonblank
            .iter()
            .filter(|l| !ITEM_RE.is_match(l) && !l.starts_with(' ') && !l.starts_with('\t'))
            .count();
        if other <= std::cmp::max(1, nonblank.len() / 10) {
            return SplitMode::ListItem;
        }
    }
    if clean(BLANK_SPLIT_RE.split(text)).len() >= 2 {
        return SplitMode::BlankLine;
    }
    let all: Vec<&str> = text.split('\n').filter(|l| !l.trim().is_empty()).collect();
    if all.len() >= 2 && all.iter().map(|l| l.chars().count()).max().unwrap_or(0) <= 500 {
        return SplitMode::Line;
    }
    SplitMode::Whole
}

/// Split text into prompt strings. Returns (pieces, the mode actually used).
///
/// `markdown`: Some(true) for Markdown files (front matter stripped, headings considered),
/// Some(false) for plain text (headings only with an explicit Heading mode), None for pasted text
/// (headings considered).
pub fn split_text(text: &str, mode: SplitMode, markdown: Option<bool>) -> (Vec<String>, SplitMode) {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let text: &str = if markdown == Some(true) { strip_front_matter(&text) } else { &text };
    let lines = fenced_lines(text);
    let mode = if mode == SplitMode::Auto { auto_mode(text, &lines, markdown) } else { mode };
    let pieces = match mode {
        SplitMode::Separator => split_separator(&lines),
        SplitMode::Heading => split_heading(&lines),
        SplitMode::ListItem => split_list_items(&lines),
        SplitMode::BlankLine => clean(BLANK_SPLIT_RE.split(text)),
        SplitMode::Line => clean(text.split('\n')),
        SplitMode::Whole | SplitMode::Auto => clean([text]),
    };
    (pieces, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use SplitMode::*;

    fn split(t: &str) -> (Vec<String>, SplitMode) {
        split_text(t, Auto, Some(false))
    }

    #[test]
    fn auto_separator() {
        let (p, m) = split("one\ntwo\n---\nthree\n***\nfour");
        assert_eq!(m, Separator);
        assert_eq!(p, ["one\ntwo", "three", "four"]);
    }

    #[test]
    fn heading_ignores_code_fences() {
        let (p, _) = split_text("# A\n```\n# not a heading\n```\n# B\nx", Heading, Some(true));
        assert_eq!(p.len(), 2);
        assert!(p[0].contains("# not a heading"));
    }

    #[test]
    fn auto_list_item_with_continuation() {
        let (p, m) = split("- first item\n  continues here\n- second\n1. third");
        assert_eq!(m, ListItem);
        assert_eq!(p, ["first item\ncontinues here", "second", "third"]);
    }

    #[test]
    fn auto_blank_line() {
        let (p, m) = split("Para one\nstill one.\n\nPara two.\n\n\nPara three.");
        assert_eq!(m, BlankLine);
        assert_eq!(p, ["Para one\nstill one.", "Para two.", "Para three."]);
    }

    #[test]
    fn auto_line_and_whole() {
        assert_eq!(split("a\nb\nc"), (vec!["a".to_string(), "b".into(), "c".into()], Line));
        let (p, m) = split("just one");
        assert_eq!(p, ["just one"]);
        assert!(m == Whole || m == Line);
        let long = format!("{}\n{}", "x".repeat(600), "y".repeat(600));
        assert_eq!(split(&long).1, Whole);
    }

    #[test]
    fn explicit_modes() {
        assert_eq!(split_text("a\n\nb\nc", Line, Some(false)).0, ["a", "b", "c"]);
        assert_eq!(split_text("a\n\nb", Whole, Some(false)).0, ["a\n\nb"]);
        assert_eq!(split_text("  a  \n\n\n  \n b ", BlankLine, Some(false)).0, ["a", "b"]);
        assert_eq!(split_text("- x\n- y", ListItem, Some(false)).0, ["x", "y"]);
        assert_eq!(split_text("# T\nbody\n# U\nmore", Heading, Some(false)).0, ["T\n\nbody", "U\n\nmore"]);
        assert_eq!(split_text("a\n___\nb", Separator, Some(false)).0, ["a", "b"]);
    }

    #[test]
    fn markdown_front_matter_and_container_headings() {
        let doc = "---\ntitle: Sample\ntags: [a]\n---\n\n# First\n\nBody one.\n\n## Sub\n\nBody sub.\n\n# Second\n\nBody two.\n";
        let (p, m) = split_text(doc, Auto, Some(true));
        assert_eq!(m, Heading);
        assert_eq!(p, ["First\n\nBody one.", "Sub\n\nBody sub.", "Second\n\nBody two."]);
        // A container heading with no body of its own is skipped.
        let (p, _) = split_text("# Part\n## A\nx\n## B\ny", Heading, Some(true));
        assert_eq!(p, ["A\n\nx", "B\n\ny"]);
        // Closing hashes are not part of the title; CRLF is normalized.
        let (p, _) = split_text("# One ##\r\nbody\r\n# Two\r\n", Heading, None);
        assert_eq!(p, ["One\n\nbody", "Two"]);
    }

    #[test]
    fn plain_text_headings_only_when_asked() {
        // Plain text: '#' lines are not headings in auto mode.
        let (_, m) = split_text("# a\n# b\n", Auto, Some(false));
        assert_eq!(m, Line);
        let (_, m) = split_text("# a\n# b\n", Auto, None);
        assert_eq!(m, Heading);
    }
}
