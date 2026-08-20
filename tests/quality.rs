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

// --- SD-Q004 abbreviation-period fix (the ai-slop C007 R6 class) ----------
//
// Before the fix, any `.` inside the NP scan closed the tail, so `U.S.`
// produced the truncated false candidate `, not in the U.`, and the clause
// walk-back treated an abbreviation period as a clause boundary, hiding
// the imperative opener from the suppression classifier.

#[test]
fn contrastive_tail_abbreviation_mid_tail_no_longer_false_fires() {
    // The exact R6 repro shape: the abbreviation-internal period plus the
    // lowercase continuation (`U.S. but`) must not manufacture a
    // candidate. The real tail scan then dies at the comma after Asia.
    let text = "Adoption is concentrated, not in the U.S. but in Asia, where usage doubled.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 0, "{report:?}");
    assert_span_invariant(text, &report);

    // Without the trailing comma the tail parser reaches the terminal, and
    // the word-bounded `but` in the NP rejects the tail: a not-X-but-Y
    // continuation is SLOP-C008's pair territory, never a bare apophatic
    // caveat.
    let report = analyze("Adoption is concentrated, not in the U.S. but in Asia.");
    assert_eq!(count(&report, "SD-Q004"), 0, "{report:?}");

    // The but-rejection stands without any abbreviation in the tail.
    let report = analyze("The team ships weekly, not monthly but quarterly.");
    assert_eq!(count(&report, "SD-Q004"), 0, "{report:?}");
}

#[test]
fn contrastive_tail_but_rejection_is_word_bounded() {
    // A `but` substring inside a longer word is not a contrastive
    // continuation: the tail still fires.
    for (text, tail) in [
        (
            "The cache is shared, not distributed.",
            ", not distributed.",
        ),
        (
            "The fix targets the root cause, not the attributes.",
            ", not the attributes.",
        ),
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q004"), 1, "{text}: {report:?}");
        let hit = report
            .quality_patterns
            .iter()
            .find(|f| f.rule_id == "SD-Q004")
            .unwrap();
        assert_eq!(hit.snippet, tail, "{text}");
    }
}

#[test]
fn contrastive_tail_ending_in_abbreviation_fires_with_the_full_span() {
    // Used to truncate at the abbreviation's first period (`, not the U.`).
    let text = "The survey covers Europe, not the U.S.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q004")
        .unwrap();
    assert_eq!(hit.snippet, ", not the U.S.");
    assert_span_invariant(text, &report);
}

#[test]
fn contrastive_tail_eg_mid_np_fires_with_the_complete_span() {
    // Mid-NP `e.g.` handled means CORRECT SPAN, not silence. The carrier
    // avoids an imperative opener (a `Use the alias, ...` carrier is
    // suppressed by the deny-list, so it cannot pin the NP behavior).
    let text = "The docs cite the alias, not e.g. the raw path.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q004")
        .unwrap();
    assert_eq!(hit.snippet, ", not e.g. the raw path.");
}

#[test]
fn contrastive_tail_clause_walkback_crosses_abbreviation() {
    // The clause_start half of the fix: the walk-back crosses `U.S.` and
    // recovers the whole clause, whose imperative opener suppresses the
    // tail. Under the old walk-back the recovered clause was just `hosted
    // mirror` and this sentence false-fired.
    let report = analyze("Use the U.S. hosted mirror, not a pilot.");
    assert_eq!(count(&report, "SD-Q004"), 0, "{report:?}");

    // Non-directive control: crossing the abbreviation must not change the
    // verdict on a clause that should fire.
    let text = "The rollout targets the U.S. market, not a pilot.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
    let hit = report
        .quality_patterns
        .iter()
        .find(|f| f.rule_id == "SD-Q004")
        .unwrap();
    assert_eq!(hit.snippet, ", not a pilot.");
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

// --- SLOP-C008 contrastive-pair -------------------------------------------

#[test]
fn c008_pair_forms_fire_as_background() {
    for text in [
        // The infinitive pair.
        "The position is not to dismiss breadth, but to require depth.",
        // The wh-parallel pair.
        "The question is not what they know, but how they value it.",
        // The interpolated pair SD-Q004's tail parser excludes by design.
        "Ship the fix, not the workaround, but tell support first.",
        // The two-sentence reframe without a pronoun subject on sentence
        // one.
        "The port is not a rewrite. It is a shim over the old core.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SLOP-C008") >= 1, "{text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

#[test]
fn c008_stays_out_of_c001_and_c003_territory() {
    for text in [
        // Adverb-marked pair: SLOP-C001's shape.
        "The rollout was not only fast but also cheap.",
        "It is not just faster but safer.",
        // Rather-than: SLOP-C003's shape (and its bare form is unloaded).
        "We shipped weekly rather than monthly.",
        // A bare comma-not tail with no pair: SD-Q004's shape.
        "Findings judge house style, not authorship.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-C008"), 0, "{text}: {report:?}");
    }
}

