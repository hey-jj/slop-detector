//! SD-Q005 verbatim self-duplication and the bundle-mode cross-file scan.
//! The scan derives from ai-slop's SLOP-U001 duplication engine.
//! Byte-range tokens share one fold buffer. A `HashMap<u64, usize>` holds
//! the latest shingle heads, linked through one `next` slot per token.
//! Each word has no owned String, and each distinct shingle has no heap Vec.
//! Exact token comparison checks every hash revisit, so collisions cannot
//! produce a finding. Determinism never depends on hash values.
//! Each emitted run is a true verbatim repeat of at least `min_run_words`
//! words. The tokenizer uses shared storage for words and shingle chains.
//!
//! Fenced and quoted content participates in the scan and retains its
//! container annotation. The scan reads raw bytes with no prose/code
//! segmentation. The file boundary is its only segment boundary.
//! The caller increments `seg` between bundle files,
//! which prevents a run from spanning a file boundary.
//!
//! Every processed anchor joins its hash chain. A revisit walks up to
//! `WALK_CAP` entries, checks token equality, extends each eligible run
//! forward and backward, and keeps the longest total run. An early copy
//! that shares the prefix but diverges below the floor leaves later copies
//! available for comparison. More than `WALK_CAP` same-prefix occurrences
//! between two copies can exhaust the walk and hide their shared run.
//! This shape is unrealistic for an attack. A document carrying 32+ copies
//! of one 8-word prefix is already its own finding.
//! The scan advances past each emitted run. O(WALK_CAP * tokens) bounds
//! its work. Memory stays proportional to the token count.

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
    /// by the caller between bundle files. Constant within one document.
    seg: u32,
    /// Bundle file index. 0 for the single-document scan.
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
/// apostrophe, typographic apostrophe folded, using the `first_token` charset
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
    // Fixed-key SipHash repeats across runs and processes. The scan
    // resolves collisions through exact token comparison.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for d in 0..k {
        tokens.word(i + d).hash(&mut h);
    }
    h.finish()
}

/// A maximal run after exact token comparison. Holds the token indices of
/// the earlier copy, the later copy, and their shared length in words.
pub(crate) struct Run {
    pub earlier: usize,
    pub later: usize,
    pub len: usize,
}

/// The shared shingle scan. With `cross_file_only`, the scan skips
/// same-file revisits because each file's SD-Q005 pass handles them.
/// Segment boundaries separate copies from different files, which
/// cannot overlap. In this mode segment discipline replaces the
/// same-file disjointness guard.
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
    // Each hash points to its latest anchor. Earlier anchors form an
    // intrusive chain through one preallocated slot per token. Every
    // processed anchor joins the chain, so an early prefix-sharing copy
    // that diverges below the floor leaves later repeats available.
    // Walk at most WALK_CAP entries, most recent first, and keep the
    // longest eligible run after exact token comparison. More same-prefix
    // occurrences can exhaust the walk before the matching copy.
    // Sub-floor candidates use fewer than floor comparisons each way.
    // An emitted run advances the scan past its end, bounding total work by
    // O(WALK_CAP * tokens), near-linear and never O(N^2).
    // Each token index sits in at most one chain. Every processed anchor
    // joins whether exact token comparison succeeds or fails.
    // Both occ2-vs-occ3 and occ1-vs-occ3 across a
    // decoy can match. More than WALK_CAP same-prefix occurrences between
    // a genuine pair can mask it. That shape is unrealistic for an attacker,
    // because 32+ copies of one 8-word prefix are already the loudest thing in
    // the document.
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
                // Bundle mode pairs copies from different files. The
                // single-document scan requires disjoint copies, because
                // overlapping anchors can describe one repeated stem.
                // Exact shingle comparison rejects hash collisions.
                // Extend each eligible run forward and backward before
                // ranking by total length. Forward length alone can favor
                // a shorter run whose anchor sits nearer its beginning.
                // Length ties keep the last candidate walked. The chain
                // decreases in position, so ties choose the earliest copy.
                // ai-slop keeps the most recent copy on a length tie.
                // Later copies here share the earliest anchor for bundle grouping.
                // best holds (earlier start, later start, total len).
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
                        // The same guards apply. Same-file copies stay
                        // disjoint (the earlier copy's end `es + len` is
                        // pinned while the later start `s` moves left, so
                        // the gap must stay positive) and neither side
                        // crosses a file segment. The run length bounds each
                        // candidate's backward work, so the pass keeps its
                        // O(WALK_CAP * tokens) bound.
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
                        // Advance past the repeated run. Sub-runs of an
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
