//! SD-Q005 verbatim self-duplication and the bundle-mode cross-file scan.
//! Ported from ai-slop's SLOP-U001 duplication engine, memory-frugal form:
//! byte-range tokens over ONE shared fold buffer (no owned String per
//! word), a `HashMap<u64, usize>` of first-seen shingle heads with an
//! intrusive per-token `next` chain (no heap Vec per distinct shingle), and
//! exact token comparison verifying every hash revisit — so determinism
//! never depends on hash values and the matcher itself cannot
//! false-positive: every emitted run is a true verbatim repeat of at least
//! `min_run_words` words.
//!
//! slop-detector scans raw bytes with no prose/code segmentation: fenced
//! and quoted material tokenizes like everything else, and a duplicated
//! run inside a fence reports with its `container = fenced-code`
//! annotation — annotate, never skip. The only segment boundary is the
//! genuine one: the file boundary in bundle mode (the caller bumps `seg`
//! between files), so a run never fuses across two files.
//!
//! The scan chains EVERY processed anchor (not just the first carrier of a
//! hash) and, on a hash revisit, walks up to `WALK_CAP` chain entries,
//! extends each verified candidate forward AND backward, and keeps the
//! candidate with the maximal TOTAL run — an early prefix-sharing decoy
//! that diverges below the floor cannot displace the genuine duplicate
//! between later copies. Recall is bounded, not absolute: more than
//! `WALK_CAP` same-prefix occurrences sitting between a genuine pair can
//! exhaust the walk before the true partner is reached and mask it (an
//! attacker-unrealistic shape — a document already carrying 32+ copies of
//! one 8-word prefix is its own finding). The scan advances past every
//! emitted run, so the whole pass stays near-linear (O(WALK_CAP * tokens)
//! bounded work, never O(N^2)) with memory proportional to the token
//! count.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub(crate) struct Tok {
    /// Byte span in the token's own source text.
    pub start: usize,
    pub end: usize,
    /// Byte offset of this token's folded word in the shared buffer. The
    /// word ends where the next token's word begins (buffer end for the
    /// last token): the buffer is the exact concatenation of the folded
    /// words, so no per-word length needs storing.
    word: usize,
    /// File segment id: a shingle or run never spans two segments. Bumped
    /// by the caller between bundle files; constant within one document.
    seg: u32,
    /// Bundle file index; 0 for the single-document scan.
    pub file: u32,
}

/// Tokenizer output: byte-range tokens plus one shared buffer holding
/// every folded word back to back.
pub(crate) struct Tokens {
    pub toks: Vec<Tok>,
    buf: String,
}

impl Tokens {
    pub(crate) fn new() -> Self {
        Tokens {
            toks: Vec::new(),
            buf: String::new(),
        }
    }

    /// The folded word carried by token `i`.
    fn word(&self, i: usize) -> &str {
        let s = self.toks[i].word;
        let e = self
            .toks
            .get(i + 1)
            .map(|t| t.word)
            .unwrap_or(self.buf.len());
        &self.buf[s..e]
    }

    /// Element-wise equality of the k-word shingles at `a` and `b`.
    fn shingles_eq(&self, a: usize, b: usize, k: usize) -> bool {
        (0..k).all(|d| self.word(a + d) == self.word(b + d))
    }
}

/// Append the lowercased word tokens of `text` (alphanumeric plus
/// apostrophe, typographic apostrophe folded — the `first_token` charset
/// from the contrastive-tail scan). Every byte tokenizes, fenced content
/// included: SD-Q005 sees the same raw bytes as every other rule, and the
/// container pre-pass annotates what lands inside a fence. The caller
/// bumps `seg` between bundle files, the one genuine boundary.
pub(crate) fn tokenize_into(tokens: &mut Tokens, text: &str, file: u32, seg: u32) {
    let mut in_word = false;
    let mut start = 0usize;
    let mut word = 0usize;
    for (i, c) in text.char_indices() {
        let c = if c == '\u{2019}' { '\'' } else { c };
        if c.is_alphanumeric() || c == '\'' {
            if !in_word {
                start = i;
                word = tokens.buf.len();
                in_word = true;
            }
            for lc in c.to_lowercase() {
                tokens.buf.push(lc);
            }
        } else if in_word {
            tokens.toks.push(Tok {
                start,
                end: i,
                word,
                seg,
                file,
            });
            in_word = false;
        }
    }
    if in_word {
        tokens.toks.push(Tok {
            start,
            end: text.len(),
            word,
            seg,
            file,
        });
    }
}

fn shingle_hash(tokens: &Tokens, i: usize, k: usize) -> u64 {
    // Fixed-key SipHash: deterministic across runs and processes. Output
    // correctness does not depend on it — collisions are resolved by the
    // exact token comparison in the scan.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for d in 0..k {
        tokens.word(i + d).hash(&mut h);
    }
    h.finish()
}

/// One verified maximal run: token indices of the earlier copy, the later
/// copy, and the shared length in words.
pub(crate) struct Run {
    pub earlier: usize,
    pub later: usize,
    pub len: usize,
}