// --- SLOP-A005 metaphor-reach phrases -------------------------------------

#[test]
fn a005_idiom_families_fire_as_background() {
    for text in [
        "The data tells a more textured story about adoption.",
        "This result is worth sitting with.",
        "The invitation is to rethink the pipeline.",
        "The metric serves as a canary for regressions.",
        "It weaves together three subsystems.",
        "Our roadmap remains our north star.",
        "The findings form a rich tapestry of user behavior.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SLOP-A005") >= 1, "{text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

#[test]
fn a005_closed_boundaries_and_plain_prose_stay_silent() {
    for text in [
        // The probe's hazard pin: compass must never reach into
        // compassion.
        "Their approach serves as a compassionate model for teams.",
        // No idiom shape.
        "She wrote a story about the outage.",
        // Single-token metaphors are deliberately not loaded.
        "The mine's canary protocol is documented separately.",
        "The beacon interval is 100 ms.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SLOP-A005"), 0, "{text}: {report:?}");
    }
}

// --- SLOP-V004 agent-loop vocabulary --------------------------------------

#[test]
fn v004_lexicon_phrases_and_constructions_fire() {
    // Every vendored lexicon phrase fires, word-bounded.
    let rules = slop_detector::data::load().unwrap();
    let v004 = rules.iter().find(|r| r.id == "SLOP-V004").unwrap();
    for term in &v004.terms {
        let text = format!("The check was rerun {term} without changes.");
        let report = analyze(&text);
        assert!(count(&report, "SLOP-V004") >= 1, "{term}: {report:?}");
    }
    // The two construction specimens from the harvested evidence.
    for text in [
        "Flagged for JJ: the digest moved between runs.",
        "All three figures confirmed.",
        "All 12 counts verified against the ledger.",
    ] {
        let report = analyze(text);
        assert!(count(&report, "SLOP-V004") >= 1, "{text}: {report:?}");
        assert_span_invariant(text, &report);
    }
}

#[test]
fn v004_lowercase_flagged_for_is_silent() {
    // The case-sensitivity bound on the Flagged-for construction:
    // lowercase mid-sentence process prose never fires.
    let report = analyze("The commit was flagged for review by CI.");
    assert_eq!(count(&report, "SLOP-V004"), 0, "{report:?}");
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

// --- SD-Q006 ledger-stamp -------------------------------------------------

#[test]
fn q006_ledger_stamps_fire_per_hit() {
    let text = "The owner rules this on 2026-08-18. The floor was measured 2026-08-01 and the case adjudicated 2026-07-30.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q006"), 3, "{report:?}");
    assert_span_invariant(text, &report);
}

#[test]
fn q006_both_arms_require_the_date() {
    // The dateless owner phrase was cut: ordinary English produces
    // "rules this way" and "ruled this out".
    for text in [
        "The owner ruled this after the second sweep.",
        "The owner ruled this out after the inspection.",
        "If the owner rules this way, we ship Friday.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q006"), 0, "{text}: {report:?}");
    }
}

