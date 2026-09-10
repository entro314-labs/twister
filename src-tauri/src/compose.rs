//! Writing posts: Markdown in, X-shaped text out, cut into a thread.
//!
//! X has no formatting, so `**bold**` and `*italic*` become the Unicode
//! mathematical letters that read as bold and italic everywhere X renders
//! text (and, fairly, nowhere a screen reader is happy — the composer says
//! so). Headings, lists and links are flattened to what survives. A `---`
//! line is a thread break; anything longer than the limit is cut at a
//! paragraph, then a sentence, then a word.
//!
//! The count is X's own rule: URLs weigh 23 whatever their length, most
//! characters weigh one, and everything outside the Latin, Greek, Cyrillic,
//! Hebrew and Arabic blocks — CJK, emoji — weighs two.

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

pub const LIMIT: usize = 280;
const URL_WEIGHT: usize = 23;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub text: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub parts: Vec<Part>,
    pub limit: usize,
}

/// Where a URL starts and ends in a run of text. X links anything that
/// looks like `scheme://host` or a bare `domain.tld/...` standing on its own
/// between spaces (an email address is not a link), and weighs it 23.
fn url_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let rest = &text[cursor..];
        let Some(offset) = rest.find(|c: char| !c.is_whitespace()) else {
            break;
        };
        let start = cursor + offset;
        let run_end = text[start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |o| start + o);
        let run = &text[start..run_end];
        let opened = run.len() - run.trim_start_matches(['(', '[']).len();
        let closed = run.len()
            - run
                .trim_end_matches([')', ']', '.', ',', '!', '?', ';', ':'])
                .len();
        if looks_like_url(&run[opened..]) {
            spans.push((start + opened, run_end - closed));
        }
        cursor = run_end;
    }
    spans
}

fn looks_like_url(run: &str) -> bool {
    let run = run.trim_end_matches(['.', ',', ')', '!', '?', ';', ':']);
    if let Some(rest) = run
        .strip_prefix("https://")
        .or_else(|| run.strip_prefix("http://"))
    {
        return rest.contains('.') && !rest.starts_with('.');
    }
    // bare domain: letters/digits/dashes, a dot, a 2+ letter tld, then maybe a path
    let host_end = run.find('/').unwrap_or(run.len());
    let host = &run[..host_end];
    let mut labels = host.rsplit('.');
    let Some(tld) = labels.next() else {
        return false;
    };
    let label_ok = |label: &str| {
        !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    tld.len() >= 2
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        && labels.clone().count() >= 1
        && labels.all(label_ok)
        && !host.starts_with('@')
}

fn weight(grapheme: &str) -> usize {
    let Some(first) = grapheme.chars().next() else {
        return 0;
    };
    let cp = first as u32;
    let light = matches!(cp, 0..=4351 | 8192..=8205 | 8208..=8223 | 8242..=8247);
    if light { 1 } else { 2 }
}

/// X's weighted length.
pub fn count(text: &str) -> usize {
    let spans = url_spans(text);
    let mut total = 0;
    let mut cursor = 0;
    for (start, end) in spans {
        total += text[cursor..start]
            .graphemes(true)
            .map(weight)
            .sum::<usize>();
        total += URL_WEIGHT;
        cursor = end;
    }
    total + text[cursor..].graphemes(true).map(weight).sum::<usize>()
}

// ─── Styling ────────────────────────────────────────────────────────────────

fn style_char(c: char, bold: bool, italic: bool) -> String {
    let (upper, lower, digit): (u32, u32, Option<u32>) = match (bold, italic) {
        (true, true) => (0x1D63C, 0x1D656, None),
        (true, false) => (0x1D5D4, 0x1D5EE, Some(0x1D7EC)),
        (false, true) => (0x1D608, 0x1D622, None),
        (false, false) => return c.to_string(),
    };
    let mapped = match c {
        'A'..='Z' => char::from_u32(upper + (c as u32 - 'A' as u32)),
        'a'..='z' => char::from_u32(lower + (c as u32 - 'a' as u32)),
        '0'..='9' => digit.and_then(|base| char::from_u32(base + (c as u32 - '0' as u32))),
        _ => None,
    };
    mapped.unwrap_or(c).to_string()
}

fn mono_char(c: char) -> String {
    let mapped = match c {
        'A'..='Z' => char::from_u32(0x1D670 + (c as u32 - 'A' as u32)),
        'a'..='z' => char::from_u32(0x1D68A + (c as u32 - 'a' as u32)),
        '0'..='9' => char::from_u32(0x1D7F6 + (c as u32 - '0' as u32)),
        _ => None,
    };
    mapped.unwrap_or(c).to_string()
}

/// Inline Markdown on one line: `**b**`, `*i*`, `_i_`, `` `code` ``, `[t](u)`.
fn inline(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut bold = false;
    let mut italic = false;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '`'
            && let Some(close) = chars[i + 1..].iter().position(|&x| x == '`')
        {
            for &ch in &chars[i + 1..i + 1 + close] {
                out.push_str(&mono_char(ch));
            }
            i += close + 2;
            continue;
        }
        if c == '['
            && let Some(mid) = chars[i..].iter().position(|&x| x == ']')
            && chars.get(i + mid + 1) == Some(&'(')
            && let Some(end) = chars[i + mid + 1..].iter().position(|&x| x == ')')
        {
            let label: String = chars[i + 1..i + mid].iter().collect();
            let url: String = chars[i + mid + 2..i + mid + 1 + end].iter().collect();
            out.push_str(&label);
            out.push(' ');
            out.push_str(&url);
            i += mid + end + 2;
            continue;
        }
        if c == '*' && next == Some('*') {
            bold = !bold;
            i += 2;
            continue;
        }
        if (c == '*' || c == '_') && boundary_ok(&chars, i, c, italic) {
            italic = !italic;
            i += 1;
            continue;
        }
        out.push_str(&style_char(c, bold, italic));
        i += 1;
    }
    out
}

