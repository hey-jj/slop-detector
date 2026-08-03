//! Report-layer pins: container pre-classification (annotation, never
//! suppression), the topic-vocabulary allowlist labeling, and the additive
//! `stats.densities` block with its raw-beside-residual figures and the
//! 100-word floor.

use slop_detector::{analyze, analyze_with, AnalyzeOptions, Container};

fn opts(terms: &[&str]) -> AnalyzeOptions {
    AnalyzeOptions {
        allow_terms: terms.iter().map(|t| t.to_string()).collect(),
    }
}

/// A finding inside a fenced block still reports — slop-detector has no
/// segmentation — and carries the fenced-code label so the reader can
/// discount it without hand work.
#[test]
fn fenced_finding_reports_with_annotation() {
    let text = "Plain prose sits here.\n\n```\nWe delve into the config.\n```\n";
    let report = analyze(text);
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SLOP-A001")
        .expect("delve inside a fence must still report");
    assert_eq!(hit.container, Container::FencedCode);
    // Raw counts keep it; residual (prose-only) excludes it. Below the
    // 100-word floor no rate is computed, but the counts stand.
    let d = report.stats.densities;
    assert_eq!(d.spike.hits, 1);
    assert_eq!(d.spike.residual_hits, 0);
    assert_eq!(d.spike.per_1k_words, None, "floor: no rate under 100 words");
}

/// Container labels across the heuristics, on one document.
#[test]
fn containers_classify_blockquote_quoted_heading_and_prose() {
    let text = "# Delve Metrics\n\nThe team continues to delve into results.\n\n> They delve further each week.\n\nShe said \"we delve into the data\" on the call.\n";
    let report = analyze(text);
    let containers: Vec<Container> = report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == "SLOP-A001")
        .map(|f| f.container)
        .collect();
    assert_eq!(
        containers,
        [
            Container::Heading,
            Container::Prose,
            Container::Blockquote,
            Container::Quoted
        ],
        "{report:?}"
    );
    let d = report.stats.densities;
    assert_eq!(d.spike.hits, 4);
    assert_eq!(d.spike.residual_hits, 1, "only the prose hit is residual");
}

/// The A005 quotation divergence from ai-slop, pinned: ai-slop suppresses
/// quoted metaphor-reach idioms entirely; slop-detector has no quotation
/// suppression, so the quoted idiom reports WITH its container label.
#[test]
fn quoted_metaphor_reach_reports_with_container_label() {
    let text = "Prose line one sits here.\n\n> The paper weaves together two traditions.\n";
    let report = analyze(text);
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SLOP-A005")
        .expect("no quotation suppression inbound: the hit must report");
    assert_eq!(hit.container, Container::Blockquote);
}

/// `--allow-term` labels a finding whose matched text equals the term,
/// case-insensitively; it never removes the finding, and only the residual
/// figures move.
#[test]
fn allow_term_labels_without_suppressing() {
    let text = "The essay delves into flourishing and its critics with care.";
    let plain = analyze(text);
    let labeled = analyze_with(text, &opts(&["Delves"]));
    // Same findings either way.
    assert_eq!(plain.quality_patterns.len(), labeled.quality_patterns.len());
    let hit = labeled
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SLOP-A001")
        .unwrap();
    assert!(hit.topic_term);
    assert!(
        !plain.quality_patterns.iter().any(|f| f.topic_term),
        "no label without the flag"
    );
    // Raw density keeps the hit; residual drops it.
    assert_eq!(labeled.stats.densities.spike.hits, 1);
    assert_eq!(labeled.stats.densities.spike.residual_hits, 0);
    assert_eq!(plain.stats.densities.spike.residual_hits, 1);
    // A term that matches nothing changes nothing.
    let unrelated = analyze_with(text, &opts(&["kubernetes"]));
    assert!(!unrelated.quality_patterns.iter().any(|f| f.topic_term));
}

/// Whole-term equality: an allow-term must equal the matched text, not
/// merely appear inside it.
#[test]
fn allow_term_is_whole_term_equality() {
    let text = "We leverage the platform daily.";
    let report = analyze_with(text, &opts(&["lever"]));
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q001")
        .unwrap();
    assert!(!hit.topic_term, "substring must not label");
}

/// At or above the 100-word floor the per-1k rates are computed for every
/// class, residual beside raw, and the arithmetic matches the documented
/// formula.
#[test]
fn densities_compute_above_the_floor() {
    // 100+ words of neutral, non-repeating filler (a repeated filler
    // sentence would rightly fire SD-Q005) plus two spike words, one of
    // them inside a blockquote.
    let filler = "The vendor sent the revised quote on Monday morning. Legal asked \
                  for one change to clause seven before signature. Finance confirmed \
                  the seat count against the January invoice. The renewal lands in \
                  March and needs a decision by the offsite. Procurement wants both \
                  bids attached to the ticket. The migration plan still lists two \
                  open dependencies from the platform team. Support coverage over \
                  the holiday window was approved last week. The training budget \
                  moves to the second quarter unchanged. Facilities booked the \
                  larger room for the review. Recruiting closed the backend role \
                  after the final loop on Thursday. The board summary goes out \
                  Friday afternoon. Notes from the previous call are linked in the \
                  agenda document. ";
    let text = format!("{filler}\nWe delve into details.\n\n> A tapestry of options.\n");
    let report = analyze(&text);
    let words = report.stats.word_count;
    assert!(words >= 100, "fixture must clear the floor ({words})");
    let d = report.stats.densities;
    assert_eq!(d.spike.hits, 2);
    assert_eq!(d.spike.residual_hits, 1);
    assert_eq!(
        d.spike.per_1k_words,
        Some(2.0 * 1000.0 / words as f64),
        "documented formula"
    );
    assert_eq!(
        d.spike.residual_per_1k_words,
        Some(1.0 * 1000.0 / words as f64)
    );
    // Untouched classes report zero hits with a computed zero rate.
    assert_eq!(d.individual.hits, 0);
    assert_eq!(d.individual.per_1k_words, Some(0.0));
}

/// The JSON shape: additive fields serialize under stable names, and the
/// report still carries no verdict-shaped key anywhere.
#[test]
fn json_shape_is_additive_and_verdict_free() {
    let text = "We delve into the numbers again today.";
    let json = serde_json::to_string_pretty(&analyze(text)).unwrap();
    for key in [
        "\"densities\"",
        "\"container\"",
        "\"topic_term\"",
        "\"residual_hits\"",
        "\"per_1k_words\"",
    ] {
        assert!(json.contains(key), "{key} missing:\n{json}");
    }
    assert!(json.contains("\"container\": \"prose\""));
    // The no-verdict invariant, checked at the serialization surface.
    for banned in [
        "\"verdict\"",
        "\"score\"",
        "\"tier\"",
        "\"severity\"",
        "\"pass\"",
        "\"fail\"",
        "\"threshold\"",
    ] {
        assert!(!json.contains(banned), "{banned} leaked into the report");
    }
}
