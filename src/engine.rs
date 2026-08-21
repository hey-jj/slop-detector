//! Compiled matching engines, following ai-slop's engine design minus
//! everything gate-shaped: two global Aho-Corasick automatons split by case
//! mode, one multi-pattern overlapping hybrid-DFA regex pass with reverse
//! start recovery, and one codepoint walk serving the Private-Use-Area,
//! invisible-unicode, and positional typographic-space rules. All engines
//! compile once per process behind `OnceLock`.

use crate::data::{self, Boundary, Case, Mechanism, Position, Rule};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind as AcMatchKind};
use regex_automata::hybrid::dfa::{Cache, OverlappingState, DFA};
use regex_automata::nfa::thompson;
use regex_automata::util::syntax;
use regex_automata::{Anchored, Input, MatchKind, PatternID};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;

/// One raw match: a rule index into the compiled table and a byte span into
/// the scanned source. Rendering to `Finding` happens in `lib.rs` at emit.
#[derive(Debug, Clone)]
pub struct Hit {
    pub rule: usize,
    pub span: Range<usize>,
}

struct RxMeta {
    rule: usize,
    /// The pattern begins/ends with `\b`. The DFA matched the ASCII
    /// `(?-u:\b)` prefilter form; the edge is re-validated against the real
    /// Unicode word-boundary rule before the hit is accepted.
    bound_start: bool,
    bound_end: bool,
    /// Maximum match width in bytes. Every pattern is bounded-width (the
    /// build rejects unbounded quantifiers), so this bounds the reverse
    /// start-recovery window, which keeps the overlapping adapter linear.
    max_width: Option<usize>,
}

pub struct Compiled {
    pub rules: Vec<Rule>,
    ac_ci: AhoCorasick,
    ac_ci_meta: Vec<usize>,
    ac_cs: AhoCorasick,
    ac_cs_meta: Vec<usize>,
    rx_fwd: DFA,
    rx_rev: DFA,
    rx_meta: Vec<RxMeta>,
    /// Codepoint-class rules: (rule index, inclusive ranges). Adjacent
    /// same-rule codepoints merge into one span.
    cp_rules: Vec<(usize, Vec<(u32, u32)>)>,
    /// Positional-space rules: (rule index, codepoints, min_count).
    space_rules: Vec<(usize, Vec<u32>, usize)>,
    /// Participial-opener rules: (rule index, lowercased stop-list,
    /// max clause bytes).
    participial_rules: Vec<(usize, HashSet<String>, usize)>,
    /// Contrastive-tail rules (SD-Q004's T1 form).
    pub(crate) contrastive_rules: Vec<ContrastiveRule>,
    /// Self-duplication rules (SD-Q005): rule index plus the shingle
    /// order, run floor, and emission cap in words.
    pub(crate) duplication_rules: Vec<DuplicationRule>,
    /// Capability-denial rules (SD-Q007).
    pub(crate) denial_rules: Vec<DenialRule>,
    /// Rationale-leak rules (SD-Q008).
    pub(crate) rationale_rules: Vec<RationaleRule>,
}

/// One compiled capability-denial rule. Every phrase is tokenized at build
/// time by the same tokenizer the scan runs over the source, so a phrase and
/// the text it matches split identically (`trade-off` is two tokens on both
/// sides). Each subject set is its standalone subjects plus every determiner
/// phrase completed by a tool noun.
pub(crate) struct DenialRule {
    rule: usize,
    /// Positive subjects, which need a following negation.
    subjects: Vec<Vec<String>>,
    /// Negative subjects, which carry the negation themselves.
    negative_subjects: Vec<Vec<String>>,
    /// The pronoun half of the subject set. A pronoun standing next to a
    /// noun-phrase subject refers back to it, so the adjacency arm reads the
    /// two as one referent.
    pronouns: HashSet<String>,
    coordinators: HashSet<String>,
    /// Negations that can open a command.
    imperative_negations: Vec<Vec<String>>,
    /// Negations that only ever carry a finite verb.
    finite_negations: Vec<Vec<String>>,
    window: usize,
    verb_window: usize,
    /// Every capability-verb form, for the two spellings that carry a subject.
    capability_verbs: HashSet<String>,
    /// The base and third-person forms, which the subjectless spelling takes
    /// behind a finite-only negation.
    capability_finite: HashSet<String>,
    /// The third-person forms alone, which are all the subjectless spelling
    /// takes behind an imperative-capable negation.
    capability_third: HashSet<String>,
    /// The base forms alone, which the coordinated case takes behind `and`.
    capability_base: HashSet<String>,
    hedges: Vec<Vec<String>>,
    /// The shared tool-noun set, kept for the adjacency arm's referent test.
    tool_nouns: HashSet<String>,
}

/// One compiled rationale-leak rule: the marker phrases plus the tool-noun
/// anchor.
pub(crate) struct RationaleRule {
    rule: usize,
    markers: Vec<Vec<String>>,
    tool_nouns: HashSet<String>,
}