/// A lone `*` or `_` marks italics only when it opens against a letter or,
/// with italics on, closes before a non-letter — `snake_case_names` must
/// stay as typed.
fn boundary_ok(chars: &[char], i: usize, marker: char, italic: bool) -> bool {
    let prev = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
    let next = chars.get(i + 1).copied();
    if italic {
        return next.is_none_or(|n| !n.is_alphanumeric());
    }
    next.is_some_and(|n| !n.is_whitespace() && n != marker)
        && prev.is_none_or(|p| !p.is_alphanumeric())
}

/// Block-level Markdown to plain lines; `---` becomes a thread break marker.
pub fn render(markdown: &str) -> String {
    let mut out = Vec::new();
    for raw in markdown.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed == "---" || trimmed == "***" {
            out.push("\u{1}".to_string());
            continue;
        }
        if let Some(heading) = trimmed.strip_prefix('#') {
            let text = heading.trim_start_matches('#').trim();
            out.push(inline(&format!("**{text}**")));
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            out.push(format!("• {}", inline(item)));
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix("> ") {
            out.push(format!("“{}”", inline(quote)));
            continue;
        }
        out.push(inline(line));
    }
    out.join("\n")
}

/// Cuts rendered text into parts under the limit.
pub fn split(rendered: &str, limit: usize) -> Vec<String> {
    let mut parts = Vec::new();
    for block in rendered.split('\u{1}') {
        let block = block.trim_matches('\n');
        if block.trim().is_empty() {
            continue;
        }
        let mut current = String::new();
        for piece in pieces(block) {
            let candidate = if current.is_empty() {
                piece.clone()
            } else {
                format!("{current}{piece}")
            };
            if count(candidate.trim()) <= limit {
                current = candidate;
                continue;
            }
            if !current.trim().is_empty() {
                parts.push(current.trim().to_string());
            }
            current = piece.trim_start().to_string();
            // A single piece over the limit is cut at words, then hard.
            while count(&current) > limit {
                let cut = cut_point(&current, limit);
                parts.push(current[..cut].trim().to_string());
                current = current[cut..].trim_start().to_string();
            }
        }
        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }
    }
    parts
}

