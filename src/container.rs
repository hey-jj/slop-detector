//! Container pre-classification: one linear pre-pass over the raw source
//! that labels where each finding sits, so the reading agent can discount
//! quoted or fenced material without hand work. Annotation only, never
//! suppression: classification changes no finding, only its `container`
//! label, so a misread by these deliberately crude heuristics costs a
//! label, not a finding. slop-detector has no markdown segmentation; these
//! span sets are heuristics over raw bytes and the skill says so.

use crate::report::Container;
use std::ops::Range;

/// The classified span sets. Each list is sorted and non-overlapping:
/// fenced, blockquote, and heading by construction (one forward pass over
/// non-overlapping lines), quoted by an explicit sort-and-merge, since
/// nested straight/curly pairs can close out of start order.
pub(crate) struct Containers {
    fenced: Vec<Range<usize>>,
    blockquote: Vec<Range<usize>>,
    quoted: Vec<Range<usize>>,
    heading: Vec<Range<usize>>,
}

/// Fenced-code regions: a line whose trimmed content starts with three
/// backticks toggles fence state; the region covers both marker lines. An
/// unclosed fence runs to the end of the text. Annotation only: SD-Q005
/// tokenizes fenced content like every other rule, and a duplicated run
/// inside a fence carries this label instead of being skipped.
pub(crate) fn fenced_ranges(src: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for (start, line) in lines_with_offsets(src) {
        if line.trim_start().starts_with("```") {
            match open.take() {
                Some(s) => out.push(s..start + line.len()),
                None => open = Some(start),
            }
        }
    }
    if let Some(s) = open {
        out.push(s..src.len());
    }
    out
}

/// Iterate lines with their byte offsets, line terminators excluded from
/// the yielded slice but not skipped in offset accounting.
fn lines_with_offsets(src: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        if at >= src.len() {
            return None;
        }
        let start = at;
        let rest = &src[at..];
        let (line, advance) = match rest.find('\n') {
            Some(n) => (&rest[..n], n + 1),
            None => (rest, rest.len()),
        };
        at += advance;
        let line = line.strip_suffix('\r').unwrap_or(line);
        Some((start, line))
    })
}

/// A short line in Title Case, with no sentence-closing punctuation, reads
/// as a heading. Deliberately crude: annotation only.
fn title_case_heading(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.len() > 60 {
        return false;
    }
    if t.ends_with(['.', '!', '?', ',', ';', ':']) {
        return false;
    }
    let stop = [
        "a", "an", "the", "and", "or", "of", "in", "on", "for", "to", "with", "at", "by", "vs",
    ];
    let mut words = t.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let leads_upper = |w: &str| {
        w.chars()
            .next()
            .map(|c| c.is_uppercase() || c.is_ascii_digit())
            .unwrap_or(false)
    };
    if !leads_upper(first) {
        return false;
    }
    words.all(|w| leads_upper(w) || stop.contains(&w.to_ascii_lowercase().as_str()))
}