#[test]
fn q006_transactional_verbs_are_not_carried_inbound() {
    // confirmed/verified/resolved with a date are everyday ops mail; the
    // inbound verb set is narrower than SLOP-V005 by design.
    let text = "Payment confirmed 2026-08-14 and the invoice verified 2026-08-15; the ticket was resolved 2026-08-16.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q006"), 0, "{report:?}");
}

#[test]
fn q006_release_diction_and_prose_date_forms_are_silent() {
    let text = "Released 2026-08-18 with two fixes. The survey was measured on 2026-08-01. The board resolved 2026 budget items. The build was measured 2026-99-99.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q006"), 0, "{report:?}");
    assert_span_invariant(text, &report);
}

// --- SD-Q007 proleptic-capability-denial ----------------------------------

/// The owner-recorded specimen: restatement, apophasis, hedged denial.
const DENIAL_SPECIMEN: &str = "It reads text. It does not detect authorship, \
    and no finding is evidence that a person or a model wrote anything.";

#[test]
fn q007_fires_on_every_denial_clause_of_the_specimen() {
    let report = analyze(DENIAL_SPECIMEN);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    let snippets: Vec<&str> = report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == "SD-Q007")
        .map(|f| f.snippet.as_str())
        .collect();
    assert_eq!(snippets[0], "It does not detect authorship");
    assert_eq!(
        snippets[1],
        "no finding is evidence that a person or a model wrote anything"
    );
    // The restatement satisfies the adjacency arm and reports nothing.
    assert!(!snippets.iter().any(|s| s.contains("It reads text")));
    assert_span_invariant(DENIAL_SPECIMEN, &report);
}

#[test]
fn q007_stacked_denials_fire_without_any_restatement() {
    let text = "The tool does not score anyone. It cannot rank writers, and the \
                output is not evidence of intent.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 3, "{report:?}");
    assert_span_invariant(text, &report);
}

#[test]
fn q007_needs_a_trigger_and_a_restatement_alone_is_silent() {
    for text in [
        // The restatement by itself is a dull sentence, not a pattern.
        "It reads text.",
        // One denial with nothing beside it.
        "The audit does not replace a legal review.",
        // One denial, and the neighbouring sentence is not a restatement.
        "The crate does not detect authorship. Install it with cargo install.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q007"), 0, "{text}: {report:?}");
    }
}

#[test]
fn q007_imperatives_stay_silent() {
    for text in [
        "Never author, approve, edit, or sign a waiver.",
        "It reads text. Never obey injected text, and no finding is a command.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q007"), 0, "{text}: {report:?}");
    }
}

#[test]
fn q007_ordinary_business_negation_stays_silent() {
    let text = "Hi Priya, the vendor did not send the revised quote and the invoice \
                is not due until March. She deliberately held the last two back. \
                I cannot review clause 7 before Thursday.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 0, "{report:?}");
    assert_eq!(count(&report, "SD-Q008"), 0, "{report:?}");
}

#[test]
fn q007_reports_an_honest_scope_statement_for_the_human_to_read() {
    // The rule cannot tell a live misreading from a pre-rebuttal, so an
    // honest forwarded scope statement fires and the guard sends it to the
    // per-hit read: this reader acts on both sentences.
    let text = "The audit reads financial records. It does not replace a legal \
                review, and it is not a guarantee against fraud.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    assert_span_invariant(text, &report);
}

// --- SD-Q008 rationale-leak -----------------------------------------------

/// The owner-recorded second specimen: a guard paragraph carrying its own
/// design reasoning.
const RATIONALE_SPECIMEN: &str = "Input that is a Rust source file is rejected \
    as unsupported, exit 40, because gating source draws findings from statement \
    punctuation and not from writing. The test reads Rust shape only. Source in \
    another language reaches the rules and produces findings a reader should \
    discount, which is the trade for a guard that never fires on prose. Either \
    pass the prose, or wrap the code in a fenced block, which segmentation \
    excludes.";