/// Paragraphs, then sentences, each with its trailing separator kept.
fn pieces(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in block.split_inclusive("\n\n") {
        let mut start = 0;
        let bytes: Vec<(usize, char)> = paragraph.char_indices().collect();
        for (n, &(_, c)) in bytes.iter().enumerate() {
            let ends_sentence = matches!(c, '.' | '!' | '?' | '\n')
                && bytes
                    .get(n + 1)
                    .is_none_or(|&(_, next)| next.is_whitespace());
            if ends_sentence {
                let end = bytes.get(n + 1).map_or(paragraph.len(), |&(j, _)| j);
                out.push(paragraph[start..end].to_string());
                start = end;
            }
        }
        if start < paragraph.len() {
            out.push(paragraph[start..].to_string());
        }
    }
    out
}

fn cut_point(text: &str, limit: usize) -> usize {
    let mut last_space = None;
    let mut last_ok = 0;
    for (i, grapheme) in text.grapheme_indices(true) {
        if count(&text[..i + grapheme.len()]) > limit {
            break;
        }
        last_ok = i + grapheme.len();
        if grapheme.chars().all(char::is_whitespace) {
            last_space = Some(i);
        }
    }
    match last_space {
        Some(space) if space > 0 => space,
        _ => last_ok.max(1),
    }
}

pub fn prepare(markdown: &str) -> Prepared {
    let parts = split(&render(markdown), LIMIT)
        .into_iter()
        .map(|text| Part {
            count: count(&text),
            text,
        })
        .collect();
    Prepared {
        parts,
        limit: LIMIT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_follows_x_weights() {
        assert_eq!(count("hello"), 5);
        assert_eq!(count("héllo"), 5);
        assert_eq!(count("日本語"), 6);
        assert_eq!(count("👍"), 2);
        assert_eq!(count("👨‍👩‍👧"), 2);
        assert_eq!(
            count("see https://example.com/a/very/long/path/indeed ok"),
            4 + 23 + 3
        );
        assert_eq!(count("see example.com/path ok"), 4 + 23 + 3);
        assert_eq!(count("email me@example.com"), 20);
        assert_eq!(count("(see x.com/home)"), 5 + 23 + 1);
        assert_eq!(count("snake_case.rs"), 13);
        assert_eq!(count(""), 0);
    }

    #[test]
    fn inline_styles_become_unicode_letters() {
        assert_eq!(inline("**Bold** text"), "𝗕𝗼𝗹𝗱 text");
        assert_eq!(inline("*it*"), "𝘪𝘵");
        assert_eq!(inline("a `code` b"), "a 𝚌𝚘𝚍𝚎 b");
        assert_eq!(inline("[X](https://x.com)"), "X https://x.com");
        assert_eq!(inline("snake_case_names stay"), "snake_case_names stay");
        assert_eq!(inline("2 * 3 * 4"), "2 * 3 * 4");
        let both = inline("***both***");
        assert_eq!(both.chars().count(), 4);
        assert_eq!(both.chars().next().map(|c| c as u32), Some(0x1D657));
    }

    #[test]
    fn blocks_flatten_and_breaks_split_the_thread() {
        let rendered = render("# Title\n\n- one\n- two\n\n> quoted\n\n---\n\nsecond post");
        assert!(rendered.starts_with("𝗧𝗶𝘁𝗹𝗲\n\n• one\n• two\n\n“quoted”"));
        let parts = split(&rendered, LIMIT);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], "second post");
    }

    #[test]
    fn long_text_is_cut_at_paragraphs_sentences_then_words() {
        let sentence = "This is a sentence that has some words in it. ";
        let text = sentence.repeat(10);
        let parts = split(&text, LIMIT);
        assert!(parts.len() >= 2);
        for part in &parts {
            assert!(count(part) <= LIMIT, "{part}");
            assert!(part.ends_with('.'), "{part}");
        }
        let words = "word ".repeat(100);
        let parts = split(&words, LIMIT);
        assert!(
            parts
                .iter()
                .all(|p| count(p) <= LIMIT && !p.contains("wor d"))
        );
        let solid = "x".repeat(600);
        let parts = split(&solid, LIMIT);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 280);
    }

    #[test]
    fn prepare_reports_counts_per_part() {
        let prepared = prepare("hello **world**\n---\nagain");
        assert_eq!(prepared.parts.len(), 2);
        // The bold letters are outside the light ranges, so they weigh two.
        assert_eq!(prepared.parts[0].count, 16);
        assert_eq!(prepared.parts[1].text, "again");
        assert_eq!(prepared.limit, 280);
        assert!(prepare("   \n\n").parts.is_empty());
    }
}
