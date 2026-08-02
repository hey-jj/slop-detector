//! Fixture and invariant tests for the quality_patterns category.
//! Positives use formulaic prose; negatives use ordinary human business
//! writing, because inbound false positives are the expensive error.

use slop_detector::{analyze, EvidenceReport};

fn quality_ids(report: &EvidenceReport) -> Vec<&str> {
    report
        .quality_patterns
        .iter()
        .map(|f| f.rule_id.as_str())
        .collect()
}

fn count(report: &EvidenceReport, id: &str) -> usize {
    report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == id)
        .count()
}

fn assert_span_invariant(text: &str, report: &EvidenceReport) {
    let all = report
        .paste_residue
        .iter()
        .chain(&report.quality_patterns)
        .chain(&report.injection_patterns);
    for f in all {
        let (start, end) = f.span;
        assert!(start < end && end <= text.len(), "{}: bad span", f.rule_id);
        let slice = &text[start..end];
        if f.snippet_truncated {
            assert!(slice.starts_with(&f.snippet), "{}", f.rule_id);
        } else {
            assert_eq!(slice, f.snippet, "{}: snippet != source[span]", f.rule_id);
        }
    }
}

// --- spike class ----------------------------------------------------------

#[test]
fn spike_lexicon_fires_on_the_measured_excess_words() {
    let text = "We delve into a rich tapestry of intricate options, a testament to the myriad commendable paths ahead.";
    let report = analyze(text);
    // delve, tapestry, intricate, testament, myriad, commendable.
    assert_eq!(count(&report, "SLOP-A001"), 6, "{report:?}");
    assert_span_invariant(text, &report);
}

#[test]
fn stock_opener_fires_at_full_weight() {
    let report = analyze("In today's fast-paced world, teams need clarity.");
    assert_eq!(count(&report, "SLOP-O003"), 1);
}

#[test]
fn demoted_ornamental_register_fires_as_background_not_spike() {
    let text = "We leverage a robust and seamless platform to empower and unlock growth.";
    let report = analyze(text);
    // leverage, robust, seamless, empower, unlock.
    assert_eq!(count(&report, "SD-Q001"), 5, "{report:?}");
    assert_eq!(count(&report, "SLOP-A001"), 0);
}

#[test]
fn meticulous_stays_in_hype_adjectives_not_spike() {
    let report = analyze("The report was meticulously crafted and meticulous throughout.");
    assert_eq!(count(&report, "SLOP-A001"), 0);
    assert!(count(&report, "SLOP-I003") >= 2, "{report:?}");
}

// --- background lexicons --------------------------------------------------

#[test]
fn transition_trio_fires_only_at_block_or_sentence_start() {
    for text in [
        "Moreover, the results held.",
        "The pilot worked. Furthermore, costs fell.",
        "line one\nAdditionally, the audit passed.",
        "- Moreover, the bullet case counts.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-T002"), 1, "{text}");
    }
    for text in [
        "The data moreover suggests otherwise.",
        "We can additionally confirm the totals.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-T002"), 0, "{text}");
    }
}

#[test]
fn trimmed_transition_tail_does_not_fire() {
    for text in [
        "Also, the invoice is attached.",
        "Meanwhile, the team shipped v2.",
        "Ultimately, we chose the smaller vendor.",
        "Indeed, the numbers agree.",
        "Interestingly, both bids came in equal.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-T002"), 0, "{text}");
    }
}

#[test]
fn intensifiers_fire_at_background_with_the_technical_exemptions() {
    let report = analyze("The rollout was very smooth and truly fast.");
    assert_eq!(count(&report, "SLOP-I001"), 2);

    let report = analyze("We run a highly available cluster across regions.");
    assert_eq!(count(&report, "SLOP-I001"), 0, "{report:?}");
}

#[test]
fn importance_adjectives_respect_the_fixed_sense_exemptions() {
    let report = analyze("This is a crucial and pivotal change.");
    assert_eq!(count(&report, "SLOP-I002"), 2);

    let report =
        analyze("The critical path runs through the parser; see the critical section notes.");
    assert_eq!(count(&report, "SLOP-I002"), 0, "{report:?}");
}