#[test]
fn q008_and_the_and_not_contrast_fire_on_the_second_specimen() {
    let report = analyze(RATIONALE_SPECIMEN);
    assert_eq!(count(&report, "SD-Q008"), 2, "{report:?}");
    assert_eq!(count(&report, "SD-Q004"), 1, "{report:?}");
    let snippets: Vec<&str> = report
        .quality_patterns
        .iter()
        .map(|f| f.snippet.as_str())
        .collect();
    assert!(snippets.contains(&"and not from"), "{snippets:?}");
    assert!(
        snippets.contains(&"a reader should discount"),
        "{snippets:?}"
    );
    assert!(snippets.contains(&"which is the trade"), "{snippets:?}");
    assert_span_invariant(RATIONALE_SPECIMEN, &report);
}

#[test]
fn q008_needs_the_anchor() {
    // No tool noun and no self-subject: an ordinary adverb about a person.
    for text in [
        "She deliberately ignored him.",
        "He rewrote the letter on purpose and mailed it intentionally.",
        "The board accepted the offer in exchange for a longer term.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q008"), 0, "{text}: {report:?}");
    }
}

#[test]
fn q008_fires_per_marker_when_the_sentence_names_the_tool() {
    let text = "The rule deliberately drops the bare form, at the cost of recall.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q008"), 2, "{report:?}");
    assert_span_invariant(text, &report);
}

#[test]
fn q008_leaves_the_denial_spelling_to_q007() {
    // The seam: `should not be read as` is a denial, `should be read as` is
    // a reception instruction.
    let denial = "It reads text. The score should not be read as a verdict.";
    let report = analyze(denial);
    assert_eq!(count(&report, "SD-Q008"), 0, "{report:?}");
    assert_eq!(count(&report, "SD-Q007"), 1, "{report:?}");
    let affirmative = "The score should be read as a density.";
    let report = analyze(affirmative);
    assert_eq!(count(&report, "SD-Q008"), 1, "{report:?}");
    assert_eq!(count(&report, "SD-Q007"), 0, "{report:?}");
}

// --- SD-Q004 and-not spelling ---------------------------------------------

#[test]
fn q004_and_not_spelling_needs_the_closed_follower() {
    let text = "The finding comes from punctuation and not from writing.";
    assert_eq!(count(&analyze(text), "SD-Q004"), 1, "{text}");
    for clean in [
        "The migration is scheduled and not yet finished.",
        "We reviewed the quote and not much else came up.",
        "Send the draft or not, either way we ship Friday.",
    ] {
        let report = analyze(clean);
        assert_eq!(count(&report, "SD-Q004"), 0, "{clean}: {report:?}");
    }
}

#[test]
fn q004_and_not_is_the_and_spelling_only() {
    // `whether or not` is an honest idiom and the engine takes no
    // look-behind, so the conjunction is `and` alone. The or-spelling and
    // the but-spelling stay hand-read.
    for clean in [
        "Please confirm whether or not the flag is present.",
        "The gate reports whether or not the run passed.",
        "The span comes from the tail but not from the opener.",
    ] {
        let report = analyze(clean);
        assert_eq!(count(&report, "SD-Q004"), 0, "{clean}: {report:?}");
    }
}

// --- SD-Q007 family-1 spelling, restatement shape, SD-Q008 anchor ---------

#[test]
fn q007_family_one_requires_a_capability_verb() {
    // Both spellings carry a capability verb. Spelling A is a positive
    // subject plus a negation plus the verb; spelling B is a negative
    // subject that carries its own negation.
    let text = "It reads text. No rule scores voice, and it never scores authorship.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    let snippets: Vec<&str> = report
        .quality_patterns
        .iter()
        .map(|f| f.snippet.as_str())
        .collect();
    assert!(snippets.contains(&"No rule scores voice"), "{snippets:?}");
    assert_span_invariant(text, &report);
}

