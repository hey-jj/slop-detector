//! Bundle-mode pins: the `analyze_bundle` entry point, cross-file
//! duplication detection, the within-file/cross-file split, and the
//! guarantee that each per-file report is identical to the single-document
//! `analyze` output for the same text.

use slop_detector::{analyze, analyze_bundle, Container, EvidenceReport, Finding};

const SHARED: &str = "Our platform unifies ingestion, storage, and search behind one binding \
                      contract for every downstream team.";

fn q005(report: &EvidenceReport) -> Vec<&Finding> {
    report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == "SD-Q005")
        .collect()
}

fn docs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(p, t)| (p.to_string(), t.to_string()))
        .collect()
}

/// Two files sharing one 16-word block report exactly one cross-file entry
/// with both paths and correct spans, and neither file reports the shared
/// block as within-file duplication.
#[test]
fn shared_block_across_two_files_reports_one_cross_file_entry() {
    let a = format!("Deck variant one opens differently.\n\n{SHARED}\n");
    let b = format!("{SHARED}\n\nDeck variant two closes differently.\n");
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b)]));

    assert_eq!(bundle.files.len(), 2);
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    assert_eq!(entry.occurrences.len(), 2);
    let paths: Vec<&str> = entry.occurrences.iter().map(|o| o.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "b.txt"]);
    for occ in &entry.occurrences {
        let text = if occ.path == "a.txt" { &a } else { &b };
        let slice = &text[occ.span.0..occ.span.1];
        assert!(slice.starts_with("Our platform unifies"), "{occ:?}");
        assert!(slice.ends_with("downstream team"), "{occ:?}");
    }
    assert!(entry.snippet.starts_with("Our platform unifies"));
    assert!(!entry.snippet_truncated);

    // The shared block appears once per file: no within-file SD-Q005.
    for f in &bundle.files {
        assert_eq!(q005(&f.report).len(), 0, "{}", f.path);
    }
}

/// Disjoint files report no cross-file duplication.
#[test]
fn disjoint_files_report_no_cross_file_duplication() {
    let bundle = analyze_bundle(&docs(&[
        (
            "a.txt",
            "The first document talks about invoices and renewal dates for the quarter.",
        ),
        (
            "b.txt",
            "The second document covers hiring plans and office moves for next year.",
        ),
    ]));
    assert!(bundle.cross_file_duplication.is_empty(), "{bundle:?}");
}

/// Three files sharing the block group into ONE entry with three
/// occurrences, anchored on the first file.
#[test]
fn three_way_share_groups_into_one_entry() {
    let mk = |head: &str| format!("{head}\n\n{SHARED}\n");
    let a = mk("Variant a.");
    let b = mk("Variant b.");
    let c = mk("Variant c.");
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b), ("c.txt", &c)]));
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    let paths: Vec<&str> = entry.occurrences.iter().map(|o| o.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "b.txt", "c.txt"]);
}

/// Within-file repeats stay in the file's own SD-Q005 findings and never
/// surface at bundle level.
#[test]
fn within_file_repeats_do_not_leak_to_bundle_level() {
    let a = format!("{SHARED} Filler sentence with several distinct words here.\n\n{SHARED}\n");
    let b = "A wholly different second file with no repeated material at all.".to_string();
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b)]));
    assert!(bundle.cross_file_duplication.is_empty(), "{bundle:?}");
    assert_eq!(q005(&bundle.files[0].report).len(), 1, "{bundle:?}");
    assert_eq!(q005(&bundle.files[1].report).len(), 0);
}

/// Raw bytes, no segmentation, in bundle mode too: a >=10-word passage
/// duplicated inside code fences in two files reports as cross-file
/// duplication, and each occurrence carries the `fenced-code` container
/// annotation. Annotate, never skip.
#[test]
fn fenced_duplicate_reports_cross_file_with_container_annotation() {
    let block = "let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; let f = 6;";
    let a = format!("Alpha file opens here.\n\n```\n{block}\n```\n");
    let b = format!("Totally different beta preamble.\n\n```\n{block}\n```\n");
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b)]));
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    assert_eq!(entry.occurrences.len(), 2);
    for occ in &entry.occurrences {
        assert_eq!(occ.container, Container::FencedCode, "{occ:?}");
        let text = if occ.path == "a.txt" { &a } else { &b };
        assert!(
            text[occ.span.0..occ.span.1].starts_with("let a = 1"),
            "{occ:?}"
        );
    }
}

/// The cross-file prefix-decoy regression: a first file sharing only the
/// 8-word shingle prefix (diverging below the 10-word floor) must not
/// mask the genuine duplicate between the two later files.
#[test]
fn cross_file_prefix_decoy_does_not_mask_the_later_pair() {
    let decoy = "alpha beta gamma delta epsilon zeta eta theta wrong ending.";
    let full = "alpha beta gamma delta epsilon zeta eta theta iota kappa.";
    let bundle = analyze_bundle(&docs(&[("d.txt", decoy), ("a.txt", full), ("b.txt", full)]));
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    let paths: Vec<&str> = entry.occurrences.iter().map(|o| o.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "b.txt"], "the decoy file is not a copy");
    for occ in &entry.occurrences {
        assert_eq!(
            &full[occ.span.0..occ.span.1],
            "alpha beta gamma delta epsilon zeta eta theta iota kappa",
            "the full 10-word run, not the 8-word prefix"
        );
    }
}