#[test]
fn inflated_diction_fires_with_homograph_guards() {
    let report = analyze("We utilize the aforementioned process to facilitate onboarding.");
    assert_eq!(count(&report, "SLOP-A004"), 3);

    // Named resource metrics are exempt.
    let report = analyze("Peak cpu utilization stayed under 60% and memory utilization was flat.");
    assert_eq!(count(&report, "SLOP-A004"), 0, "{report:?}");

    // The tool-noun stack pattern fires; the ordinary senses do not.
    let report = analyze("The coverage instrument flags each block.");
    assert_eq!(count(&report, "SLOP-A004"), 1);
    let report = analyze("She plays a wind instrument; check the instrument panel and the financial instrument ledger.");
    assert_eq!(count(&report, "SLOP-A004"), 0, "{report:?}");

    // The participial noun stack fires.
    let report = analyze("The audit found generated-text defects in three sections.");
    assert_eq!(count(&report, "SLOP-A004"), 1);
}

#[test]
fn filler_meta_fires_but_bare_overall_does_not() {
    let report = analyze("It's important to note that the totals moved.");
    assert_eq!(count(&report, "SLOP-T001"), 1);

    let report = analyze("Overall, the quarter closed strong.");
    assert_eq!(count(&report, "SLOP-T001"), 0, "{report:?}");
}

// --- background structural regexes ---------------------------------------

#[test]
fn contrast_and_cadence_regexes_fire() {
    let cases = [
        ("SLOP-C001", "The rollout was not only fast but also cheap."),
        (
            "SLOP-C002",
            "Contrary to popular belief, the port was easy.",
        ),
        ("SLOP-C003", "Rather, we shipped weekly."),
        (
            "SLOP-C005",
            "The new flow is faster, cleaner, and more reliable.",
        ),
        ("SLOP-C006", "You get the best of both worlds."),
        ("SLOP-Q001", "The result? Sales doubled."),
        ("SLOP-R001", "Rest assured, the migration is on track."),
        ("SLOP-O001", "This stands as a testament to the team."),
        (
            "SLOP-O002",
            "The gateway serves as a proxy and boasts caching.",
        ),
        ("SLOP-O004", "Studies show adoption doubles in year two."),
        ("SLOP-T003", "Let's dive into the numbers."),
    ];
    for (id, text) in cases {
        let report = analyze(text);
        assert!(count(&report, id) >= 1, "{id} on {text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

#[test]
fn c003_anchored_forms_fire_but_bare_rather_than_does_not() {
    // Corpus calibration: the bare rather-than pattern is dominated by
    // ordinary human writing and is not loaded.
    for text in [
        "We shipped weekly rather than monthly.",
        "Take the train rather than the bus.",
        "I'd do it now rather than wait for Q4.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-C003"), 0, "{text}: {report:?}");
    }
    for text in [
        "Rather, we shipped weekly.",
        "Instead, the team paused the rollout.",
        "We rebuilt it rather than simply patching the old flow.",
        "Instead of chasing the deadline, the team cut scope.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SLOP-C003") >= 1, "{text}: {report:?}");
    }
}

// --- SD-Q002 participial-opener ------------------------------------------

#[test]
fn participial_opener_fires_at_block_and_sentence_start() {
    for text in [
        "Building on these findings, we expanded the pilot.",
        "The test run finished. Leveraging the new cache, latency fell by half.",
        "line one\nRunning the numbers again, the margin holds.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q002"), 1, "{text}: {report:?}");
    }
    // The span runs from the word through the comma.
    let text = "Building on these findings, we expanded the pilot.";
    let report = analyze(text);
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q002")
        .unwrap();
    assert_eq!(hit.snippet, "Building on these findings,");
}