/// Split a phrase into the same tokens the scan produces from source text.
fn phrase_tokens(phrase: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in phrase.chars() {
        let c = if c == '\u{2019}' { '\'' } else { c };
        if c.is_alphanumeric() || c == '\'' || c == '*' {
            cur.push(c.to_ascii_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A closed subject set: the standalone subjects, then every determiner
/// phrase completed by a tool noun. A determiner may itself be several words
/// (`none of the`).
fn subject_set(
    subjects: &[String],
    determiners: &[String],
    tool_nouns: &[String],
) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = subjects.iter().map(|s| phrase_tokens(s)).collect();
    for d in determiners {
        for n in tool_nouns {
            let mut phrase = phrase_tokens(d);
            phrase.push(n.clone());
            out.push(phrase);
        }
    }
    // Longest first, so `the tool` wins over a bare pronoun at the same
    // position and the recorded subject is the whole phrase.
    out.sort_by_key(|p| std::cmp::Reverse(p.len()));
    out
}

/// One compiled self-duplication rule.
pub(crate) struct DuplicationRule {
    pub(crate) rule: usize,
    pub(crate) shingle_words: usize,
    pub(crate) min_run_words: usize,
    pub(crate) max_reports: usize,
}

/// One compiled contrastive-tail rule: the imperative-opener deny-list and
/// second-person cues are lowercased at load; `max_np` caps the noun phrase
/// in bytes and `window` caps the clause walk-back in bytes.
pub(crate) struct ContrastiveRule {
    rule: usize,
    openers: HashSet<String>,
    second_person: Vec<String>,
    max_np: usize,
    window: usize,
}

static COMPILED: OnceLock<Result<Compiled, String>> = OnceLock::new();

/// The compiled engine. The rule table and lexicons are embedded at build
/// time, so a load or compile failure is a defect in the shipped data, not a
/// property of any input; `data_compiles` in the test suite guards it.
pub fn compiled() -> &'static Compiled {
    COMPILED
        .get_or_init(build)
        .as_ref()
        .expect("embedded pattern data failed to compile (build defect)")
}

/// Rewrite a pattern into DFA-compatible form. Pattern-edge `\b` becomes an
/// ASCII prefilter re-validated in `scan_rx`; look-arounds are unsupported.
fn rewrite_pattern(p: &str) -> Result<(String, bool, bool), String> {
    if p.contains("(?<") || p.contains("(?!") || p.contains("(?=") {
        return Err(format!("unsupported look-around in pattern {p}"));
    }
    let bound_start = p.strip_prefix("(?i)").unwrap_or(p).starts_with(r"\b");
    let bound_end = p.ends_with(r"\b");
    Ok((p.replace(r"\b", r"(?-u:\b)"), bound_start, bound_end))
}

/// Validate a rewritten pattern against the locked bounded-width policy and
/// return its maximum match width in bytes. The overlapping adapter recovers
/// each match start with a reverse search; an unbounded-width pattern makes
/// that window the whole region and the scan quadratic, so every
/// unbounded-width quantifier (`*`, `+`, `{n,}`) is rejected at build,
/// whitespace included.
fn validate_bounded_width(pat: &str) -> Result<Option<usize>, String> {
    let hir = regex_syntax::parse(pat)
        .map_err(|e| format!("pattern {pat} failed width-validation parse: {e}"))?;
    if unbounded_repetition(&hir) {
        return Err(format!(
            "pattern {pat} has an unbounded-width quantifier (*, +, or {{n,}}); \
             bounded forms are required, whitespace included"
        ));
    }
    Ok(hir.properties().maximum_len())
}

fn unbounded_repetition(hir: &regex_syntax::hir::Hir) -> bool {
    use regex_syntax::hir::HirKind;
    match hir.kind() {
        HirKind::Repetition(rep) => rep.max.is_none() || unbounded_repetition(&rep.sub),
        HirKind::Capture(c) => unbounded_repetition(&c.sub),
        HirKind::Concat(v) | HirKind::Alternation(v) => v.iter().any(unbounded_repetition),
        _ => false,
    }
}

fn build() -> Result<Compiled, String> {
    let rules = data::load()?;

    let mut ci_pats: Vec<&str> = Vec::new();
    let mut ci_meta: Vec<usize> = Vec::new();
    let mut cs_pats: Vec<&str> = Vec::new();
    let mut cs_meta: Vec<usize> = Vec::new();
    let mut rx_pats: Vec<String> = Vec::new();
    let mut rx_meta: Vec<RxMeta> = Vec::new();
    let mut cp_rules: Vec<(usize, Vec<(u32, u32)>)> = Vec::new();
    let mut space_rules: Vec<(usize, Vec<u32>, usize)> = Vec::new();
    let mut participial_rules: Vec<(usize, HashSet<String>, usize)> = Vec::new();
    let mut contrastive_rules: Vec<ContrastiveRule> = Vec::new();
    let mut duplication_rules: Vec<DuplicationRule> = Vec::new();
    let mut denial_rules: Vec<DenialRule> = Vec::new();
    let mut rationale_rules: Vec<RationaleRule> = Vec::new();

    for (idx, rule) in rules.iter().enumerate() {
        // Every text rule's `patterns` ride the shared regex pass; the data
        // loader guarantees non-text mechanisms carry none.
        for p in &rule.patterns {
            let (pat, bs, be) = rewrite_pattern(p)?;
            let max_width = validate_bounded_width(&pat)?;
            rx_pats.push(pat);
            rx_meta.push(RxMeta {
                rule: idx,
                bound_start: bs,
                bound_end: be,
                max_width,
            });
        }
        match rule.mechanism {
            Mechanism::WordSet | Mechanism::Regex => {
                for term in &rule.terms {
                    match rule.case {
                        Case::Insensitive => {
                            ci_pats.push(term);
                            ci_meta.push(idx);
                        }
                        Case::Sensitive => {
                            cs_pats.push(term);
                            cs_meta.push(idx);
                        }
                    }
                }
            }
            Mechanism::Codepoint => cp_rules.push((idx, rule.ranges.clone())),
            Mechanism::PositionalSpace => {
                space_rules.push((idx, rule.codepoints.clone(), rule.min_count));
            }
            Mechanism::ParticipialOpener => {
                participial_rules.push((
                    idx,
                    rule.stoplist.iter().cloned().collect(),
                    rule.max_clause,
                ));
            }
            Mechanism::ContrastiveTail => {
                contrastive_rules.push(ContrastiveRule {
                    rule: idx,
                    openers: rule.stoplist.iter().cloned().collect(),
                    second_person: rule.second_person.clone(),
                    max_np: rule.max_np,
                    window: rule.clause_window,
                });
            }
            Mechanism::SelfDuplication => {
                duplication_rules.push(DuplicationRule {
                    rule: idx,
                    shingle_words: rule.shingle_words,
                    min_run_words: rule.min_run_words,
                    max_reports: rule.max_reports,
                });
            }
            Mechanism::CapabilityDenial => {
                denial_rules.push(DenialRule {
                    rule: idx,
                    subjects: subject_set(&rule.subjects, &rule.determiners, &rule.tool_nouns),
                    negative_subjects: subject_set(
                        &rule.negative_subjects,
                        &rule.negative_determiners,
                        &rule.tool_nouns,
                    ),
                    pronouns: rule.subjects.iter().cloned().collect(),
                    coordinators: rule.coordinators.iter().cloned().collect(),
                    imperative_negations: rule
                        .imperative_negations
                        .iter()
                        .map(|p| phrase_tokens(p))
                        .collect(),
                    finite_negations: rule
                        .finite_negations
                        .iter()
                        .map(|p| phrase_tokens(p))
                        .collect(),
                    window: rule.negation_window,
                    verb_window: rule.verb_window,
                    capability_verbs: rule
                        .capability_verbs_base
                        .iter()
                        .chain(&rule.capability_verbs_s)
                        .chain(&rule.capability_verbs_ing)
                        .cloned()
                        .collect(),
                    capability_finite: rule
                        .capability_verbs_base
                        .iter()
                        .chain(&rule.capability_verbs_s)
                        .cloned()
                        .collect(),
                    capability_third: rule.capability_verbs_s.iter().cloned().collect(),
                    capability_base: rule.capability_verbs_base.iter().cloned().collect(),
                    hedges: rule.hedges.iter().map(|p| phrase_tokens(p)).collect(),
                    tool_nouns: rule.tool_nouns.iter().cloned().collect(),
                });
            }
            Mechanism::RationaleLeak => {
                rationale_rules.push(RationaleRule {
                    rule: idx,
                    markers: rule.markers.iter().map(|p| phrase_tokens(p)).collect(),
                    tool_nouns: rule.tool_nouns.iter().cloned().collect(),
                });
            }
        }
    }

    let ac_ci = AhoCorasickBuilder::new()
        .match_kind(AcMatchKind::Standard)
        .ascii_case_insensitive(true)
        .build(&ci_pats)
        .map_err(|e| format!("case-insensitive automaton: {e}"))?;
    let ac_cs = AhoCorasickBuilder::new()
        .match_kind(AcMatchKind::Standard)
        .build(&cs_pats)
        .map_err(|e| format!("case-sensitive automaton: {e}"))?;

    let syn = syntax::Config::new()
        .unicode(true)
        .utf8(true)
        .multi_line(true);
    // One multi-pattern machine with MatchKind::All. The lazy DFA keeps the
    // overlapping semantics while materializing only reachable states.
    let fwd = DFA::builder()
        .configure(
            DFA::config()
                .match_kind(MatchKind::All)
                .starts_for_each_pattern(true)
                .cache_capacity(4 * 1024 * 1024),
        )
        .syntax(syn)
        .build_many(&rx_pats)
        .map_err(|e| format!("forward dfa: {e}"))?;
    let rev = DFA::builder()
        .configure(
            DFA::config()
                .match_kind(MatchKind::All)
                .starts_for_each_pattern(true)
                .cache_capacity(4 * 1024 * 1024),
        )
        .thompson(thompson::Config::new().reverse(true))
        .syntax(syn)
        .build_many(&rx_pats)
        .map_err(|e| format!("reverse dfa: {e}"))?;

    // Empty-matchable patterns are banned: an empty match cites nothing.
    let mut probe_cache = fwd.create_cache();
    for (pid, pat) in rx_pats.iter().enumerate() {
        let input = Input::new("").anchored(Anchored::Pattern(PatternID::new_unchecked(pid)));
        if let Ok(Some(_)) = fwd.try_search_fwd(&mut probe_cache, &input) {
            return Err(format!("pattern {pat} can match empty"));
        }
    }

    Ok(Compiled {
        rules,
        ac_ci,
        ac_ci_meta: ci_meta,
        ac_cs,
        ac_cs_meta: cs_meta,
        rx_fwd: fwd,
        rx_rev: rev,
        rx_meta,
        cp_rules,
        space_rules,
        participial_rules,
        contrastive_rules,
        duplication_rules,
        denial_rules,
        rationale_rules,
    })
}

fn word_bounded(hay: &str, span: &Range<usize>) -> bool {
    let before_ok = hay[..span.start]
        .chars()
        .next_back()
        .map(|c| !unicode_ident::is_xid_continue(c))
        .unwrap_or(true);
    let after_ok = hay[span.end..]
        .chars()
        .next()
        .map(|c| !unicode_ident::is_xid_continue(c))
        .unwrap_or(true);
    before_ok && after_ok
}

/// Real Unicode word boundary at `at`: exactly one side of the position is a
/// word (xid_continue) character. Out-of-text sides count as non-word.
fn unicode_word_boundary(hay: &str, at: usize) -> bool {
    let before = hay[..at]
        .chars()
        .next_back()
        .map(unicode_ident::is_xid_continue)
        .unwrap_or(false);
    let after = hay[at..]
        .chars()
        .next()
        .map(unicode_ident::is_xid_continue)
        .unwrap_or(false);
    before != after
}

/// Scripts whose orthography requires ZWJ/ZWNJ for shaping: Arabic and its
/// presentation forms, Syriac, the nine contiguous Indic blocks, Sinhala,
/// Myanmar, and Khmer.
fn joining_script(c: char) -> bool {
    matches!(c as u32,
        0x0600..=0x06FF
            | 0x0700..=0x074F
            | 0x0750..=0x077F
            | 0x08A0..=0x08FF
            | 0x0900..=0x0DFF
            | 0x1000..=0x109F
            | 0x1780..=0x17FF
            | 0xFB50..=0xFDFF
            | 0xFE70..=0xFEFC)
}

/// The pictographic blocks that participate in emoji ZWJ sequences:
/// miscellaneous symbols, dingbats, supplemental arrows-B symbols, and the
/// plane-1 emoji planes (which include the skin-tone modifiers and regional
/// indicators).
fn pictographic(c: char) -> bool {
    matches!(c as u32,
        0x2600..=0x27BF | 0x2B00..=0x2B5F | 0x1F000..=0x1FAFF)
}

/// The list markers a writer may put in front of the first word. The four
/// bullet glyphs also appear in `LEADING_DECORATION`, because ai-slop and
/// unslop hold one combined set while this tree splits the roles: its marker
/// arm carries the ASCII list openers too. Both roles skip, so the overlap
/// changes nothing.
///
/// Fleet-wide set, fixed by F-R14a and F-R14e. Edit this list and
/// `LEADING_DECORATION` together, and in all three repos.
///
/// Measured over 3.08M lines of the fleet corpus: U+2022 appears 205 times,
/// 136 of them line-leading. U+2023, U+2043, and U+2219 have a combined
/// population of one, which every corpus available to the fleet reads as
/// indistinguishable from zero. They are carried on cost asymmetry and on
/// the completeness of the set, never on measured need. The asymmetry is
/// one-directional: widening a skip set turns silences into findings and
/// never the reverse, though a finding it creates can still be wrong and
/// still goes to the reader. U+00B7 is deliberately absent, on 2,341
/// occurrences with only 184 line-leading: the middle dot is an inline
/// separator and a letter in Catalan, so a word behind one opens nothing.
const LEADING_MARKERS: [char; 9] = [
    '-', '*', '+', '>', '#', '\u{2022}', '\u{2023}', '\u{2043}', '\u{2219}',
];

/// What a writer may put in front of the first word as decoration, as
/// inclusive codepoint ranges. A single codepoint is written as a range onto
/// itself so the table reads one way throughout.
///
/// Fleet-wide set, fixed by F-R14a and F-R14e, and the same codepoints
/// `is_leading_decoration` carries in ai-slop and unslop. The order below
/// follows theirs so the three lists diff cleanly. Edit this list and
/// `LEADING_MARKERS` together, and in all three repos.
///
/// The Geometric Shapes block is what the measurement turned on: the nested
/// list glyphs a paste brings with it live there, and they led 3,474 lines
/// of the corpus. Letters, digits, and the punctuation that carries a
/// sentence forward stay out, since a comma in front of a word puts the word
/// mid-sentence.
///
/// The whole union is carried because the walk stops at the first character
/// it does not cover. One uncovered glyph defeats every covered glyph beside
/// it, and a pasted bullet run mixes them as a matter of course, so
/// `▪ 🎉 Moreover` needs both blocks present to read as an opening.
///
/// Kept separate from `pictographic`, which answers a different question for
/// SD-R003: whether a codepoint joins an emoji ZWJ sequence. Widening that
/// one would move an unrelated rule.
const LEADING_DECORATION: [(u32, u32); 22] = [
    (0x2022, 0x2022), // bullet
    (0x2023, 0x2023), // triangular bullet
    (0x2043, 0x2043), // hyphen bullet
    (0x2219, 0x2219), // bullet operator
    (0x200D, 0x200D), // zero width joiner
    (0x20E3, 0x20E3), // combining enclosing keycap
    (0xFE0E, 0xFE0F), // variation selectors 15 and 16
    (0x203C, 0x203C), // double exclamation
    (0x2049, 0x2049), // exclamation question
    (0x2122, 0x2122), // trade mark
    (0x2139, 0x2139), // information
    (0x2190, 0x21FF), // arrows
    (0x2300, 0x23FF), // miscellaneous technical
    (0x24C2, 0x24C2), // circled M
    (0x25A0, 0x25FF), // geometric shapes
    (0x2600, 0x27BF), // miscellaneous symbols and dingbats
    (0x2B00, 0x2BFF), // miscellaneous symbols and arrows
    (0x3030, 0x3030),
    (0x303D, 0x303D),
    (0x3297, 0x3297),
    (0x3299, 0x3299),
    // The emoji planes, which include the skin-tone modifiers and the
    // regional indicators.
    (0x1F000, 0x1FAFF),
];

fn is_leading_decoration(c: char) -> bool {
    let u = c as u32;
    LEADING_DECORATION
        .iter()
        .any(|&(lo, hi)| (lo..=hi).contains(&u))
}

/// The ordered-list marker ending at `punct`, which holds a `.` or a `)`.
/// Returns the offset of the first digit when the run is a list marker,
/// meaning a digit run that opens its line behind nothing but whitespace.
/// A digit run following other text on the line is not a marker: in
/// `See item 3. Moreover` the `3` sits behind `item`, so the `.` is doing
/// its ordinary work of ending a sentence.
fn ordered_marker(src: &str, punct: usize) -> Option<usize> {
    let digits_end = punct;
    let mut digits_start = punct;
    for (off, c) in src[..digits_end].char_indices().rev() {
        if c.is_ascii_digit() {
            digits_start = off;
        } else {
            break;
        }
    }
    if digits_start == digits_end {
        return None; // no digit run, so no marker
    }
    // Only whitespace may stand between the digit run and the line start.
    for c in src[..digits_start].chars().rev() {
        match c {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => return Some(digits_start),
            c if c.is_whitespace() => continue,
            _ => return None,
        }
    }
    Some(digits_start)
}

/// True when `at` sits at a block or sentence start: the start of the text,
/// after a line break, or after sentence-ending punctuation. Anything a
/// writer puts in front of the first word without starting a new thought is
/// skipped on the way back. That covers whitespace, the unordered list
/// markers, an ordered list marker (`1.`, `2)`), a blockquote `>`, a heading
/// `#`, and a leading emoji run. Skipping is self-gating, because the walk
/// has to reach a line start or a terminal to return true: in `C# Moreover`
/// the `#` is skipped and the `C` behind it ends the walk on false.
fn at_block_start(src: &str, at: usize) -> bool {
    let mut i = at;
    while let Some(c) = src[..i].chars().next_back() {
        let cs = i - c.len_utf8();
        match c {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => return true,
            '!' | '?' => return true,
            '.' | ')' => {
                if let Some(marker) = ordered_marker(src, cs) {
                    i = marker;
                    continue;
                }
                // `i` is the offset just past the punctuation, which is the
                // terminal test's own argument. A `)` never ends a sentence.
                return c == '.' && period_is_terminal(src, i);
            }
            c if LEADING_MARKERS.contains(&c) => i = cs,
            c if c.is_whitespace() || is_leading_decoration(c) => i = cs,
            _ => return false,
        }
    }
    true
}

/// True when the hit is fully contained in one of the rule's exemption
/// phrases, checked case-insensitively in a window around the span
/// (ai-slop's `exempted`). Lowercasing can change byte lengths for
/// non-ASCII, so the match position is recomputed by lowercasing the
/// prefix.
fn exempted(hay: &str, span: &Range<usize>, phrases: &[String]) -> bool {
    if phrases.is_empty() {
        return false;
    }
    let win_start =
        crate::widen_to_char_boundaries(hay, span.start.saturating_sub(60)..span.start).start;
    let win_end =
        crate::widen_to_char_boundaries(hay, span.end..(span.end + 60).min(hay.len())).end;
    let window = hay[win_start..win_end].to_lowercase();
    let rel_start = hay[win_start..span.start].to_lowercase().len();
    let rel_end = rel_start + hay[span.start..span.end].to_lowercase().len();
    for phrase in phrases {
        let mut at = 0usize;
        while let Some(pos) = window[at..].find(phrase.as_str()) {
            let s = at + pos;
            let e = s + phrase.len();
            if s <= rel_start && e >= rel_end {
                return true;
            }
            at = s + 1;
        }
    }
    false
}

/// Pass 1: both Aho-Corasick automatons over the source bytes. Overlapping
/// standard matching; leftmost kinds silently drop nested entries and are
/// prohibited. The source is never lowercased: case-insensitivity lives in
/// the automaton, so offsets stay in source coordinates.
fn scan_ac(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    let passes: [(&AhoCorasick, &[usize]); 2] =
        [(&cp.ac_ci, &cp.ac_ci_meta), (&cp.ac_cs, &cp.ac_cs_meta)];
    for (ac, meta) in passes {
        for m in ac.find_overlapping_iter(src) {
            let rule_idx = meta[m.pattern().as_usize()];
            let rule = &cp.rules[rule_idx];
            let span = m.start()..m.end();
            if rule.boundary == Boundary::Word && !word_bounded(src, &span) {
                continue;
            }
            if rule.position == Position::BlockStart && !at_block_start(src, span.start) {
                continue;
            }
            if exempted(src, &span, &rule.exemptions) {
                continue;
            }
            hits.push(Hit {
                rule: rule_idx,
                span,
            });
        }
    }
}

/// Pass 5: the participial-opener scan (SD-Q002). A capitalized ASCII
/// `-ing` word at a block or sentence start, not on the stop-list, opening
/// a bounded clause that ends at a comma before any sentence break. The
/// span runs from the word through the comma.
fn scan_participial(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    if cp.participial_rules.is_empty() {
        return;
    }
    for (i, c) in src.char_indices() {
        if !c.is_ascii_uppercase() || !at_block_start(src, i) {
            continue;
        }
        // The candidate word: one uppercase letter then lowercase ASCII.
        let word_len = src[i + 1..]
            .bytes()
            .take_while(|b| b.is_ascii_lowercase())
            .count();
        let word_end = i + 1 + word_len;
        let word = &src[i..word_end];
        if word.len() < 5 || word.len() > 30 || !word.ends_with("ing") {
            continue;
        }
        // The word must open a clause: a space, then bounded non-break
        // text, then a comma.
        if !src[word_end..].starts_with(' ') {
            continue;
        }
        for (rule_idx, stoplist, max_clause) in &cp.participial_rules {
            if stoplist.contains(&word.to_ascii_lowercase()) {
                continue;
            }
            let clause = &src[word_end + 1..];
            let limit = (*max_clause).min(clause.len());
            let mut comma = None;
            for (j, cc) in clause.char_indices() {
                if j >= limit {
                    break;
                }
                match cc {
                    ',' => {
                        comma = Some(j);
                        break;
                    }
                    '.' | '!' | '?' | ';' | ':' | '\n' => break,
                    _ => {}
                }
            }
            // A comma directly after the word carries no clause.
            let Some(j) = comma else { continue };
            if j == 0 {
                continue;
            }
            hits.push(Hit {
                rule: *rule_idx,
                span: i..word_end + 1 + j + 1,
            });
        }
    }
}

/// First word token of a clause: leading non-word characters (quotes,
/// brackets) are skipped, then the maximal run of alphanumerics plus
/// apostrophes is collected, ASCII-lowercased, with the typographic
/// apostrophe folded so a `don\u{2019}t` in the source still matches the
/// base-form deny-list entry `don't`.
fn first_token(clause: &str) -> String {
    let mut out = String::new();
    for c in clause.chars() {
        let c = if c == '\u{2019}' { '\'' } else { c };
        if c.is_alphanumeric() || c == '\'' {
            out.push(c.to_ascii_lowercase());
        } else if out.is_empty() {
            continue;
        } else {
            break;
        }
    }
    out
}

/// Word token beginning exactly at `at` (used for the interior-directive
/// check, where the position after `, ` or `then ` is already known).
fn token_at(clause_lower: &str, at: usize) -> String {
    first_token(&clause_lower[at..])
}

/// Word-bounded, case-insensitive containment of `needle` (already
/// lowercase) in `hay_lower` (already lowercase).
fn contains_word(hay_lower: &str, needle: &str) -> bool {
    let mut at = 0usize;
    while let Some(pos) = hay_lower[at..].find(needle) {
        let s = at + pos;
        let e = s + needle.len();
        let before_ok = hay_lower[..s]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        let after_ok = hay_lower[e..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        at = s + 1;
    }
    false
}

/// Bounded terminal test for a `.` met during the NP scan or the clause
/// walk-back, ported from ai-slop's SLOP-C007 fix. `dot_end` is the offset
/// just past the `.` in `text`. A period followed directly by an
/// alphanumeric character is abbreviation- or number-internal (`U.S`,
/// `3.5`): not a terminal. A period followed by a bounded ASCII space/tab
/// run and then a lowercase continuation is mid-sentence punctuation
/// (`U.S. but`, `e.g. the`): not a terminal. Everything else — end of
/// text, a line break, an uppercase/digit/quote/bracket follower, a
/// whitespace run past the parser's 8-unit bound — is a terminal, exactly
/// as before this test existed. The peek is O(1) and bounded. Accepted
/// false negatives, mirrored from ai-slop's KNOWN-EDGES: chat-style prose
/// that starts sentences lowercase reads a real terminal as a
/// continuation and stays silent, and an abbreviation followed by a
/// capitalized word (`Mr. Smith`) still reads as a terminal — both
/// resolve toward silence or the pre-existing behavior, never toward a
/// new firing surface.
fn period_is_terminal(text: &str, dot_end: usize) -> bool {
    let mut chars = text[dot_end..].chars();
    let Some(first) = chars.next() else {
        return true; // end of text
    };
    if first.is_alphanumeric() {
        return false; // abbreviation- or number-internal
    }
    if first != ' ' && first != '\t' {
        // Line breaks end the block; quotes, brackets, and punctuation all
        // sit on the terminal side.
        return true;
    }
    // Walk at most 8 ASCII space/tab units, mirroring the tail parser's own
    // whitespace bound.
    let mut seen = 1usize;
    loop {
        match chars.next() {
            Some(' ') | Some('\t') => {
                seen += 1;
                if seen > 8 {
                    return true;
                }
            }
            Some('\n') | Some('\r') => return true, // block end
            Some(c) => return !c.is_lowercase(),
            None => return true,
        }
    }
}

/// Parse the contrastive-tail shape starting at the comma at `comma`: up to
/// 8 whitespace characters, `not` or `never` (case-insensitive, followed by
/// 1..=8 whitespace), then an NP of 1..=`np_max` bytes containing none of
/// `!?;:,\n` (nor a U+FFFD replacement character, which in raw inbound
/// text is decode residue, never a noun phrase) and at least one
/// non-whitespace character (a whitespace-only "NP" is not a noun phrase),
/// closed by a terminal `.`, `!`, or `?`. A non-terminal `.`
/// (abbreviation-internal or mid-sentence per `period_is_terminal`) is
/// legal NP content. Returns the exclusive end offset of the terminal
/// punctuation. The no-interior-comma constraint is what keeps the
/// parenthetical `X, not Y, verb ...` interpolation out of scope, and a
/// word-bounded `but` anywhere in the NP rejects the tail outright: a
/// contrastive continuation (`, not in the U.S. but in Asia.`) is the
/// not-X-but-Y pair form — SLOP-C008's territory and a legitimate
/// contrast — never a bare apophatic caveat.
/// Both whitespace loops match ASCII whitespace only (space/tab/LF/CR), by
/// design, mirroring ai-slop's SLOP-C007: a non-ASCII space inside a
/// contrastive tail is an accepted false negative.
///
/// Words ending in `-ing` that the participial exemption never covers: the
/// four quantifier pronouns, which are ordinary NP heads, and the preposition
/// `during`, which opens one. Denying `during` the exemption is what keeps
/// the participle test honest, and whether `, not during matching.` should
/// fire at all is a question about the rule's scope, not about this list.
const NOT_A_PARTICIPLE: [&str; 5] = ["nothing", "anything", "something", "everything", "during"];

fn parse_tail(text: &str, comma: usize, np_max: usize) -> Option<usize> {
    let rest = text.get(comma + 1..)?;
    let mut i = 0usize;
    for c in rest.chars().take(8) {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let after_ws = &rest[i..];
    // `get` rather than direct slicing: the byte at the cut can sit inside a
    // multi-byte character, and a directly sliced prefix would panic there.
    let kw_len = if after_ws
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("never"))
    {
        5
    } else if after_ws
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("not"))
    {
        3
    } else {
        return None;
    };
    // The keyword must be followed by 1..=8 ASCII whitespace characters
    // (its right word boundary).
    let mut j = i + kw_len;
    let mut ws = 0usize;
    for c in rest[j..].chars().take(8) {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            ws += 1;
            j += c.len_utf8();
        } else {
            break;
        }
    }
    if ws == 0 {
        return None;
    }
    // A participial adjunct is not a contrastive tail: `never judging anyone`
    // says how she listened, not what she did instead. The exemption is
    // narrow. The negation has to be immediately followed by the `-ing` word,
    // so a determiner in between keeps the tail (`not the beginning`, `not a
    // building`), and five words wear the same letters without being
    // participles.
    let first: String = rest[j..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '\'')
        .flat_map(char::to_lowercase)
        .collect();
    if first.ends_with("ing") && !NOT_A_PARTICIPLE.contains(&first.as_str()) {
        return None;
    }
    // NP scan: bounded, no clause punctuation, must close with a terminal,
    // and must carry at least one non-whitespace character — an empty or
    // whitespace-only span between the keyword and the terminal is not a
    // noun phrase.
    let np_start = j;
    let mut k = j;
    let mut np_has_content = false;
    for c in rest[np_start..].chars() {
        match c {
            '.' if !period_is_terminal(text, comma + 1 + k + 1) => {
                // Abbreviation-internal or mid-sentence period (`U.S.`,
                // `e.g.`): NP content, not a terminal.
                np_has_content = true;
                k += 1;
                if k - np_start > np_max {
                    return None;
                }
            }
            '.' | '!' | '?' => {
                if !np_has_content {
                    return None; // empty or whitespace-only NP
                }
                // A word-bounded `but` inside the tail means the negation
                // carries its own contrastive continuation ("not in the
                // U.S. but in Asia"): a not-X-but-Y pair, which is a
                // legitimate contrast shape and SLOP-C008's territory, not
                // a bare apophatic caveat. The comma-tail rule stays
                // silent. Bounded: the NP is at most `np_max` bytes.
                let np_lower = rest[np_start..k].to_ascii_lowercase();
                if contains_word(&np_lower, "but") {
                    return None;
                }
                return Some(comma + 1 + k + c.len_utf8());
            }
            ';' | ':' | ',' | '\n' | '\u{FFFD}' => return None,
            _ => {
                if !c.is_whitespace() {
                    np_has_content = true;
                }
                k += c.len_utf8();
                if k - np_start > np_max {
                    return None;
                }
            }
        }
    }
    None
}

/// Recover the clause start: walk back from the comma at most `window`
/// bytes to the nearest clause boundary — a line break, or terminal
/// punctuation (`.`, `!`, `?`, plus `:`) followed by whitespace — as a
/// single bounded backward pass. A `.` additionally goes through
/// `period_is_terminal`, so an abbreviation (`the U.S. market`) no longer
/// truncates the recovered clause — the suppression classifier sees the
/// whole sentence, an FP-reducing change. The `:` `!` `?` arms are
/// untouched: a colon followed by lowercase is a legitimate clause
/// boundary and must stay one. Offset 0 counts as a boundary when it lies
/// inside the window. `None` means the window was exhausted without a
/// boundary; the caller fires by default (fail toward the evidence report).
fn clause_start(text: &str, comma: usize, window: usize) -> Option<usize> {
    let lo = crate::widen_to_char_boundaries(text, comma.saturating_sub(window)..comma).start;
    let region = &text[lo..comma];
    for (off, c) in region.char_indices().rev() {
        let abs = lo + off;
        let boundary_end = match c {
            '\n' => Some(abs + 1),
            '.' | '!' | '?' | ':' => {
                let next = text[abs + c.len_utf8()..].chars().next();
                if matches!(next, Some(w) if w.is_whitespace())
                    && (c != '.' || period_is_terminal(text, abs + 1))
                {
                    Some(abs + c.len_utf8())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(mut p) = boundary_end {
            // The clause proper starts after the whitespace run.
            for w in text[p..comma].chars() {
                if w.is_whitespace() {
                    p += w.len_utf8();
                } else {
                    break;
                }
            }
            return Some(p);
        }
    }
    if lo == 0 {
        return Some(0);
    }
    None
}

/// The suppression classifier over a recovered clause. True means the site
/// reads as a directive and stays silent.
fn suppressed(clause: &str, openers: &HashSet<String>, second_person: &[String]) -> bool {
    let lower = clause.to_lowercase();
    // 1. Imperative opener: the clause's first token is on the base-form
    //    deny-list.
    let head = first_token(&lower);
    if !head.is_empty() && openers.contains(&head) {
        return true;
    }
    // 2. Second-person cue anywhere before the comma, word-bounded.
    if second_person.iter().any(|t| contains_word(&lower, t)) {
        return true;
    }
    // 3. A deny-list verb immediately after an interior `, ` or after
    //    `then ` — the leading-adverbial directive
    //    ("When in doubt, use the builder, not the raw constructor.").
    let mut at = 0usize;
    while let Some(pos) = lower[at..].find(", ") {
        let s = at + pos + 2;
        let tok = token_at(&lower, s);
        if !tok.is_empty() && openers.contains(&tok) {
            return true;
        }
        at = s;
    }
    let mut at = 0usize;
    while let Some(pos) = lower[at..].find("then ") {
        let s = at + pos;
        let before_ok = lower[..s]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        if before_ok {
            let tok = token_at(&lower, s + 5);
            if !tok.is_empty() && openers.contains(&tok) {
                return true;
            }
        }
        at = s + 5;
    }
    false
}

/// Pass 6: the contrastive-tail scan (SD-Q004's T1 form), ported from
/// ai-slop's SLOP-C007 structural evaluator. A trailing `, not <NP>.` or
/// `, never <NP>.` tag closing its sentence fires unless the recovered
/// clause reads as a directive: an imperative opener on the deny-list, a
/// second-person cue before the comma, or a deny-list verb after an
/// interior `, ` or `then `. An exhausted walk-back window fires by
/// default. Every window is bounded by rule data; the scan runs over the
/// raw source — slop-detector has no prose/code segmentation, and the
/// rule's guard states that caveat.
fn scan_contrastive(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    if cp.contrastive_rules.is_empty() {
        return;
    }
    for cr in &cp.contrastive_rules {
        for (comma, _) in src.char_indices().filter(|&(_, c)| c == ',') {
            let Some(tail_end) = parse_tail(src, comma, cr.max_np) else {
                continue;
            };
            if let Some(cs) = clause_start(src, comma, cr.window) {
                if suppressed(&src[cs..comma], &cr.openers, &cr.second_person) {
                    continue;
                }
            }
            hits.push(Hit {
                rule: cr.rule,
                span: comma..tail_end,
            });
        }
    }
}

/// One lowercased word token with its byte span in the source. A token is a
/// maximal run of alphanumerics and apostrophes, with the typographic
/// apostrophe folded to ASCII so `doesn\u{2019}t` and `doesn't` are one token.
struct Tok {
    start: usize,
    end: usize,
    text: String,
}

fn tokens_in(src: &str, range: &Range<usize>) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for (i, c) in src[range.clone()].char_indices() {
        let at = range.start + i;
        let c = if c == '\u{2019}' { '\'' } else { c };
        if c.is_alphanumeric() || c == '\'' {
            match &mut open {
                Some((_, text)) => text.push(c.to_ascii_lowercase()),
                None => open = Some((at, c.to_ascii_lowercase().to_string())),
            }
        } else if let Some((start, text)) = open.take() {
            out.push(Tok {
                start,
                end: at,
                text,
            });
        }
    }
    if let Some((start, text)) = open.take() {
        out.push(Tok {
            start,
            end: range.end,
            text,
        });
    }
    out
}

/// Match `phrase` against the tokens starting at `i`, returning the exclusive
/// token index just past the match. A `*` element matches any one token.
fn phrase_at(toks: &[Tok], i: usize, phrase: &[String]) -> Option<usize> {
    if phrase.is_empty() || i + phrase.len() > toks.len() {
        return None;
    }
    for (k, want) in phrase.iter().enumerate() {
        if want != "*" && toks[i + k].text != *want {
            return None;
        }
    }
    Some(i + phrase.len())
}

/// The token index a clause's tests start from: one leading coordinator is
/// skipped, so `and it does not ...` and `it does not ...` read alike.
fn clause_head(toks: &[Tok], coordinators: &HashSet<String>) -> usize {
    match toks.first() {
        Some(t) if coordinators.contains(&t.text) => 1,
        _ => 0,
    }
}

/// The closed subject occupying the head of a clause.
fn head_subject(toks: &[Tok], head: usize, subjects: &[Vec<String>]) -> Option<(String, usize)> {
    subjects
        .iter()
        .find_map(|p| phrase_at(toks, head, p).map(|end| (p.join(" "), end)))
}

/// Whether a token is a base-form verb, by English suffix. `-s` marks the
/// third person (`scores`), `-ing` and `-ed` mark participles; a word ending
/// in `ss`, `us`, or `is` (`process`, `focus`, `axis`) is not inflected. The
/// test runs only on the word a clause-initial negation governs, and every
/// misreading resolves the same way the rule behaved before the test
/// existed: `ping` and `feed` read as inflected, so their clause is not
/// excluded, and it then fails the family tests for want of a subject.
fn base_form(word: &str) -> bool {
    if word.ends_with("ing") || word.ends_with("ed") {
        return false;
    }
    if word.ends_with('s') {
        return word.ends_with("ss") || word.ends_with("us") || word.ends_with("is");
    }
    true
}

/// Trim a byte range to its non-whitespace extent. `None` when nothing is
/// left.
fn trim_range(src: &str, range: Range<usize>) -> Option<Range<usize>> {
    let slice = src.get(range.clone())?;
    let lead = slice.len() - slice.trim_start().len();
    let trail = slice.len() - slice.trim_end().len();
    let out = (range.start + lead)..(range.end - trail);
    (out.start < out.end).then_some(out)
}

/// Paragraph blocks: byte ranges of the text between blank lines. A blank
/// line is the only block break, so an ordinary hard-wrapped paragraph stays
/// one block and its sentences remain adjacent.
fn blocks(src: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut pos = 0usize;
    for line in src.split_inclusive('\n') {
        let end = pos + line.len();
        if line.trim().is_empty() {
            if let Some(r) = trim_range(src, start..pos) {
                out.push(r);
            }
            start = end;
        }
        pos = end;
    }
    if let Some(r) = trim_range(src, start..src.len()) {
        out.push(r);
    }
    out
}

/// Sentence ranges inside a block. A sentence closes at `!`, `?`, a terminal
/// `.` (`period_is_terminal`, so `U.S.` does not split one), or a line break,
/// which keeps a heading or a list item from running into the text below it.
fn sentences(src: &str, block: &Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = block.start;
    let mut i = block.start;
    while i < block.end {
        let Some(c) = src[i..].chars().next() else {
            break;
        };
        let next = i + c.len_utf8();
        let breaks = match c {
            '\n' | '!' | '?' => true,
            '.' => period_is_terminal(src, next),
            _ => false,
        };
        if breaks {
            if let Some(r) = trim_range(src, start..next) {
                out.push(r);
            }
            start = next;
        }
        i = next;
    }
    if let Some(r) = trim_range(src, start..block.end) {
        out.push(r);
    }
    out
}

/// Clause ranges inside a sentence. A comma or a semicolon divides the
/// sentence and belongs to neither side.
fn clauses(src: &str, sentence: &Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = sentence.start;
    for (i, c) in src[sentence.clone()].char_indices() {
        if matches!(c, ',' | ';') {
            let at = sentence.start + i;
            if let Some(r) = trim_range(src, start..at) {
                out.push(r);
            }
            start = at + c.len_utf8();
        }
    }
    if let Some(r) = trim_range(src, start..sentence.end) {
        out.push(r);
    }
    out
}

/// The negated self-capability clause, in the two spellings the rule
/// accepts. Spelling A: a positive subject at the clause head, an explicit
/// negation within the subject window, and a capability verb within the verb
/// window after that negation, so intervening adverbs are free. Spelling B:
/// a negative subject, which carries its own negation, and a capability verb
/// within the subject window. Both spellings require the capability verb.
/// Denying a function verb (`the check does not fire`) is an honest scope
/// fact and stays out.
fn denied_capability(toks: &[Tok], head: usize, dr: &DenialRule) -> Option<String> {
    let verb_at = |from: usize, width: usize| {
        toks[from.min(toks.len())..(from + width).min(toks.len())]
            .iter()
            .any(|t| dr.capability_verbs.contains(&t.text))
    };
    if let Some((subject, after)) = head_subject(toks, head, &dr.subjects) {
        for start in after..(after + dr.window).min(toks.len()) {
            let negated = dr
                .imperative_negations
                .iter()
                .chain(&dr.finite_negations)
                .filter_map(|n| phrase_at(toks, start, n))
                .any(|end| verb_at(end, dr.verb_window));
            if negated {
                return Some(subject);
            }
        }
    }
    if let Some((subject, after)) = head_subject(toks, head, &dr.negative_subjects) {
        if verb_at(after, dr.window) {
            return Some(subject);
        }
    }
    // Spelling C, the subjectless clause: the negation sits at the head with
    // its subject elided from the clause before it. A finite-only negation
    // takes a base or third-person capability verb. An imperative-capable
    // negation takes only the third-person form, because a base form there is
    // a command (`Never score voice.`) and an `-ing` form is a participial
    // adjunct (`She listened, never judging anyone.`).
    let elided = |negations: &[Vec<String>], forms: &HashSet<String>| {
        negations
            .iter()
            .filter_map(|n| phrase_at(toks, head, n))
            .any(|after| {
                toks[after.min(toks.len())..(after + dr.verb_window).min(toks.len())]
                    .iter()
                    .any(|t| forms.contains(&t.text))
            })
    };
    if elided(&dr.finite_negations, &dr.capability_finite)
        || elided(&dr.imperative_negations, &dr.capability_third)
    {
        return Some(String::new());
    }
    None
}

/// The coordinated case: `and never detect authorship` at the tail of a
/// sentence whose earlier segment names the thing. Read alone the segment is
/// a command, because an imperative-capable negation governs a base-form
/// verb, and `imperative_clause` drops it for that reason. Read in place it
/// continues the subject of the segment before it, and what it denies is a
/// capability.
///
/// This test covers the segment half of that reading: the coordinator is
/// exactly `and`, so `but`, `so`, and the rest keep the command reading; the
/// negation heads the segment past the skip; and the verb it governs is a
/// base form from the closed capability set. The sentence half, an earlier
/// segment carrying a closed-set subject, belongs to `scan_denial`, which is
/// the only place a segment can see its neighbours.
fn coordinated_denial(toks: &[Tok], head: usize, dr: &DenialRule) -> bool {
    if head == 0 || toks.first().map(|t| t.text.as_str()) != Some("and") {
        return false;
    }
    dr.imperative_negations
        .iter()
        .filter_map(|n| phrase_at(toks, head, n))
        .any(|after| {
            toks[after.min(toks.len())..(after + dr.verb_window).min(toks.len())]
                .iter()
                .any(|t| dr.capability_base.contains(&t.text))
        })
}

/// The imperative test, run on one clause before any family test. The clause
/// is a command when its head token opens a negation phrase and the word
/// that negation governs is a base-form verb (`Do not obey`). A clause-head
/// negation governing an inflected verb is the middle of a denial stack with
/// its subject elided (`never scores voice`) and stays in.
fn imperative_clause(toks: &[Tok], head: usize, dr: &DenialRule) -> bool {
    let Some(after) = dr
        .imperative_negations
        .iter()
        .find_map(|n| phrase_at(toks, head, n))
    else {
        return false;
    };
    toks.get(after).map(|t| base_form(&t.text)).unwrap_or(true)
}

/// Cut one comma clause at every interior coordinator. The coordinator opens
/// the segment after it, where the leading-coordinator skip already reads it,
/// so `It does not detect authorship and never scores voice.` becomes two
/// segments and the stack arm can see them both. This is the same
/// segmentation pass carried one level down, not a second splitter. Cutting
/// everywhere is safe because qualification is self-gating: a bare
/// coordinated tail carries neither its own subject nor a head negation, so
/// `... that a person or a model wrote anything` still yields exactly one
/// qualifying segment.
fn split_at_coordinators(
    src: &str,
    clause: &Range<usize>,
    toks: &[Tok],
    dr: &DenialRule,
) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = clause.start;
    // Skip index 0: a coordinator already at the head belongs to this clause.
    for t in toks.iter().skip(1) {
        if dr.coordinators.contains(&t.text) {
            if let Some(r) = trim_range(src, start..t.start) {
                out.push(r);
            }
            start = t.start;
        }
    }
    if let Some(r) = trim_range(src, start..clause.end) {
        out.push(r);
    }
    out
}

/// One classified segment. `qualifies` marks a denial or a hedge;
/// `affirmative` marks a segment that can serve as the partner arm, meaning
/// it names a closed-set subject and denies nothing. `referent` is what a
/// qualifying segment claims to be about, absent when its subject is elided.
/// `report` is the span the finding cites: the segment itself for the three
/// subject-and-verb spellings, which is the unit a writer edits, and the
/// whole comma clause for an evidential hedge, whose phrases govern an open
/// complement.
struct ClauseFacts {
    report: Range<usize>,
    qualifies: bool,
    affirmative: bool,
    subject: Option<String>,
    referent: Option<String>,
    /// Whether this segment can carry the adjacency arm. A denial names its
    /// own subject or elides it from the clause before, so it can. An
    /// evidential hedge over a subject outside the closed set is about
    /// something else, so coreference cannot be tested and the segment
    /// counts toward the stack arm alone.
    arm_b_eligible: bool,
    /// Whether the segment carries the coordinated shape (`and never detect
    /// authorship`). The sentence loop decides what to do with it.
    coordinated: bool,
}

/// The span a finding cites. It opens where the analysis opened, at the
/// first token past the leading-coordinator skip, and closes on the last
/// byte of clause content. Trailing whitespace and delimiters are left out:
/// a mid-sentence comma or semicolon, and a sentence-final `.`, `!`, or `?`.
/// So a span never opens on a coordinator and never closes on sentence
/// punctuation, and the cited text is the writer's own words.
fn reported_span(src: &str, range: &Range<usize>, dr: &DenialRule) -> Range<usize> {
    let toks = tokens_in(src, range);
    let head = clause_head(&toks, &dr.coordinators);
    let start = toks.get(head).map_or(range.start, |t| t.start);
    let mut end = range.end.max(start);
    while end > start {
        let Some(c) = src[..end].chars().next_back() else {
            break;
        };
        if c.is_whitespace() || matches!(c, '.' | '!' | '?' | ',' | ';') {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    start..end
}

/// Run every segment-level test once. The imperative exclusion comes first,
/// so a command neither qualifies nor stands in as an affirmative partner.
fn classify_clause(
    src: &str,
    segment: Range<usize>,
    clause: &Range<usize>,
    dr: &DenialRule,
) -> ClauseFacts {
    let toks = tokens_in(src, &segment);
    let head = clause_head(&toks, &dr.coordinators);
    let subject = head_subject(&toks, head, &dr.subjects).map(|(s, _)| s);
    if imperative_clause(&toks, head, dr) {
        return ClauseFacts {
            report: reported_span(src, &segment, dr),
            qualifies: false,
            affirmative: false,
            subject,
            referent: None,
            arm_b_eligible: false,
            coordinated: coordinated_denial(&toks, head, dr),
        };
    }
    let denied = denied_capability(&toks, head, dr);
    // An open hedge form (`no <one or two words> is evidence`) carries its
    // head noun in the last wildcard slot. That noun is what the clause is
    // about, so it takes the closed-set test in place of a head subject.
    let mut hedged = false;
    let mut hedge_head = None;
    for i in 0..toks.len() {
        for h in &dr.hedges {
            if phrase_at(&toks, i, h).is_some() {
                hedged = true;
                if let Some(slot) = h.iter().rposition(|p| p == "*") {
                    hedge_head = Some(toks[i + slot].text.clone());
                }
            }
        }
    }
    let hedge_head = hedge_head.filter(|n| dr.tool_nouns.contains(n));
    let family_one = denied.is_some();
    // An empty string is the subjectless spelling: the segment qualifies and
    // names no referent of its own.
    let referent = denied.filter(|s| !s.is_empty());
    // A hedge borrows the adjacency arm only when it names a closed-set
    // thing, at its head subject or as the head noun of an open form. That
    // is what makes coreference testable.
    let arm_b_eligible = family_one || subject.is_some() || hedge_head.is_some();
    ClauseFacts {
        report: reported_span(src, if family_one { &segment } else { clause }, dr),
        qualifies: family_one || hedged,
        affirmative: !(family_one || hedged) && subject.is_some(),
        referent: if family_one {
            referent
        } else {
            subject.clone().or(hedge_head)
        },
        subject,
        arm_b_eligible,
        coordinated: false,
    }
}

/// Pass 8: the capability-denial scan (SD-Q007). Within one block, a
/// segment qualifies when it denies a capability of the closed subject (see
/// `denied_capability` for the three spellings) or carries an evidential
/// hedge phrase. The imperative test runs per segment first, so one command
/// at the head of a sentence cannot carry a denial stack behind it. Arm A is
/// the stack: two qualifying segments anywhere in the block. Arm B is one
/// qualifying segment beside an affirmative partner, searched in the ruled
/// order. One finding per qualifying segment, never one per partner, and
/// each finding names the arm that fired.
fn scan_denial(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    for dr in &cp.denial_rules {
        for block in blocks(src) {
            let sentences = sentences(src, &block);
            // One segmentation pass serves every test. Each sentence becomes
            // its final segment list, and each segment is classified once.
            let table: Vec<Vec<ClauseFacts>> = sentences
                .iter()
                .map(|sentence| {
                    let mut facts = Vec::new();
                    for clause in clauses(src, sentence) {
                        let toks = tokens_in(src, &clause);
                        for segment in split_at_coordinators(src, &clause, &toks, dr) {
                            facts.push(classify_clause(src, segment, &clause, dr));
                        }
                    }
                    // The coordinated case, decided here because it is the
                    // only test that reads one segment against another. An
                    // `and` segment that reads as a command on its own is a
                    // denial when an earlier segment of the same sentence
                    // named the thing, because that is the subject it
                    // continues. It borrows that subject rather than naming
                    // one, so it takes the absent-subject key and the
                    // adjacency arm reads it as coreferent by definition.
                    for i in 0..facts.len() {
                        if facts[i].coordinated && facts[..i].iter().any(|f| f.subject.is_some()) {
                            facts[i].qualifies = true;
                            facts[i].referent = None;
                            facts[i].arm_b_eligible = true;
                        }
                    }
                    facts
                })
                .collect();

            // One referent when the two subjects match, when either is a bare
            // closed-set pronoun (it refers back to its neighbour), or when
            // both name the same tool noun, counting a singular and its
            // plural as one word. Two different tool nouns are two things, so
            // they do not corefer. A segment whose own subject is elided
            // names no referent and takes any partner.
            let lemma = |s: &str| {
                s.split(' ').rev().find_map(|t| {
                    dr.tool_nouns.contains(t).then(|| {
                        t.strip_suffix('s')
                            .filter(|base| dr.tool_nouns.contains(*base))
                            .unwrap_or(t)
                            .to_string()
                    })
                })
            };
            let coreferent = |a: &Option<String>, b: &str| match a {
                None => true,
                Some(a) => {
                    a == b
                        || dr.pronouns.contains(a)
                        || dr.pronouns.contains(b)
                        || lemma(a).zip(lemma(b)).is_some_and(|(x, y)| x == y)
                }
            };
            // The affirmative partner search, in the ruled order, stopping at
            // the first match: the qualifying segment's own sentence at any
            // distance, then the sentence before, then the sentence after.
            let partner_in = |si: usize, skip: Option<usize>, referent: &Option<String>| {
                table.get(si).is_some_and(|facts| {
                    facts.iter().enumerate().any(|(ci, f)| {
                        Some(ci) != skip
                            && f.affirmative
                            && f.subject.as_ref().is_some_and(|s| coreferent(referent, s))
                    })
                })
            };
            let has_partner = |si: usize, ci: usize, referent: &Option<String>| {
                partner_in(si, Some(ci), referent)
                    || si
                        .checked_sub(1)
                        .is_some_and(|prev| partner_in(prev, None, referent))
                    || partner_in(si + 1, None, referent)
            };

            let qualifying: Vec<(usize, usize)> = table
                .iter()
                .enumerate()
                .flat_map(|(si, facts)| {
                    facts
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.qualifies)
                        .map(move |(ci, _)| (si, ci))
                })
                .collect();
            // The arm is readable from the findings themselves: several in
            // one block is the stack, one is the adjacency form. Nothing is
            // written into the report to say so.
            let fires = qualifying.len() >= 2
                || qualifying.first().is_some_and(|&(si, ci)| {
                    table[si][ci].arm_b_eligible && has_partner(si, ci, &table[si][ci].referent)
                });
            if !fires {
                continue;
            }
            for (si, ci) in qualifying {
                hits.push(Hit {
                    rule: dr.rule,
                    span: table[si][ci].report.clone(),
                });
            }
        }
    }
}

/// Pass 9: the rationale-leak scan (SD-Q008). A design-economics or
/// reception-instruction marker fires only when its sentence also names a
/// tool noun, at any position. That anchor is the whole precision budget:
/// it keeps ordinary adverbs (`she deliberately ignored him`, `that was
/// deliberately vague`) silent. One finding per marker occurrence, spanning
/// the marker.
fn scan_rationale(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    for rr in &cp.rationale_rules {
        for block in blocks(src) {
            for sentence in sentences(src, &block) {
                let toks = tokens_in(src, &sentence);
                let anchored = toks.iter().any(|t| rr.tool_nouns.contains(&t.text));
                if !anchored {
                    continue;
                }
                for i in 0..toks.len() {
                    for marker in &rr.markers {
                        if let Some(end) = phrase_at(&toks, i, marker) {
                            hits.push(Hit {
                                rule: rr.rule,
                                span: toks[i].start..toks[end - 1].end,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Pass 2: the overlapping adapter over regex-automata's DFAs. The forward
/// DFA yields (pattern, end) pairs; the reverse DFA anchored to the pattern
/// and bounded by the pattern's max width recovers the start.
fn scan_rx(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    if cp.rx_meta.is_empty() || src.is_empty() {
        return;
    }
    let mut fwd_cache: Cache = cp.rx_fwd.create_cache();
    let mut rev_cache: Cache = cp.rx_rev.create_cache();
    let input = Input::new(src);
    let mut state = OverlappingState::start();
    let mut seen: HashSet<(usize, usize, usize)> = HashSet::new();
    loop {
        if cp
            .rx_fwd
            .try_search_overlapping_fwd(&mut fwd_cache, &input, &mut state)
            .is_err()
        {
            // A cache failure cannot invent findings; the scan stops with
            // whatever was already found.
            return;
        }
        let Some(hm) = state.get_match() else { break };
        let pid = hm.pattern();
        let end = hm.offset();
        let meta = &cp.rx_meta[pid.as_usize()];
        // Bound the reverse start-recovery window by the pattern's max
        // width: the true start is at most that many bytes before `end`.
        let rev_lo = match meta.max_width {
            Some(w) => end.saturating_sub(w),
            None => 0,
        };
        let rin = Input::new(src)
            .range(rev_lo..end)
            .anchored(Anchored::Pattern(pid));
        let start = match cp.rx_rev.try_search_rev(&mut rev_cache, &rin) {
            Ok(Some(h)) => h.offset(),
            _ => continue,
        };
        if start >= end || !seen.insert((pid.as_usize(), start, end)) {
            continue;
        }
        // The DFA matched the ASCII `\b` prefilter form; re-validate the
        // declared edges against real Unicode word boundaries.
        if (meta.bound_start && !unicode_word_boundary(src, start))
            || (meta.bound_end && !unicode_word_boundary(src, end))
        {
            continue;
        }
        hits.push(Hit {
            rule: meta.rule,
            span: start..end,
        });
    }
}

/// Passes 3 and 4: one `char_indices` walk serving the codepoint-class rules
/// (adjacent same-rule codepoints merge into one span) and the
/// positional-space rules (candidates between two alphabetic neighbors,
/// digit-adjacent excluded by that predicate, emitted only when the
/// per-document candidate count meets the rule's minimum).
fn scan_codepoints(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    let mut open: Vec<Option<Range<usize>>> = vec![None; cp.cp_rules.len()];
    let mut candidates: Vec<Vec<Range<usize>>> = vec![Vec::new(); cp.space_rules.len()];
    let mut prev: Option<char> = None;
    let mut prev2: Option<char> = None;
    let mut iter = src.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let v = c as u32;
        let end = i + c.len_utf8();
        let next = iter.peek().map(|&(_, n)| n);
        for (slot, (rule_idx, ranges)) in cp.cp_rules.iter().enumerate() {
            if ranges.iter().any(|&(lo, hi)| lo <= v && v <= hi) {
                let rule = &cp.rules[*rule_idx];
                // A leading U+FEFF is an editor byte-order mark; a U+FE0E or
                // U+FE0F right after a visible base character is an ordinary
                // presentation selector (emoji text). Neither is residue. A
                // selector preceded by another in-range codepoint still
                // fires: invisible runs stay evidence.
                let in_range = |c: char| {
                    ranges
                        .iter()
                        .any(|&(lo, hi)| lo <= c as u32 && c as u32 <= hi)
                };
                // A ZWNJ or ZWJ between two joining-script characters or two
                // pictographic characters is orthography (Indic and Arabic
                // shaping, emoji ZWJ sequences), not residue. The backward
                // neighbor skips one presentation selector, because emoji
                // sequences interleave U+FE0F before the joiner. A joiner
                // between ordinary prose characters still fires.
                let joining_exempt = || {
                    let back = match prev {
                        Some(p) if p as u32 == 0xFE0E || p as u32 == 0xFE0F => prev2,
                        p => p,
                    };
                    match (back, next) {
                        (Some(b), Some(f)) => {
                            (joining_script(b) && joining_script(f))
                                || (pictographic(b) && pictographic(f))
                        }
                        _ => false,
                    }
                };
                if (rule.exempt_leading_bom && v == 0xFEFF && i == 0)
                    || (rule.exempt_presentation_selector
                        && (v == 0xFE0E || v == 0xFE0F)
                        && prev.map(|p| !in_range(p)).unwrap_or(false))
                    || (rule.exempt_joining_zwj && (v == 0x200C || v == 0x200D) && joining_exempt())
                {
                    continue;
                }
                match &mut open[slot] {
                    Some(r) if r.end == i => r.end = end,
                    r => {
                        if let Some(done) = r.take() {
                            hits.push(Hit {
                                rule: *rule_idx,
                                span: done,
                            });
                        }
                        *r = Some(i..end);
                    }
                }
            }
        }
        for (slot, (_, codepoints, _)) in cp.space_rules.iter().enumerate() {
            if codepoints.contains(&v)
                && prev.map(char::is_alphabetic).unwrap_or(false)
                && next.map(char::is_alphabetic).unwrap_or(false)
            {
                candidates[slot].push(i..end);
            }
        }
        prev2 = prev;
        prev = Some(c);
    }
    for (slot, (rule_idx, _)) in cp.cp_rules.iter().enumerate() {
        if let Some(done) = open[slot].take() {
            hits.push(Hit {
                rule: *rule_idx,
                span: done,
            });
        }
    }
    for (slot, (rule_idx, _, min_count)) in cp.space_rules.iter().enumerate() {
        if candidates[slot].len() >= *min_count {
            for span in candidates[slot].drain(..) {
                hits.push(Hit {
                    rule: *rule_idx,
                    span,
                });
            }
        }
    }
}

/// Pass 7: the within-document self-duplication scan (SD-Q005), ported
/// from ai-slop's SLOP-U001 in its memory-frugal form (see the
/// `duplication` module). One hit per repeat occurrence (second and
/// later), span = the later copy, capped at `max_reports` longest-first.
/// Raw bytes throughout: fenced content shingles like everything else,
/// and the container pre-pass annotates what lands inside a fence.
fn scan_duplication(cp: &Compiled, src: &str, hits: &mut Vec<Hit>) {
    use crate::duplication;
    if cp.duplication_rules.is_empty() {
        return;
    }
    let mut tokens = duplication::Tokens::new();
    duplication::tokenize_into(&mut tokens, src, 0, 0);
    for dr in &cp.duplication_rules {
        let mut runs = duplication::find_runs(&tokens, dr.shingle_words, dr.min_run_words, false);
        duplication::cap_longest_first(&mut runs, dr.max_reports);
        for run in runs {
            let toks = &tokens.toks;
            hits.push(Hit {
                rule: dr.rule,
                span: toks[run.later].start..toks[run.later + run.len - 1].end,
            });
        }
    }
}

/// Run every pass over one source text.
pub fn scan_all(cp: &Compiled, src: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    scan_ac(cp, src, &mut hits);
    scan_rx(cp, src, &mut hits);
    scan_codepoints(cp, src, &mut hits);
    scan_participial(cp, src, &mut hits);
    scan_contrastive(cp, src, &mut hits);
    scan_duplication(cp, src, &mut hits);
    scan_denial(cp, src, &mut hits);
    scan_rationale(cp, src, &mut hits);
    resolve_overlaps(&mut hits);
    hits
}

/// Within one rule, a span contained in a wider span of the same rule merges
/// into it (`utm_source=chatgpt` inside `utm_source=chatgpt.com` reports
/// once, at the wider span). Exact duplicates merge the same way.
fn resolve_overlaps(hits: &mut Vec<Hit>) {
    hits.sort_by(|a, b| {
        (a.rule, a.span.start, std::cmp::Reverse(a.span.end)).cmp(&(
            b.rule,
            b.span.start,
            std::cmp::Reverse(b.span.end),
        ))
    });
    let mut out: Vec<Hit> = Vec::new();
    for h in hits.drain(..) {
        if let Some(prev) = out.last() {
            if prev.rule == h.rule && h.span.start >= prev.span.start && h.span.end <= prev.span.end
            {
                continue;
            }
        }
        out.push(h);
    }
    *hits = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_compiles() {
        // Guards the `expect` in `compiled`: the embedded table and lexicons
        // build every engine.
        let cp = compiled();
        assert!(!cp.rules.is_empty());
        assert!(!cp.rx_meta.is_empty());
        assert!(!cp.cp_rules.is_empty());
        assert_eq!(cp.space_rules.len(), 1);
        assert_eq!(cp.participial_rules.len(), 1);
        assert_eq!(cp.contrastive_rules.len(), 1);
        assert_eq!(cp.duplication_rules.len(), 1);
        assert_eq!(cp.denial_rules.len(), 1);
        assert_eq!(cp.rationale_rules.len(), 1);
    }

    #[test]
    fn phrase_tokens_split_a_hyphenated_marker_like_the_source_does() {
        assert_eq!(
            phrase_tokens("the trade-off is"),
            ["the", "trade", "off", "is"]
        );
        assert_eq!(
            phrase_tokens("no * is evidence"),
            ["no", "*", "is", "evidence"]
        );
        let src = "It carries the trade-off is nowhere.";
        let toks = tokens_in(src, &(0..src.len()));
        let words: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            words,
            ["it", "carries", "the", "trade", "off", "is", "nowhere"]
        );
    }

    #[test]
    fn sentences_split_on_line_breaks_and_real_terminals_only() {
        let src = "The U.S. team shipped it. Next line\nA heading";
        let block = blocks(src);
        assert_eq!(block.len(), 1);
        let s: Vec<&str> = sentences(src, &block[0])
            .into_iter()
            .map(|r| &src[r])
            .collect();
        assert_eq!(s, ["The U.S. team shipped it.", "Next line", "A heading"]);
    }

    #[test]
    fn base_form_reads_english_verb_suffixes() {
        for base in [
            "obey", "author", "sign", "judge", "process", "focus", "discuss",
        ] {
            assert!(base_form(base), "{base}");
        }
        for inflected in ["scores", "detects", "judging", "rated"] {
            assert!(!base_form(inflected), "{inflected}");
        }
    }

    #[test]
    fn clauses_divide_on_commas_and_semicolons() {
        let src = "It reads text; it does not detect authorship, and no rule scores voice.";
        let block = blocks(src);
        let sent = sentences(src, &block[0]);
        assert_eq!(sent.len(), 1);
        let c: Vec<&str> = clauses(src, &sent[0])
            .into_iter()
            .map(|r| &src[r])
            .collect();
        assert_eq!(
            c,
            [
                "It reads text",
                "it does not detect authorship",
                "and no rule scores voice."
            ]
        );
    }

    #[test]
    fn blank_lines_separate_blocks() {
        let src = "One.\n\nTwo.\n";
        let b: Vec<&str> = blocks(src).into_iter().map(|r| &src[r]).collect();
        assert_eq!(b, ["One.", "Two."]);
    }

    #[test]
    fn bounded_width_gate_rejects_unbounded_quantifiers() {
        assert!(validate_bounded_width(r"\d+").is_err());
        assert!(validate_bounded_width(r".*").is_err());
        assert!(validate_bounded_width(r"a\s+b").is_err());
        assert!(matches!(
            validate_bounded_width(r"turn\d{1,4}search\d{0,4}"),
            Ok(Some(_))
        ));
    }

    #[test]
    fn rewrite_rejects_look_arounds() {
        assert!(rewrite_pattern(r"(?<=\w)foo").is_err());
        assert!(rewrite_pattern(r"foo(?=bar)").is_err());
        assert!(rewrite_pattern(r"\bfoo\b").is_ok());
    }

    #[test]
    fn contained_same_rule_spans_merge() {
        let mut hits = vec![
            Hit {
                rule: 1,
                span: 5..25,
            },
            Hit {
                rule: 1,
                span: 5..17,
            },
            Hit {
                rule: 2,
                span: 5..17,
            },
        ];
        resolve_overlaps(&mut hits);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.rule == 1 && h.span == (5..25)));
        assert!(hits.iter().any(|h| h.rule == 2 && h.span == (5..17)));
    }
}