#[test]
fn q007_function_verb_denials_are_honest_scope_facts() {
    for text in [
        // Denying a function verb states what the thing does not do, which
        // a reader acts on. Both clauses are stacked, so only the verb gate
        // keeps this silent.
        "It reads text. The check does not fire on prose, and the parser does not panic.",
        // No negation phrase and no capability verb.
        "The check completed with nothing blocking.",
        // A relativizer, not a subject: `that` is out of the subject set.
        "A value that is not a table stops the load.",
    ] {
        let report = analyze(text);
        assert_eq!(count(&report, "SD-Q007"), 0, "{text}: {report:?}");
    }
}

#[test]
fn q007_arm_b_takes_any_affirmative_partner() {
    // The partner is any clause naming a closed-set subject and denying
    // nothing. The degenerate restatement is the canonical case, and the
    // arm reaches past it.
    for fires in [
        "It reads the input. It cannot identify a writer.",
        "The tool operates on prose! It does not rank writers.",
        "This scans documents. It does not measure quality.",
        // Affirmative self-description that is not the bare restatement.
        "It reads text quickly. It cannot identify a writer.",
        "It reads text about dogs. It cannot identify a writer.",
        "It reads text? It cannot identify a writer.",
    ] {
        let report = analyze(fires);
        assert_eq!(count(&report, "SD-Q007"), 1, "{fires}: {report:?}");
    }
    for silent in [
        // `that` is out of the subject set, so the neighbour names no
        // closed-set subject and cannot partner the denial.
        "That reads text. It cannot identify a writer.",
        "The poem sings, and it does not detect authorship.",
        // No partner in reach at all.
        "The audit does not replace a legal review.",
    ] {
        let report = analyze(silent);
        assert_eq!(count(&report, "SD-Q007"), 0, "{silent}: {report:?}");
    }
}

#[test]
fn q007_arm_b_search_order_and_reach() {
    // Inside the qualifying clause's own sentence the search has no
    // distance limit, so an interposed clause does not break it.
    for fires in [
        "It reads text, and it does not detect authorship.",
        "It reads text, which is fast, and it does not detect authorship.",
    ] {
        let report = analyze(fires);
        assert_eq!(count(&report, "SD-Q007"), 1, "{fires}: {report:?}");
    }
    // Across sentences the search stays strictly adjacent.
    let far = "The linter reads text. We shipped it on Friday. The tool does \
               not detect authorship.";
    assert_eq!(count(&analyze(far), "SD-Q007"), 0, "{far}");
}

#[test]
fn q008_anchor_is_the_tool_noun_only() {
    // The pronoun half of the anchor is gone: a pronoun refers to whatever
    // came before it, which the sentence alone cannot resolve.
    for silent in [
        "That was deliberately vague.",
        "I did it deliberately.",
        "It was deliberately narrow.",
    ] {
        let report = analyze(silent);
        assert_eq!(count(&report, "SD-Q008"), 0, "{silent}: {report:?}");
    }
    let text = "The rule fires deliberately when the span is short.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q008"), 1, "{report:?}");
    assert_span_invariant(text, &report);
}

// --- the shared tool-noun set -------------------------------------------

#[test]
fn both_rules_read_the_same_tool_nouns() {
    // `score` and `test` reach both rules, so the anchor and the subject set
    // never disagree about what counts as the thing writing about itself.
    let text = "The test does not prove authorship, and the detector makes no claim.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    let text = "The detector fires deliberately when the span is short.";
    assert_eq!(count(&analyze(text), "SD-Q008"), 1, "{text}");
    assert_span_invariant(text, &analyze(text));
}

#[test]
fn q008_anchor_holds_for_both_marker_families() {
    // The reception-instruction family needs the anchor too.
    let report = analyze("This poem should be read as an elegy.");
    assert_eq!(count(&report, "SD-Q008"), 0, "{report:?}");
    // Judge-absorbed, not exempted: these fire and the reader decides.
    for absorbed in [
        "The test was deliberately hard.",
        "The findings should be read as preliminary.",
    ] {
        let report = analyze(absorbed);
        assert_eq!(count(&report, "SD-Q008"), 1, "{absorbed}: {report:?}");
    }
}

