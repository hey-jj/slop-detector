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

    for (idx, rule) in rules.iter().enumerate() {
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

/// True when `at` sits at a block or sentence start: the start of the text,
/// after a line break (leading whitespace and plain-text bullet markers
/// skipped), or after sentence-ending punctuation.
fn at_block_start(src: &str, at: usize) -> bool {
    for c in src[..at].chars().rev() {
        match c {
            ' ' | '\t' | '-' | '*' | '\u{2022}' => continue,
            '\n' | '\r' | '.' | '!' | '?' => return true,
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

/// Run every pass over one source text.
pub fn scan_all(cp: &Compiled, src: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    scan_ac(cp, src, &mut hits);
    scan_rx(cp, src, &mut hits);
    scan_codepoints(cp, src, &mut hits);
    scan_participial(cp, src, &mut hits);
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