/// The shared shingle scan. `cross_file_only` is the bundle mode: verified
/// same-file revisits are skipped (each file's own SD-Q005 pass owns
/// them), and the same-file disjointness guard is replaced by the segment
/// discipline, since copies in different files cannot overlap.
pub(crate) fn find_runs(
    tokens: &Tokens,
    k: usize,
    floor: usize,
    cross_file_only: bool,
) -> Vec<Run> {
    let toks = &tokens.toks;
    let mut runs = Vec::new();
    if k == 0 || toks.len() < k {
        return runs;
    }
    // Shingle hash -> most-recent token index carrying that hash, with
    // earlier carriers chained through `next` (an intrusive singly linked
    // list: each token index sits in at most one chain, so one
    // preallocated slot per token suffices). EVERY processed anchor joins
    // its chain, verified or not: keeping only one representative per
    // distinct sequence is the prefix-decoy hole — an early occurrence
    // that shares the k-word prefix but diverges below the floor would
    // hold the slot and block the genuine duplicate between later copies.
    // A revisit therefore walks the chain (most recent first, capped at
    // `WALK_CAP` entries) and keeps the MAXIMAL verified eligible run, so
    // occ2-vs-occ3 and occ1-vs-occ3-across-a-decoy both land. The cap
    // trades absolute recall for the near-linear bound: more than
    // `WALK_CAP` same-prefix occurrences between a genuine pair can
    // exhaust the walk before the true partner and mask it —
    // attacker-unrealistic, since 32+ copies of one 8-word prefix are
    // already the loudest thing in the document. The cap bounds the walk
    // on a phrase repeated N times: sub-floor candidates cost under
    // `floor` comparisons each way, and a candidate at or above the floor
    // emits and advances `i` past the run, so total work stays
    // O(WALK_CAP * tokens) — near-linear, never O(N^2).
    const WALK_CAP: usize = 32;
    const NIL: usize = usize::MAX;
    let mut heads: HashMap<u64, usize> = HashMap::new();
    let mut next: Vec<usize> = vec![NIL; toks.len()];
    let mut i = 0usize;
    while i + k <= toks.len() {
        if toks[i + k - 1].seg != toks[i].seg {
            i += 1; // shingle spans a file boundary
            continue;
        }
        match heads.entry(shingle_hash(tokens, i, k)) {
            Entry::Vacant(v) => {
                v.insert(i);
                i += 1;
            }
            Entry::Occupied(mut o) => {
                // Walk the chain, most recent first. Eligibility per mode:
                // the bundle scan pairs cross-file copies only (each
                // file's own SD-Q005 pass owns same-file repeats), and the
                // single-document scan requires disjoint copies — an
                // anchor overlapping its own revisit ("the the the") is
                // repetition inside one passage, not a duplicated passage.
                // A hash collision fails `shingles_eq` and is skipped the
                // same way. Every verified candidate is extended forward
                // AND backward before ranking, so candidates compete on
                // their TOTAL run — ranking on forward length alone would
                // let a candidate that extends far forward beat one whose
                // run reaches further backward, reporting a non-maximal
                // run. Length ties keep the LAST candidate walked: the
                // chain is strictly decreasing in position, so
                // equal-length copies anchor on the EARLIEST occurrence —
                // a deliberate divergence from ai-slop's most-recent tie,
                // so that every later copy of one passage shares one
                // anchor and bundle grouping folds them into one entry.
                // `best` holds (earlier start, later start, total len).
                let mut best: Option<(usize, usize, usize)> = None;
                let mut e = *o.get();
                let mut walked = 0usize;
                loop {
                    let same_file = toks[e].file == toks[i].file;
                    let eligible = if cross_file_only {
                        !same_file
                    } else {
                        e + k <= i
                    };
                    if eligible && tokens.shingles_eq(e, i, k) {
                        // Extend greedily to the maximal shared run,
                        // keeping same-file copies disjoint (`e + len <=
                        // i`) and each side inside one file segment.
                        let mut len = k;
                        while i + len < toks.len()
                            && (!same_file || e + len < i)
                            && tokens.word(e + len) == tokens.word(i + len)
                            && toks[e + len].seg == toks[e].seg
                            && toks[i + len].seg == toks[i].seg
                        {
                            len += 1;
                        }
                        // Extend backward to the true run start: the
                        // anchor window can sit one or more words into the
                        // real run when the run-initial windows lost their
                        // capped walks to decoy crowds on earlier passes.
                        // Same guards mirrored — same-file copies stay
                        // disjoint (the earlier copy's end `es + len` is
                        // pinned while the later start `s` moves left, so
                        // the gap must stay positive) and neither side
                        // crosses a file segment. Backward work per
                        // candidate is bounded by the run length, so the
                        // pass keeps its O(WALK_CAP * tokens) bound.
                        let (mut es, mut s, mut len) = (e, i, len);
                        while es > 0
                            && (!same_file || es + len < s)
                            && tokens.word(es - 1) == tokens.word(s - 1)
                            && toks[es - 1].seg == toks[es].seg
                            && toks[s - 1].seg == toks[s].seg
                        {
                            es -= 1;
                            s -= 1;
                            len += 1;
                        }
                        if best.is_none_or(|(_, _, b)| len >= b) {
                            best = Some((es, s, len));
                        }
                    }
                    walked += 1;
                    if walked >= WALK_CAP || next[e] == NIL {
                        break;
                    }
                    e = next[e];
                }
                // Prepend this anchor so LATER occurrences can pair with
                // it even when an older decoy shares the chain.
                next[i] = *o.get();
                o.insert(i);
                match best {
                    Some((e, s, len)) if len >= floor => {
                        runs.push(Run {
                            earlier: e,
                            later: s,
                            len,
                        });
                        // Advance past the repeated run: sub-runs of an
                        // emitted run are not separate findings. `s + len`
                        // is the anchor plus the winner's forward-extended
                        // length (backward steps move `s` left exactly as
                        // they grow `len`), so progress is at least the
                        // shingle order `k`.
                        i = s + len;
                    }
                    _ => i += 1,
                }
            }
        }
    }
    runs
}

/// Longest runs first under the emission cap, position as the
/// deterministic tiebreak.
pub(crate) fn cap_longest_first(runs: &mut Vec<Run>, cap: usize) {
    runs.sort_by_key(|r| (std::cmp::Reverse(r.len), r.later, r.earlier));
    runs.truncate(cap);
}