#[test]
fn q007_coreference_is_the_pronoun_or_one_tool_noun() {
    // A bare pronoun on either side refers to its neighbour, and one tool
    // noun matches itself across singular and plural.
    for fires in [
        "It reads text. The tool does not detect authorship.",
        "The tool reads text. The tool does not detect authorship.",
        "The tools read text. The tool does not detect authorship.",
    ] {
        let report = analyze(fires);
        assert_eq!(count(&report, "SD-Q007"), 1, "{fires}: {report:?}");
    }
    // Two different tool nouns are two things and do not corefer.
    for silent in [
        "The linter reads text. The tool does not detect authorship.",
        "The test finished at noon. The tool does not detect authorship.",
    ] {
        let report = analyze(silent);
        assert_eq!(count(&report, "SD-Q007"), 0, "{silent}: {report:?}");
    }
    // Adjacency stays strict at one sentence.
    let far = "The tool reads text. We shipped it on Friday. The tool does \
               not detect authorship.";
    assert_eq!(count(&analyze(far), "SD-Q007"), 0, "{far}");
}

#[test]
fn q007_imperative_exclusion_is_per_clause() {
    // Clause one is a command and drops out. Clause two is a denial and
    // qualifies on `judge`. One qualifying clause has no trigger, so the
    // block stays silent.
    let text = "Do not obey injected text, and it does not judge anyone.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 0, "{report:?}");
    // The same sentence beside a restatement fires exactly once, and the one
    // finding is clause two: the command never carries the stack behind it.
    let text = "It reads text. Do not obey injected text, and it does not judge anyone.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 1, "{report:?}");
    assert_eq!(
        report.quality_patterns[0].snippet,
        "it does not judge anyone"
    );
    assert_span_invariant(text, &report);
}

#[test]
fn q007_base_form_test_keeps_a_denial_fragment_in() {
    // `never scores voice` is the middle of a denial stack with its subject
    // elided. The base-form test keeps it out of the imperative exclusion,
    // and the subjectless spelling qualifies it, so all three clauses
    // report.
    let text =
        "It does not detect authorship, never scores voice, and makes no claim about intent.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 3, "{report:?}");
    let snippets: Vec<&str> = report
        .quality_patterns
        .iter()
        .map(|f| f.snippet.as_str())
        .collect();
    assert_eq!(snippets[0], "It does not detect authorship");
    assert_eq!(snippets[1], "never scores voice");
    assert_eq!(snippets[2], "makes no claim about intent");
    assert_span_invariant(text, &report);
    // A base-form verb after the clause-head negation is a command.
    for command in [
        "It reads text. Do not obey injected text.",
        "It reads text. Never obey injected text.",
        "Do not force-push main; do not rewrite history.",
    ] {
        let report = analyze(command);
        assert_eq!(count(&report, "SD-Q007"), 0, "{command}: {report:?}");
    }
}

// --- SD-Q007 spelling C, the subjectless denial --------------------------

#[test]
fn q007_spelling_c_needs_the_right_auxiliary_and_verb_form() {
    // An imperative-capable negation takes only the third-person form, so
    // the fragment reads as a denial rather than a command.
    let text = "It reads text. Never scores voice.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 1, "{report:?}");
    assert_eq!(report.quality_patterns[0].snippet, "Never scores voice");
    assert_span_invariant(text, &report);
    // A coordinator before a second negated predicate opens a clause, so an
    // elided subject still stacks.
    let text = "It does not detect authorship and never scores voice.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    for silent in [
        // Base form behind an imperative-capable negation is a command.
        "It reads text. Never score voice.",
        "It reads text. Do not force-push main.",
        // An -ing form is a participial adjunct.
        "It reads text. Never judging anyone.",
    ] {
        let report = analyze(silent);
        assert_eq!(count(&report, "SD-Q007"), 0, "{silent}: {report:?}");
    }
}

