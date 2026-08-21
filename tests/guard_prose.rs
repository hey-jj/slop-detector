//! The guard-prose gate. Every rule in the inbound table carries prose a
//! reader eventually sees, through the agent skill, and that prose holds to
//! the same writing standard as the shipped documents. This gate is the
//! mechanical half of that standard: the classes a reader should never have
//! to catch by hand.
//!
//! Field-scoped on purpose. It reads the prose fields alone, `guard` and
//! `weight`, and never a pattern, a range, a codepoint list, or a lexicon.
//! A regex character class is not prose and never reaches this gate.
//!
//! Why `weight` is in scope. The gate covers the guard and judge prose of
//! whatever schema it runs against. This schema carries no `judge` field,
//! and `weight` is declared on `RuleSpec` as a note carried to the skill
//! and never evaluated by the engine, which is `guard`'s standing word for
//! word. Scanning it applies that scope to the fields this table actually
//! has. A reviewer comparing this gate against a sibling repo's will find
//! one more field named here and the same rule behind it.

use slop_detector::data::{load, Rule};
use std::collections::HashSet;

/// Punctuation the prose never carries. No exemption reaches these: a
/// declared term cannot make a semicolon into something else.
const PUNCTUATION: &[(&str, &str)] = &[
    ("\u{2014}", "em dash"),
    ("\u{2013}", "en dash"),
    (";", "semicolon"),
];

/// Contrast scaffolding: the shapes that set up a foil instead of saying
/// what happens.
const CONTRAST_SHAPES: &[&str] = &[
    "not just",
    "not merely",
    "not only",
    "rather than",
    "instead of",
    "is not about",
    "isn't about",
    "less about",
    "more about",
    "it is not that",
    "it's not that",
    "think of it less as",
];

/// Filler the house style does not use. A rule that names one of these as a
/// term it matches is quoting, and the mention exemption covers that.
const FILLER: &[&str] = &[
    "delve",
    "tapestry",
    "seamless",
    "seamlessly",
    "robust",
    "leverage",
    "myriad",
    "plethora",
    "utilize",
    "crucial",
    "vital",
    "comprehensive",
    "intricate",
    "meticulous",
    "testament",
    "showcase",
    "underscore",
    "underscores",
    "elevate",
    "unlock",
    "empower",
    "foster",
    "streamline",
    "realm",
    "landscape",
    "furthermore",
    "moreover",
    "additionally",
    "it is important to note",
    "it's important to note",
    "it is worth noting",
    "it's worth noting",
    "in today's",
];

/// Every literal the table declares anywhere: lexicon terms, stop-lists,
/// exemption phrases, the clause-shape word sets, and the alphabetic runs
/// written into the regex sources. A guard naming one of these is quoting
/// the thing the rule matches, which is the one honest reason for the word
/// to appear.
fn declared_terms(rules: &[Rule]) -> HashSet<String> {
    let mut out = HashSet::new();
    for r in rules {
        let lists = [
            &r.terms,
            &r.exemptions,
            &r.stoplist,
            &r.second_person,
            &r.subjects,
            &r.determiners,
            &r.negative_subjects,
            &r.negative_determiners,
            &r.tool_nouns,
            &r.coordinators,
            &r.imperative_negations,
            &r.finite_negations,
            &r.capability_verbs_base,
            &r.capability_verbs_s,
            &r.capability_verbs_ing,
            &r.hedges,
            &r.markers,
        ];
        for list in lists {
            out.extend(list.iter().map(|t| t.to_lowercase()));
        }
        // A regex declares its literals inline, so the alphabetic runs in
        // the pattern source are terms the rule matches too.
        for p in &r.patterns {
            let mut word = String::new();
            for c in p.chars() {
                if c.is_ascii_alphabetic() {
                    word.push(c.to_ascii_lowercase());
                } else {
                    if word.len() >= 3 {
                        out.insert(std::mem::take(&mut word));
                    }
                    word.clear();
                }
            }
            if word.len() >= 3 {
                out.insert(word);
            }
        }
    }
    out
}

/// Whether `hay[at..at + needle_len]` stands on its own rather than inside a
/// longer word, so `vital` does not match through `vitally`.
fn standalone(hay: &str, at: usize, len: usize) -> bool {
    let before = hay[..at].chars().next_back();
    let after = hay[at + len..].chars().next();
    let free = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '\'');
    free(before) && free(after)
}

