//! Deterministic pattern detector for inbound received text.
//!
//! Text in, evidence report out. The library performs no I/O. `analyze` is a
//! pure function of the input text. It reports pattern occurrences with cited
//! spans. It attaches no judgment or score. A coupled agent skill interprets
//! the report.

pub mod data;
pub mod engine;
pub mod report;

pub use report::{EvidenceReport, Finding, Stats};

/// Snippet cap in bytes. A longer match is cited by its span; the snippet
/// carries the first cap-aligned bytes.
const SNIPPET_CAP: usize = 200;

/// Analyze one input text and return the evidence report.
///
/// Findings route to the report categories by each rule's `category` field
/// in `data/inbound/inbound.toml`. Output is deterministic: same input,
/// byte-identical report. Never panics on any input.
pub fn analyze(text: &str) -> EvidenceReport {
    let cp = engine::compiled();
    let mut hits = engine::scan_all(cp, text);
    hits.sort_by(|a, b| {
        (a.span.start, a.span.end, cp.rules[a.rule].id.as_str()).cmp(&(
            b.span.start,
            b.span.end,
            cp.rules[b.rule].id.as_str(),
        ))
    });

    let mut report = EvidenceReport {
        stats: report::Stats {
            word_count: count_words(text),
            byte_len: text.len(),
        },
        ..EvidenceReport::default()
    };
    for hit in hits {
        let rule = &cp.rules[hit.rule];
        let span = widen_to_char_boundaries(text, hit.span);
        if span.start >= span.end {
            continue;
        }
        let (snippet, snippet_truncated) = snippet_of(text, &span);
        let finding = Finding {
            rule_id: rule.id.clone(),
            span: (span.start, span.end),
            snippet,
            snippet_truncated,
        };
        let bucket = match rule.category {
            data::Category::PasteResidue => &mut report.paste_residue,
            data::Category::QualityPatterns => &mut report.quality_patterns,
            data::Category::Injection => &mut report.injection_patterns,
        };
        // Same rule at the same span reports once.
        if bucket
            .last()
            .map(|f| f.rule_id == finding.rule_id && f.span == finding.span)
            .unwrap_or(false)
        {
            continue;
        }
        bucket.push(finding);
    }
    report
}

/// Widen a span outward to character boundaries. Every pass emits
/// boundary-aligned spans already; this is the defensive floor under the
/// never-panic contract.
pub(crate) fn widen_to_char_boundaries(
    src: &str,
    mut span: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    span.start = span.start.min(src.len());
    span.end = span.end.min(src.len());
    while span.start > 0 && !src.is_char_boundary(span.start) {
        span.start -= 1;
    }
    while span.end < src.len() && !src.is_char_boundary(span.end) {
        span.end += 1;
    }
    span
}

/// The snippet is the source slice at the span, verbatim. A slice over
/// `SNIPPET_CAP` bytes is capped on a character boundary and flagged
/// truncated; the span still covers the whole occurrence.
fn snippet_of(src: &str, span: &std::ops::Range<usize>) -> (String, bool) {
    let slice = &src[span.clone()];
    if slice.len() <= SNIPPET_CAP {
        return (slice.to_string(), false);
    }
    let mut end = SNIPPET_CAP;
    while !slice.is_char_boundary(end) {
        end -= 1;
    }
    (slice[..end].to_string(), true)
}

/// Count maximal `is_xid_continue` runs, the same word counter ai-slop uses.
fn count_words(s: &str) -> usize {
    let mut n = 0usize;
    let mut in_word = false;
    for c in s.chars() {
        let w = unicode_ident::is_xid_continue(c);
        if w && !in_word {
            n += 1;
        }
        in_word = w;
    }
    n
}