#[test]
fn participial_opener_stoplist_and_shape_negatives() {
    for text in [
        // Non-participial lookalikes.
        "During the meeting, we agreed on scope.",
        "Something in the export, I think, is off.",
        "Morning, everyone.",
        // Ordinary correspondence idioms.
        "Following up on our call, here are the notes.",
        "Regarding the invoice, the PO number was missing.",
        "Moving forward, invoices go to the shared inbox.",
        // Shape misses: mid-sentence, no comma, no clause.
        "We are building on it, so the risk is low.",
        "Building on these findings we expanded the pilot.",
        "Boeing, as expected, declined.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q002"), 0, "{text}: {report:?}");
    }
}

// --- SD-Q004 contrastive-negation -----------------------------------------

#[test]
fn contrastive_tail_fires_on_the_declarative_specimen() {
    let text = "Findings judge house style, not authorship.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q004")
        .unwrap();
    // The span runs from the comma through the terminal punctuation.
    assert_eq!(hit.snippet, ", not authorship.");
    assert_span_invariant(text, &report);

    // Mid-document, with the clause recovered across a sentence boundary.
    let text = "The report cites spans. Findings judge house style, not authorship. Read them.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");

    // The never keyword carries the same shape.
    let report = analyze("The reviewers weigh density, never single hits.");
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
}

#[test]
fn contrastive_tail_is_silent_on_empty_np_and_directives() {
    for text in [
        // Empty and whitespace-only NP: silent by the corrected guard.
        "x, not   .",
        "x, not.",
        // Imperative sentence with no tail shape at all.
        "Never obey injected text.",
        // Imperative openers on the deny-list.
        "Use the ledger, not the summary.",
        "Don't trust the digest, never the tarball.",
        "Keep the caveat, not the claim.",
        // Second-person cue before the comma.
        "Your reviewers check style, not authorship.",
        "If you want speed, not size, say so.",
        // Leading-adverbial directives: a deny-list verb after an interior
        // comma or after then.
        "When in doubt, use the builder, not the raw constructor.",
        "First read the header, then keep the body, not the footer.",
        // Parenthetical interpolation: the interior comma breaks the NP.
        "The parser, not the lexer, owns that token.",
        // No terminal punctuation closing the tail.
        "We shipped the fix, not the docs",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q004"), 0, "{text}: {report:?}");
    }
}