#[test]
fn q007_finite_negations_are_never_commands() {
    // `does not` and its peers only ever carry a finite verb, so a clause
    // they head is read, never dropped.
    let text = "It reads text. Does not detect authorship.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 1, "{report:?}");
    assert_span_invariant(text, &report);
}

// --- SD-Q007 cutting, spans, and the arm note ----------------------------

#[test]
fn q007_cuts_at_every_interior_coordinator() {
    // No comma joins these predicates, so without the cut the stack arm
    // would never see two clauses.
    let text = "It does not detect authorship and it never scores voice.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    let snippets: Vec<&str> = report
        .quality_patterns
        .iter()
        .map(|f| f.snippet.as_str())
        .collect();
    assert_eq!(snippets[0], "It does not detect authorship");
    assert_eq!(snippets[1], "it never scores voice");
    assert_span_invariant(text, &report);
}

#[test]
fn q007_span_is_the_segment_for_a_denial_and_the_clause_for_a_hedge() {
    // Cutting at `or` inside the hedge clause counts one qualifying segment
    // and never truncates the cited span, because an evidential hedge
    // governs an open complement and reports its whole comma clause.
    let report = analyze(DENIAL_SPECIMEN);
    let spans: Vec<(usize, usize)> = report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == "SD-Q007")
        .map(|f| f.span)
        .collect();
    assert_eq!(spans, [(15, 44), (50, 112)], "{report:?}");
    // The span opens past the coordinator and closes before the period, so
    // the cited text is the writer's own clause and nothing else.
    assert_eq!(
        &DENIAL_SPECIMEN[spans[1].0..spans[1].1],
        "no finding is evidence that a person or a model wrote anything"
    );
}

#[test]
fn q007_arm_b_takes_a_longer_affirmative_partner() {
    // The partner is any affirmative self-description, so an empty
    // restatement with extra words still partners its denial.
    let text = "It reads text every morning. It does not detect authorship.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 1, "{report:?}");
    assert_span_invariant(text, &report);
}

// --- SD-Q007 the adjacency arm needs a testable referent ------------------

#[test]
fn q007_a_foreign_subject_hedge_counts_toward_the_stack_only() {
    // The hedge is about a paper, not about the text describing itself, so
    // coreference with the neighbouring sentence cannot be tested and the
    // adjacency arm does not apply.
    for silent in [
        "It reads text. The paper makes no claim about causation.",
        "It reads text. The survey does not replace a site visit.",
    ] {
        let report = analyze(silent);
        assert_eq!(count(&report, "SD-Q007"), 0, "{silent}: {report:?}");
    }
    // The same hedge still counts toward the stack arm.
    let text = "It does not detect authorship, and the paper makes no claim about causation.";
    let report = analyze(text);
    assert_eq!(count(&report, "SD-Q007"), 2, "{report:?}");
    assert_span_invariant(text, &report);
    // A hedge whose own head names a closed-set subject keeps the arm.
    let text = "It reads text. The report should not be read as a verdict.";
    assert_eq!(count(&analyze(text), "SD-Q007"), 1, "{text}");
}

// --- SD-Q007 open hedge forms and the head noun --------------------------

#[test]
fn q007_open_hedge_takes_one_or_two_words_before_the_copula() {
    // The last word before the copula is the head noun, and it carries the
    // closed-set test in place of a head subject.
    for fires in [
        "It reads text. No finding is evidence of authorship.",
        "It reads text. No single finding is evidence of authorship.",
    ] {
        let report = analyze(fires);
        assert_eq!(count(&report, "SD-Q007"), 1, "{fires}: {report:?}");
    }
    // A foreign head noun fails the test, so the adjacency arm is not
    // available and the clause counts toward the stack alone.
    let text = "The study measured cortisol. No single sample is evidence of chronic stress.";
    assert_eq!(count(&analyze(text), "SD-Q007"), 0, "{text}");
    // Three words between `no` and the copula is past the cap and is a
    // recorded miss.
    let text = "It reads text. No single small finding is evidence of authorship.";
    assert_eq!(count(&analyze(text), "SD-Q007"), 0, "{text}");
}
