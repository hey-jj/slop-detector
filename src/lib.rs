//! Deterministic pattern detector for inbound received text.
//!
//! Text in, evidence report out. The library performs no I/O. `analyze` is a
//! pure function of the input text. It reports pattern occurrences with cited
//! spans. It attaches no judgment or score. A coupled agent skill interprets
//! the report.
//!
//! `analyze_bundle` is the multi-document entry point: per-file reports plus
//! cross-file verbatim-duplication evidence for a set of related documents
//! (deck variants, shared report copies). Each per-file report is identical
//! to what `analyze` returns for that text alone.

mod container;
pub mod data;
mod duplication;
pub mod engine;
pub mod report;

pub use report::{
    BundleFile, BundleReport, ClassDensity, Container, CrossFileDuplication, CrossFileOccurrence,
    Densities, EvidenceReport, Finding, Stats,
};

/// Snippet cap in bytes. A longer match is cited by its span; the snippet
/// carries the first cap-aligned bytes.
const SNIPPET_CAP: usize = 200;

/// Density floor in words. Below it the per-1k figures are omitted: short
/// texts quantize, and one register word in a 30-word note produces a huge
/// number that means nothing.
const DENSITY_FLOOR_WORDS: usize = 100;

/// Caller-supplied reading context. Nothing here changes what fires; the
/// options only label findings and shift the residual density figures,
/// which always print beside the raw ones.
#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    /// Topic-vocabulary allowlist (the CLI's repeatable `--allow-term`):
    /// per-run, human-supplied context ("this paper is about flourishing"),
    /// never shipped data. A finding whose matched text equals a term
    /// (case-insensitive, whole-term) is labeled `topic_term` and leaves
    /// the residual densities while staying in the raw ones.
    pub allow_terms: Vec<String>,
}

/// Analyze one input text and return the evidence report.
///
/// Findings route to the report categories by each rule's `category` field
/// in `data/inbound/inbound.toml`. Output is deterministic: same input,
/// byte-identical report. Never panics on any input.
pub fn analyze(text: &str) -> EvidenceReport {
    analyze_with(text, &AnalyzeOptions::default())
}

/// `analyze` with caller-supplied reading context. `analyze(text)` is
/// exactly `analyze_with(text, &AnalyzeOptions::default())`.
pub fn analyze_with(text: &str, opts: &AnalyzeOptions) -> EvidenceReport {
    let cp = engine::compiled();
    let mut hits = engine::scan_all(cp, text);
    hits.sort_by(|a, b| {
        (a.span.start, a.span.end, cp.rules[a.rule].id.as_str()).cmp(&(
            b.span.start,
            b.span.end,
            cp.rules[b.rule].id.as_str(),
        ))
    });

    let containers = container::Containers::scan(text);
    let allow: Vec<String> = opts.allow_terms.iter().map(|t| t.to_lowercase()).collect();

    let mut report = EvidenceReport {
        stats: report::Stats {
            word_count: count_words(text),
            byte_len: text.len(),
            densities: report::Densities::default(),
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
            container: containers.classify(span.start),
            topic_term: !allow.is_empty() && allow.contains(&text[span.clone()].to_lowercase()),
        };
        // Same rule at the same span reports once.
        fn bucket_of(report: &mut EvidenceReport, category: data::Category) -> &mut Vec<Finding> {
            match category {
                data::Category::PasteResidue => &mut report.paste_residue,
                data::Category::QualityPatterns => &mut report.quality_patterns,
                data::Category::Injection => &mut report.injection_patterns,
            }
        }
        if bucket_of(&mut report, rule.category)
            .last()
            .map(|f| f.rule_id == finding.rule_id && f.span == finding.span)
            .unwrap_or(false)
        {
            continue;
        }
        if rule.category == data::Category::QualityPatterns {
            let class = rule.class.expect("quality rules carry a class (loader)");
            let slot = match class {
                data::Class::Spike => &mut report.stats.densities.spike,
                data::Class::Background => &mut report.stats.densities.background,
                data::Class::Individual => &mut report.stats.densities.individual,
            };
            slot.hits += 1;
            if finding.container == Container::Prose && !finding.topic_term {
                slot.residual_hits += 1;
            }
        }
        bucket_of(&mut report, rule.category).push(finding);
    }
    finish_densities(&mut report.stats);
    report
}

/// Fill the per-1k figures once the counts are final. Below the density
/// floor the rates stay `None`: the counts are still reported, the rate is
/// not pretended.
fn finish_densities(stats: &mut report::Stats) {
    let words = stats.word_count;
    let d = &mut stats.densities;
    for slot in [&mut d.spike, &mut d.background, &mut d.individual] {
        if words >= DENSITY_FLOOR_WORDS {
            slot.per_1k_words = Some(slot.hits as f64 * 1000.0 / words as f64);
            slot.residual_per_1k_words = Some(slot.residual_hits as f64 * 1000.0 / words as f64);
        }
    }
}