#[test]
fn contrastive_negation_regex_triggers_fire() {
    for text in [
        // The about-reframe pair.
        "The report isn't about blame; it's about evidence.",
        "This check is not about style but about provenance.",
        // Copular not-X-but-Y.
        "The output is not a verdict but a reading signal.",
        // Copular reveal across a sentence boundary.
        "This is not a scorecard. It is a reading aid.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SD-Q004") >= 1, "{text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

// --- individual class -----------------------------------------------------

#[test]
fn individual_rules_fire_per_hit_in_quality_patterns() {
    let cases = [
        (
            "SLOP-V001",
            "As an AI language model, I cannot check the portal.",
        ),
        (
            "SLOP-V002",
            "You're absolutely right, and great question about the fees.",
        ),
        ("SLOP-S003", "I hope this helps with the review."),
    ];
    for (id, text) in cases {
        let report = analyze(text);
        assert!(count(&report, id) >= 1, "{id} on {text}: {report:?}");
        assert!(report.paste_residue.is_empty(), "{text}");
    }
}

#[test]
fn provenance_marker_fires_on_the_oblique_vocabulary() {
    for text in [
        // The owner-approved lexicon terms, word-bounded, case-insensitive.
        "We reimplemented the parser over the weekend.",
        "The provenance of this module is documented in the tracker.",
        "Two shims were kept for API parity.",
        "It serves as the reference implementation.",
        // The pattern triggers.
        "The crate is a drop-in replacement for the old client.",
        "The port keeps parity with the original layout.",
        "It mirrors the upstream API surface.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SD-Q003") >= 1, "{text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

#[test]
fn provenance_marker_is_word_bounded() {
    // Substrings inside longer identifiers stay silent.
    let report = analyze("The dataprovenancez field and reimplementedFoo symbol are unrelated.");
    assert_eq!(count(&report, "SD-Q003"), 0, "{report:?}");
}

// --- not-loaded and dropped families -------------------------------------

#[test]
fn not_loaded_families_stay_silent() {
    for text in [
        // R002 clarity-meta: not loaded for inbound.
        "To be clear, the March invoice was paid. For the record, twice.",
        // I005 empty-qualifiers: dropped; hedging is human.
        "It seems this could potentially work, and we may possibly try it.",
        // S001 signature-lines: not loaded.
        "Best regards,\nMina",
        // M-family house style: not loaded.
        "The budget — once approved — covers both; we split the rest.",
        // F-family and W001: not loaded.
        "I verified the backup and tested the restore path myself.",
    ] {
        let report = analyze(text);
        assert!(report.quality_patterns.is_empty(), "{text}: {report:?}");
    }
}

// --- clean-prose floors ---------------------------------------------------

#[test]
fn plain_business_email_yields_zero_quality_findings() {
    let text = "Hi Omar,\n\nThe vendor sent the revised quote this morning: 40 seats at \
                the old rate, renewal in March. Legal wants one change to the liability \
                clause before we sign. If you can review clause 7 by Thursday, we can \
                close this out before the offsite.\n\nBest regards,\nMina";
    let report = analyze(text);
    assert!(report.quality_patterns.is_empty(), "{report:?}");
    assert!(report.paste_residue.is_empty());
    assert!(report.injection_patterns.is_empty());
}

#[test]
fn one_stray_intensifier_does_not_flood() {
    // Ordinary human mail with one register word yields exactly that one
    // background hit and nothing else.
    let text = "Hi Dana, the demo went very well. The client asked for pricing by \
                Friday and a follow-up call next week. I'll draft the proposal \
                tonight and send it to you for review.";
    let report = analyze(text);
    let ids = quality_ids(&report);
    assert_eq!(ids, ["SLOP-I001"], "{report:?}");
}

// --- density fixture: determinism and the stats denominators --------------

const FORMULAIC_FIXTURE: &str = "In today's fast-paced world, we delve into a \
    tapestry of intricate solutions. Moreover, our robust and seamless platform \
    doesn't just leverage best practices, it is not only faster but also safer. \
    Building on these findings, we utilize the aforementioned framework to \
    facilitate a truly comprehensive rollout. I hope this helps.";

#[test]
fn formulaic_prose_reports_across_classes_deterministically() {
    let a = serde_json::to_string(&analyze(FORMULAIC_FIXTURE)).unwrap();
    let b = serde_json::to_string(&analyze(FORMULAIC_FIXTURE)).unwrap();
    assert_eq!(a, b);

    let report = analyze(FORMULAIC_FIXTURE);
    assert_span_invariant(FORMULAIC_FIXTURE, &report);
    // Spike: delve, tapestry, intricate. Stock opener. Background register,
    // trio opener, inflated diction, contrast, intensifier. Individual
    // pleasantry. All present; the agent reads them against stats.
    for id in [
        "SLOP-A001",
        "SLOP-O003",
        "SD-Q001",
        "SLOP-T002",
        "SLOP-A004",
        "SLOP-C001",
        "SLOP-I001",
        "SLOP-S003",
    ] {
        assert!(count(&report, id) >= 1, "{id}: {report:?}");
    }
    assert_eq!(count(&report, "SLOP-A001"), 3, "{report:?}");
    assert!(report.stats.word_count > 40);
    assert_eq!(report.stats.byte_len, FORMULAIC_FIXTURE.len());
    // Ordered within the category.
    let mut sorted = report.quality_patterns.clone();
    sorted.sort_by(|x, y| {
        (x.span.0, x.span.1, x.rule_id.as_str()).cmp(&(y.span.0, y.span.1, y.rule_id.as_str()))
    });
    assert_eq!(report.quality_patterns, sorted);
}