/// Every violation in one prose field, as reader-facing lines. Split out
/// from the table walk so the negative control can drive it directly.
fn violations_in(id: &str, field: &str, prose: &str, terms: &HashSet<String>) -> Vec<String> {
    let mut found = Vec::new();
    for (mark, name) in PUNCTUATION {
        if prose.contains(mark) {
            found.push(format!("{id} {field}: {name}"));
        }
    }
    let lower = prose.to_lowercase();
    for (class, list) in [
        ("contrast scaffolding", CONTRAST_SHAPES),
        ("filler", FILLER),
    ] {
        for banned in list {
            let mut at = 0usize;
            while let Some(pos) = lower[at..].find(banned) {
                let s = at + pos;
                at = s + 1;
                if !standalone(&lower, s, banned.len()) {
                    continue;
                }
                // The mention exemption: the rule is quoting a term the
                // table declares, which is why the word is on the page.
                if terms.contains(*banned) {
                    continue;
                }
                found.push(format!("{id} {field}: {class} \"{banned}\""));
            }
        }
    }
    found
}

#[test]
fn every_guard_holds_to_the_writing_standard() {
    let rules = load().expect("the embedded table loads");
    let terms = declared_terms(&rules);
    let mut found = Vec::new();
    for r in &rules {
        found.extend(violations_in(&r.id, "guard", &r.guard, &terms));
        if let Some(weight) = &r.weight {
            found.extend(violations_in(&r.id, "weight", weight, &terms));
        }
    }
    assert!(
        found.is_empty(),
        "guard prose carries {} mechanical violations:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn the_gate_catches_a_violating_guard() {
    // The negative control. Without it a gate that matches nothing looks
    // exactly like a tree that is clean.
    let terms = HashSet::new();
    let bad = "The rule is robust \u{2014} not just a check \u{2013} and it is \
               crucial; furthermore it fosters a seamless read.";
    let found = violations_in("SD-TEST", "guard", bad, &terms);
    for want in [
        "em dash",
        "en dash",
        "semicolon",
        "contrast scaffolding \"not just\"",
        "filler \"robust\"",
        "filler \"crucial\"",
        "filler \"furthermore\"",
        "filler \"seamless\"",
    ] {
        assert!(
            found.iter().any(|f| f.contains(want)),
            "the gate missed {want}: {found:?}"
        );
    }
    // Every line names the rule so a failure points somewhere.
    assert!(found.iter().all(|f| f.starts_with("SD-TEST guard: ")));
}

#[test]
fn the_mention_exemption_covers_a_quoted_term_and_never_punctuation() {
    let terms: HashSet<String> = ["delve", "seamless"]
        .iter()
        .map(|t| t.to_string())
        .collect();
    // A guard quoting a term the table declares is doing its job.
    let quoting = "The measured set: the delve forms, and seamless.";
    assert!(violations_in("SD-TEST", "guard", quoting, &terms).is_empty());
    // A term that is not declared still fails.
    let not_quoting = "The check is robust.";
    assert_eq!(
        violations_in("SD-TEST", "guard", not_quoting, &terms).len(),
        1
    );
    // Punctuation is out of the exemption's reach whatever is declared.
    let punctuated = "The delve forms load; the seamless entry does not.";
    let found = violations_in("SD-TEST", "guard", punctuated, &terms);
    assert_eq!(found, ["SD-TEST guard: semicolon"], "{found:?}");
}

#[test]
fn the_gate_reads_prose_fields_only() {
    // A regex character class carries semicolons and dashes as data. The
    // gate never looks at one, so the table's own patterns cannot fail it.
    let rules = load().expect("the embedded table loads");
    let pattern_punctuation = rules
        .iter()
        .flat_map(|r| &r.patterns)
        .any(|p| p.contains(';') || p.contains('-'));
    assert!(
        pattern_punctuation,
        "the table should carry punctuation inside patterns for this to mean anything"
    );
    let terms = declared_terms(&rules);
    for r in &rules {
        assert!(
            violations_in(&r.id, "guard", &r.guard, &terms).is_empty(),
            "{}",
            r.id
        );
    }
}