impl Containers {
    pub(crate) fn scan(src: &str) -> Self {
        let fenced = fenced_ranges(src);
        let mut blockquote = Vec::new();
        let mut heading = Vec::new();
        let mut quoted = Vec::new();
        // Quoted spans: straight and curly double quotes only, tracked
        // independently. State resets at blank lines so an unbalanced
        // quote cannot poison the rest of the document; an open span
        // discarded at a blank line is simply not recorded.
        let mut open_straight: Option<usize> = None;
        let mut open_curly: Option<usize> = None;
        for (start, line) in lines_with_offsets(src) {
            if line.trim().is_empty() {
                open_straight = None;
                open_curly = None;
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with('>') {
                blockquote.push(start..start + line.len());
            } else if trimmed.starts_with('#') || title_case_heading(line) {
                heading.push(start..start + line.len());
            }
            for (i, c) in line.char_indices() {
                let abs = start + i;
                match c {
                    '"' => match open_straight.take() {
                        Some(s) => quoted.push(s..abs + c.len_utf8()),
                        None => open_straight = Some(abs),
                    },
                    '\u{201C}' => {
                        if open_curly.is_none() {
                            open_curly = Some(abs);
                        }
                    }
                    '\u{201D}' => {
                        if let Some(s) = open_curly.take() {
                            quoted.push(s..abs + c.len_utf8());
                        }
                    }
                    _ => {}
                }
            }
        }
        // Straight and curly pairs are tracked independently, so nested or
        // interleaved quotes close out of start order and their spans can
        // overlap ("outer \u{201C}inner\u{201D} tail"). `classify`'s binary
        // search requires each list sorted and non-overlapping: normalize
        // by sorting and merging. The merged union covers exactly the
        // positions inside at least one closed quote pair — no
        // prose-only position gains the label, and a position the raw
        // list would misclassify as prose (inside the outer pair, before
        // the inner one) regains it.
        quoted.sort_by_key(|r: &Range<usize>| (r.start, r.end));
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in quoted {
            match merged.last_mut() {
                Some(m) if r.start <= m.end => m.end = m.end.max(r.end),
                _ => merged.push(r),
            }
        }
        Containers {
            fenced,
            blockquote,
            quoted: merged,
            heading,
        }
    }

    /// Classify the position `at` (a finding's span start). Precedence when
    /// sets overlap: fenced-code, then blockquote, then quoted, then
    /// heading; everything else is prose.
    pub(crate) fn classify(&self, at: usize) -> Container {
        if covers(&self.fenced, at) {
            Container::FencedCode
        } else if covers(&self.blockquote, at) {
            Container::Blockquote
        } else if covers(&self.quoted, at) {
            Container::Quoted
        } else if covers(&self.heading, at) {
            Container::Heading
        } else {
            Container::Prose
        }
    }
}

/// Binary search over a sorted, non-overlapping range list.
fn covers(ranges: &[Range<usize>], at: usize) -> bool {
    let i = ranges.partition_point(|r| r.end <= at);
    ranges.get(i).map(|r| r.start <= at).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_toggle_and_unclosed_fence() {
        let src = "prose\n```\ncode here\n```\nafter\n```\ntail";
        let f = fenced_ranges(src);
        assert_eq!(f.len(), 2);
        assert!(src[f[0].clone()].contains("code here"));
        assert_eq!(f[1].end, src.len(), "unclosed fence runs to EOF");
        assert!(!f[0].contains(&src.find("after").unwrap()));
    }

    #[test]
    fn classification_precedence_and_reset() {
        let src = "# Heading Line\n\nplain prose \"a quoted span\" more\n\n> quoted block\n\nAn \"unbalanced quote\n\nclean prose after the blank line\n";
        let c = Containers::scan(src);
        assert_eq!(c.classify(0), Container::Heading);
        assert_eq!(c.classify(src.find("plain").unwrap()), Container::Prose);
        assert_eq!(
            c.classify(src.find("quoted span").unwrap()),
            Container::Quoted
        );
        assert_eq!(
            c.classify(src.find("> quoted").unwrap()),
            Container::Blockquote
        );
        // The unbalanced quote resets at the blank line and poisons nothing.
        assert_eq!(
            c.classify(src.find("clean prose").unwrap()),
            Container::Prose
        );
    }

    /// Nested and interleaved straight/curly pairs produce overlapping,
    /// out-of-order raw spans; after normalization the quoted list must
    /// stay sorted and non-overlapping, classify never panics, positions
    /// inside the outer pair (including before the inner one) are Quoted,
    /// and prose outside every pair stays Prose.
    #[test]
    fn nested_and_interleaved_quotes_classify_safely() {
        let src = "He said \"we delve into \u{201C}nested\u{201D} data\" today.\n\
                   Then \u{201C}curly holds \"straight inside\" still\u{201D} here.\n\
                   Interleaved \"straight \u{201C}then curly\" closes\u{201D} after.\n\
                   Plain prose closes the document.\n";
        let c = Containers::scan(src);
        for w in c.quoted.windows(2) {
            assert!(w[0].end <= w[1].start, "overlap survived: {:?}", c.quoted);
        }
        // Inside the outer straight pair, before the nested curly pair.
        assert_eq!(c.classify(src.find("delve").unwrap()), Container::Quoted);
        // Inside the nested pair and after it, still inside the outer.
        assert_eq!(c.classify(src.find("nested").unwrap()), Container::Quoted);
        assert_eq!(
            c.classify(src.find(" data").unwrap() + 1),
            Container::Quoted
        );
        // Prose after the outer close on the same line.
        assert_eq!(c.classify(src.find("today").unwrap()), Container::Prose);
        // Curly-outer with straight-inner, and interleaved closes.
        assert_eq!(
            c.classify(src.find("curly holds").unwrap()),
            Container::Quoted
        );
        assert_eq!(c.classify(src.find("here").unwrap()), Container::Prose);
        assert_eq!(
            c.classify(src.find("then curly").unwrap()),
            Container::Quoted
        );
        assert_eq!(c.classify(src.find("after").unwrap()), Container::Prose);
        // Untouched prose lines never gain the label.
        assert_eq!(
            c.classify(src.find("Plain prose").unwrap()),
            Container::Prose
        );
        assert_eq!(c.classify(src.len().saturating_sub(1)), Container::Prose);
    }

    #[test]
    fn title_case_heading_is_short_and_unpunctuated() {
        assert!(title_case_heading("Quarterly Revenue Summary"));
        assert!(title_case_heading("Roadmap for the Next Phase"));
        assert!(!title_case_heading("The vendor sent the revised quote."));
        assert!(!title_case_heading("plain lowercase line"));
        assert!(!title_case_heading("Hi Omar,"));
    }
}