/// The cross-file maximal-run ranking regression, mirroring the
/// single-file shape: the earlier file carries the 13-word passage, four
/// 33-deep decoy walls that exhaust the capped walk at the later file's
/// first four windows, and a 10-word rival gluing the passage tail to the
/// word after the later copy. At the deciding anchor the rival ranks 10
/// forward against the genuine copy's 9, but the genuine candidate's
/// TOTAL run is 13 — the bundle entry must span the full 13 words in both
/// files.
#[test]
fn cross_file_lower_forward_candidate_with_longer_total_run_wins() {
    let full = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu";
    let mut a = format!("{full}. ");
    for i in 0..33 {
        a.push_str(&format!(
            "alpha beta gamma delta epsilon zeta eta theta aone{i}. "
        ));
    }
    for i in 0..33 {
        a.push_str(&format!(
            "beta gamma delta epsilon zeta eta theta iota btwo{i}. "
        ));
    }
    for i in 0..33 {
        a.push_str(&format!(
            "gamma delta epsilon zeta eta theta iota kappa cthree{i}. "
        ));
    }
    for i in 0..33 {
        a.push_str(&format!(
            "delta epsilon zeta eta theta iota kappa lambda dfour{i}. "
        ));
    }
    a.push_str("epsilon zeta eta theta iota kappa lambda mu nu tail.");
    let b = format!("{full} tail.");
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b)]));
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    assert_eq!(entry.occurrences.len(), 2);
    for occ in &entry.occurrences {
        let text = if occ.path == "a.txt" { &a } else { &b };
        assert_eq!(
            &text[occ.span.0..occ.span.1],
            full,
            "the entry must span the maximal 13-word total run: {occ:?}"
        );
    }
}

/// Two bundle entries sharing one path label must not panic and must
/// slice each snippet from the document the occurrence actually came
/// from, resolved by file index, never by re-finding the path string.
#[test]
fn duplicate_path_labels_never_panic_and_snippets_are_correct() {
    let short = "A short unrelated first file.";
    let long = format!(
        "Filler paragraph one has distinct words here. Filler paragraph two also has \
         other distinct words. Filler paragraph three keeps the shared block's offset \
         beyond the first file's length.\n\n{SHARED}\n"
    );
    let other = format!("{SHARED}\n");
    let bundle = analyze_bundle(&docs(&[
        ("dup.txt", short),
        ("dup.txt", &long),
        ("other.txt", &other),
    ]));
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    assert!(
        entry.snippet.starts_with("Our platform unifies"),
        "{entry:?}"
    );
    assert_eq!(entry.occurrences.len(), 2);
    assert_eq!(entry.occurrences[0].path, "dup.txt");
    assert_eq!(entry.occurrences[1].path, "other.txt");
    // The dup.txt occurrence belongs to the SECOND dup.txt entry: its span
    // slices `long` (and would be out of bounds in `short`).
    let (s, e) = entry.occurrences[0].span;
    assert!(
        e > short.len(),
        "the span must come from the second dup.txt"
    );
    assert!(long[s..e].starts_with("Our platform unifies"));
    let (s, e) = entry.occurrences[1].span;
    assert!(other[s..e].starts_with("Our platform unifies"));
}

/// A passage shared by more than 21 files stays ONE entry retaining every
/// occurrence: grouping happens before the cap, and the cap limits
/// entries, never a grouped entry's occurrence list.
#[test]
fn passage_shared_by_more_than_21_files_keeps_every_occurrence() {
    let pairs: Vec<(String, String)> = (0..23)
        .map(|i| {
            (
                format!("f{i:02}.txt"),
                format!("Variant number{i}.\n\n{SHARED}\n"),
            )
        })
        .collect();
    let bundle = analyze_bundle(&pairs);
    assert_eq!(bundle.cross_file_duplication.len(), 1, "{bundle:?}");
    let entry = &bundle.cross_file_duplication[0];
    assert_eq!(
        entry.occurrences.len(),
        23,
        "every occurrence must be retained: {entry:?}"
    );
    let paths: Vec<&str> = entry.occurrences.iter().map(|o| o.path.as_str()).collect();
    let expected: Vec<String> = (0..23).map(|i| format!("f{i:02}.txt")).collect();
    assert_eq!(paths, expected, "occurrences in file order");
    for (occ, (_, text)) in entry.occurrences.iter().zip(&pairs) {
        assert!(text[occ.span.0..occ.span.1].starts_with("Our platform unifies"));
    }
}

/// Each per-file report is identical to the single-document `analyze`
/// output for the same text — bundle mode adds evidence on top, it never
/// changes the single-document contract.
#[test]
fn per_file_reports_equal_single_document_analyze() {
    let a = format!("We delve into the numbers.\n\n{SHARED}\n");
    let b = format!("{SHARED}\n\nRest assured, the totals held.\n");
    let bundle = analyze_bundle(&docs(&[("a.txt", &a), ("b.txt", &b)]));
    for (path, text) in [("a.txt", &a), ("b.txt", &b)] {
        let single = analyze(text);
        let in_bundle = &bundle.files.iter().find(|f| f.path == path).unwrap().report;
        assert_eq!(in_bundle, &single, "{path}");
        assert_eq!(
            serde_json::to_string(in_bundle).unwrap(),
            serde_json::to_string(&single).unwrap(),
            "{path}: byte-identical serialization"
        );
    }
}

/// Bundle output is deterministic across runs.
#[test]
fn bundle_report_is_deterministic() {
    let a = format!("Variant one.\n\n{SHARED}\n");
    let b = format!("Variant two.\n\n{SHARED}\n");
    let d = docs(&[("a.txt", &a), ("b.txt", &b)]);
    let x = serde_json::to_string(&analyze_bundle(&d)).unwrap();
    let y = serde_json::to_string(&analyze_bundle(&d)).unwrap();
    assert_eq!(x, y);
}