/// Analyze a set of related documents: `(path, text)` pairs. Each file gets
/// its own full `analyze` report; on top of them, one cross-file
/// duplication pass (the SD-Q005 shingle machinery with the same order,
/// floor, and cap) reports verbatim runs shared BETWEEN files. Within-file
/// repeats stay in each file's own SD-Q005 findings and never appear at
/// bundle level.
pub fn analyze_bundle(docs: &[(String, String)]) -> BundleReport {
    analyze_bundle_with(docs, &AnalyzeOptions::default())
}

/// `analyze_bundle` with caller-supplied reading context, applied to every
/// per-file report.
pub fn analyze_bundle_with(docs: &[(String, String)], opts: &AnalyzeOptions) -> BundleReport {
    let files = docs
        .iter()
        .map(|(path, text)| BundleFile {
            path: path.clone(),
            report: analyze_with(text, opts),
        })
        .collect();
    BundleReport {
        files,
        cross_file_duplication: cross_file_duplication(docs),
    }
}

/// The cross-file pass. All files share one token table and one shingle
/// map; the segment id bumps at every file boundary, so a run can never
/// fuse across two files. Fenced content participates like everything
/// else and each occurrence carries its container annotation. Grouping
/// happens BEFORE the emission cap: runs that share an anchor (same
/// earlier copy, same length) fold into one entry whose occurrences list
/// the anchor and every later copy in file order, and the cap then keeps
/// the longest ENTRIES — a passage shared by many files never silently
/// drops later occurrences.
fn cross_file_duplication(docs: &[(String, String)]) -> Vec<CrossFileDuplication> {
    let cp = engine::compiled();
    let Some(dr) = cp.duplication_rules.first() else {
        return Vec::new();
    };
    let mut tokens = duplication::Tokens::new();
    for (file, (_, text)) in docs.iter().enumerate() {
        // A run never spans a file boundary: one segment per file.
        duplication::tokenize_into(&mut tokens, text, file as u32, file as u32);
    }
    let runs = duplication::find_runs(&tokens, dr.shingle_words, dr.min_run_words, true);
    // Group by anchor: (earlier index, length) identifies one duplicated
    // text; every later copy is an occurrence of it. Scan order is file
    // order, which grouping preserves.
    let mut entries: Vec<((usize, usize), Vec<usize>)> = Vec::new();
    for run in &runs {
        let key = (run.earlier, run.len);
        match entries.iter_mut().find(|(k, _)| *k == key) {
            Some((_, laters)) => laters.push(run.later),
            None => entries.push((key, vec![run.later])),
        }
    }
    // The cap applies to whole entries, longest first, anchor position as
    // the deterministic tiebreak. An entry that survives keeps ALL its
    // occurrences.
    entries.sort_by_key(|&((earlier, len), _)| (std::cmp::Reverse(len), earlier));
    entries.truncate(dr.max_reports);
    let containers: Vec<container::Containers> = docs
        .iter()
        .map(|(_, text)| container::Containers::scan(text))
        .collect();
    // Occurrence construction resolves the owning document by the token's
    // retained FILE INDEX, never by re-finding the path string: two bundle
    // entries may share one path label, and a label collision must not
    // slice one document with another document's span.
    let occurrence = |tok: usize, len: usize| {
        let toks = &tokens.toks;
        let file = toks[tok].file as usize;
        let text = &docs[file].1;
        let span = widen_to_char_boundaries(text, toks[tok].start..toks[tok + len - 1].end);
        let occ = CrossFileOccurrence {
            path: docs[file].0.clone(),
            span: (span.start, span.end),
            container: containers[file].classify(span.start),
        };
        (file, occ)
    };
    let mut out: Vec<CrossFileDuplication> = entries
        .into_iter()
        .map(|((earlier, len), laters)| {
            let (anchor_file, anchor) = occurrence(earlier, len);
            let (snippet, snippet_truncated) =
                snippet_of(&docs[anchor_file].1, &(anchor.span.0..anchor.span.1));
            let mut occurrences = vec![anchor];
            occurrences.extend(laters.iter().map(|&l| occurrence(l, len).1));
            CrossFileDuplication {
                snippet,
                snippet_truncated,
                occurrences,
            }
        })
        .collect();
    // Deterministic reading order: by first occurrence position.
    out.sort_by(|a, b| {
        (&a.occurrences[0].path, a.occurrences[0].span)
            .cmp(&(&b.occurrences[0].path, b.occurrences[0].span))
    });
    out
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
