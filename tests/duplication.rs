//! SD-Q005 self-duplication pins: the shingle floor, the emission cap, the
//! raw-byte stance on fenced content (tokenize and annotate, never skip),
//! the annotate-never-suppress stance on quoted copies, the prefix-decoy
//! and maximal-run regressions, clean-fixture silence, determinism, and
//! the worst-case memory budget of the memory-frugal tokenizer.

use slop_detector::{analyze, Container, EvidenceReport, Finding};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Peak-tracking wrapper around the system allocator, powering the
/// worst-case memory-budget pin below. Relaxed atomics: the counters are
/// statistics, not synchronization, and an off-by-a-few-bytes race cannot
/// move a 100 MiB assertion.
struct PeakAlloc;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn track_alloc(n: usize) {
    let cur = CURRENT.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(cur, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for PeakAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            track_alloc(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                track_alloc(new_size - layout.size());
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

const PARA: &str = "The gate runs the full policy over every draft before it ships to a reader.";

fn q005(report: &EvidenceReport) -> Vec<&Finding> {
    report
        .quality_patterns
        .iter()
        .filter(|f| f.rule_id == "SD-Q005")
        .collect()
}

/// A verbatim restated paragraph (14 words, above the 10-word floor) fires
/// exactly once, on the SECOND copy.
#[test]
fn q005_restated_paragraph_fires_once_on_the_second_copy() {
    let text = format!(
        "{PARA} Filler sentence one sits here. Filler sentence two follows with other words.\n\n{PARA}\n"
    );
    let report = analyze(&text);
    let hits = q005(&report);
    assert_eq!(hits.len(), 1, "exactly one duplication finding: {report:?}");
    let (start, end) = hits[0].span;
    let first_end = text.find(" Filler").unwrap();
    assert!(
        start > first_end,
        "the reported span must be the second copy (start {start} <= first copy end {first_end})"
    );
    assert!(text[start..end].starts_with("The gate runs"));
    assert_eq!(&text[start..end], hits[0].snippet);
}

/// The 10-word floor excludes a 9-word repeat.
#[test]
fn q005_nine_word_repeat_is_below_the_floor() {
    let nine = "The gate runs the policy over every draft twice.";
    let text = format!("{nine} Middle text sits here with several other words.\n\n{nine}\n");
    let report = analyze(&text);
    assert_eq!(q005(&report).len(), 0, "the floor moved: {report:?}");
}

/// A third verbatim occurrence is its own finding: one hit per repeat
/// occurrence, second and later.
#[test]
fn q005_third_occurrence_reports_again() {
    let text =
        format!("{PARA} Distinct filler follows here.\n\n{PARA} More distinct filler.\n\n{PARA}\n");
    let report = analyze(&text);
    assert_eq!(
        q005(&report).len(),
        2,
        "second and third copies each report: {report:?}"
    );
}

/// The `max_reports` cap under a degenerate repeated-stem input: 26 copies
/// (25 repeat occurrences) clip to exactly 20 findings.
#[test]
fn q005_emission_cap_is_respected() {
    let mut text = String::new();
    for i in 0..26 {
        text.push_str(&format!(
            "Verified against the frozen policy digest and the recorded manifest entry today m{i}.\n\n"
        ));
    }
    let report = analyze(&text);
    assert_eq!(q005(&report).len(), 20, "25 repeats must clip to the cap");
}

/// Raw bytes, no segmentation: a >=10-word passage duplicated inside two
/// code fences TOKENIZES like every other slop-detector rule's input and
/// reports SD-Q005 with the `container = fenced-code` annotation. The
/// container pre-pass labels the reported run.
#[test]
fn q005_fenced_duplicate_reports_with_fenced_code_container() {
    let block = "use std::io; use std::fmt; use std::mem; use std::ops; use std::cmp; extra tokens here now";
    let text = format!("```\n{block}\n```\n\nProse between the fences.\n\n```\n{block}\n```\n");
    let report = analyze(&text);
    let hits = q005(&report);
    assert_eq!(
        hits.len(),
        1,
        "the duplicated fenced passage must report: {report:?}"
    );
    assert_eq!(hits[0].container, Container::FencedCode);
    let (start, end) = hits[0].span;
    assert!(
        start > text.find("Prose between").unwrap(),
        "the reported span must be the second fenced copy"
    );
    assert!(text[start..end].starts_with("use std"));
}

/// The prefix-decoy regression (shared with ai-slop's SLOP-U001 fix): an
/// early occurrence that shares the 8-word shingle prefix but diverges
/// below the 10-word floor must not hold the hash slot and mask the
/// genuine duplicate between the two later full copies. The chain walk
/// pairs copy 3 with copy 2 and reports the full 10-word run.
#[test]
fn q005_prefix_decoy_does_not_mask_the_later_duplicate() {
    let text = "alpha beta gamma delta epsilon zeta eta theta wrong ending. \
                amber bronze copper. \
                alpha beta gamma delta epsilon zeta eta theta iota kappa. \
                ivory jade silver. \
                alpha beta gamma delta epsilon zeta eta theta iota kappa.";
    let report = analyze(text);
    let hits = q005(&report);
    assert_eq!(
        hits.len(),
        1,
        "the decoy masked the genuine duplicate: {report:?}"
    );
    let (start, end) = hits[0].span;
    assert_eq!(
        &text[start..end],
        "alpha beta gamma delta epsilon zeta eta theta iota kappa",
        "the run must be the maximal 10-word later copy"
    );
    assert_eq!(
        start,
        text.rfind("alpha").unwrap(),
        "span is the THIRD copy"
    );
}

/// The maximal-run and backward-extension regression: 33 sub-floor decoys
/// (over the WALK_CAP of 32) sit between two full 12-word copies, so the
/// second copy's run-initial window exhausts the capped walk on decoys.
/// The next window pairs with the earlier full copy one word in, and
/// backward extension must recover the true run start. The finding spans
/// all 12 words. It never clips to an 11-word tail.
#[test]
fn q005_run_extends_backward_to_its_true_start_past_walk_capped_decoys() {
    let full = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu";
    let mut text = format!("{full}. ");
    for i in 0..33 {
        text.push_str(&format!(
            "alpha beta gamma delta epsilon zeta eta theta decoy{i}. "
        ));
    }
    text.push_str(&format!("{full}."));
    let report = analyze(&text);
    let hits = q005(&report);
    assert_eq!(hits.len(), 1, "{report:?}");
    let (start, end) = hits[0].span;
    assert_eq!(
        &text[start..end],
        full,
        "backward extension must recover the full maximal run"
    );
    assert_eq!(start, text.rfind("alpha").unwrap(), "span is the last copy");
}

/// The maximal-run ranking regression: candidates must compete on their
/// total run, including both forward and backward extension.
/// Four 33-deep decoy walls exhaust the capped walk at the later full
/// copy's first four windows, so the deciding anchor sits FOUR words into
/// the 13-word run. A rival 10-word passage glues the run's tail to the
/// word after the later copy. It ranks 10 forward against the earlier
/// copy's 9. Backward extension gives the earlier copy a 13-word total,
/// which must win the ranking.
#[test]
fn q005_lower_forward_candidate_with_longer_total_run_wins() {
    let full = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu";
    let mut text = format!("{full}. ");
    for i in 0..33 {
        text.push_str(&format!(
            "alpha beta gamma delta epsilon zeta eta theta aone{i}. "
        ));
    }
    for i in 0..33 {
        text.push_str(&format!(
            "beta gamma delta epsilon zeta eta theta iota btwo{i}. "
        ));
    }
    for i in 0..33 {
        text.push_str(&format!(
            "gamma delta epsilon zeta eta theta iota kappa cthree{i}. "
        ));
    }
    for i in 0..33 {
        text.push_str(&format!(
            "delta epsilon zeta eta theta iota kappa lambda dfour{i}. "
        ));
    }
    text.push_str("epsilon zeta eta theta iota kappa lambda mu nu tail. ");
    text.push_str(&format!("{full} tail."));
    let report = analyze(&text);
    let hits = q005(&report);
    assert_eq!(hits.len(), 1, "{report:?}");
    let (start, end) = hits[0].span;
    assert_eq!(
        &text[start..end],
        full,
        "the finding must be the maximal 13-word total run, not the rival's 10-word forward run"
    );
    assert_eq!(
        start,
        text.rfind("alpha").unwrap(),
        "span is the later full copy"
    );
}

/// ai-slop's SLOP-U001 suppresses quotations. slop-detector has no quotation
/// suppression. Quoted content participates in the scan. A blockquoted first
/// copy still anchors, and each later prose copy reports. The container
/// annotation lets the reader weigh the quoted text separately. The
/// report-layer suite checks that label.
#[test]
fn q005_quoted_copies_still_report_with_annotation_not_suppression() {
    let text = format!("> {PARA}\n\n{PARA}\n\n{PARA}\n");
    let report = analyze(&text);
    assert_eq!(
        q005(&report).len(),
        2,
        "both prose repeats of the blockquoted paragraph report: {report:?}"
    );
}

/// Clean fixtures stay silent: ordinary correspondence and the crate's own
/// README carry no 10-word verbatim self-repeats.
#[test]
fn q005_clean_fixtures_stay_silent() {
    let mail = "Hi Omar,\n\nThe vendor sent the revised quote this morning: 40 seats at \
                the old rate, renewal in March. Legal wants one change to the liability \
                clause before we sign. If you can review clause 7 by Thursday, we can \
                close this out before the offsite.\n\nBest regards,\nMina";
    assert_eq!(q005(&analyze(mail)).len(), 0);
    let readme = include_str!("../README.md");
    assert_eq!(q005(&analyze(readme)).len(), 0, "README self-duplicates");
}

/// Determinism: the full report serializes byte-identically across runs on
/// a duplication-heavy input.
#[test]
fn q005_report_is_deterministic() {
    let text = format!("{PARA} Middle words differ here in this stretch.\n\n{PARA}\n");
    let a = serde_json::to_string(&analyze(&text)).unwrap();
    let b = serde_json::to_string(&analyze(&text)).unwrap();
    assert_eq!(a, b);
}

/// A near-2 MiB document with distinct words makes every 8-word shingle
/// unique. Shared word storage and per-token chains must keep the whole
/// test binary below a 100 MiB peak heap budget. The measurement includes
/// every engine pass and this fixture's roughly 4 MiB allocation. The
/// budget keeps the 4 MiB CLI cap safe in a small worker. This shape
/// makes a String-per-word tokenizer and Vec-per-shingle index balloon.
#[test]
fn q005_worst_case_shape_stays_inside_the_memory_budget() {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(2 * 1024 * 1024);
    let mut word = 0usize;
    while text.len() < 1_900_000 {
        for _ in 0..15 {
            write!(text, "{word:x} ").unwrap();
            word += 1;
        }
        text.pop();
        text.push_str(".\n\n");
    }
    let report = analyze(&text);
    assert_eq!(
        q005(&report).len(),
        0,
        "all-distinct words cannot duplicate"
    );
    let peak = PEAK.load(Ordering::Relaxed);
    assert!(
        peak < 100 * 1024 * 1024,
        "peak heap {peak} bytes breaches the 100 MiB worst-case budget"
    );
}
