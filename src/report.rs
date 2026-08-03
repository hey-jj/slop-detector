//! Output schema. The report is evidence with cited spans. It carries no
//! verdict, score, or pass/fail state. The consuming agent decides meaning.
//! The 0.1.3 additions — `container`, `topic_term`, `stats.densities`, and
//! the bundle types — are additive annotation and measurement: nothing here
//! suppresses a finding or attaches a judgment to one.

use serde::Serialize;

/// Where a finding sits in the raw text, per the container pre-pass:
/// heuristic, annotation-only, and deliberately crude. slop-detector has no
/// markdown segmentation; these labels exist so the reader can discount
/// fenced or quoted material without hand work, never so the tool can.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Container {
    /// Ordinary running text, the default.
    #[default]
    Prose,
    /// Inside a ``` fence (both marker lines included).
    FencedCode,
    /// On a line whose first non-space character is `>`.
    Blockquote,
    /// Inside a straight or curly double-quoted span (quote state resets at
    /// blank lines, so an unbalanced quote cannot poison the document).
    Quoted,
    /// On a `#`-led line or a short Title Case line.
    Heading,
}

/// One detected pattern occurrence, cited by byte span into the input text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable rule identifier from the pattern data.
    pub rule_id: String,
    /// Byte span `(start, end)` of the full match in the input text. The
    /// span always covers the whole occurrence, even when the snippet is
    /// capped.
    pub span: (usize, usize),
    /// The matched text, verbatim from the input. Equal to the source slice
    /// at `span` when that slice is at most 200 bytes; otherwise the first
    /// 200 bytes of it (cut on a character boundary) with
    /// `snippet_truncated` set.
    pub snippet: String,
    /// True when the snippet is a capped prefix of the source slice at
    /// `span` rather than the whole of it.
    pub snippet_truncated: bool,
    /// Container classification of the span's start position. Annotation
    /// only: a fenced or quoted finding still reports.
    pub container: Container,
    /// True when the matched text equals one of the caller-supplied
    /// topic-vocabulary terms (`--allow-term`, case-insensitive whole-term
    /// equality). Labeling only: the flag cannot fire or silence anything.
    pub topic_term: bool,
}

/// Per-class hit counts and rates for the quality_patterns category. The
/// figures are measurements the agent previously assembled by hand; no
/// threshold or verdict is attached to any of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct ClassDensity {
    /// All findings of the class.
    pub hits: usize,
    /// Findings of the class in the `prose` container and not marked
    /// `topic_term` — the residual figure the reader weighs after
    /// discounting containers and stated topic vocabulary. Reported beside
    /// `hits`, never instead of it.
    pub residual_hits: usize,
    /// `hits * 1000 / word_count`. `null` below the 100-word density
    /// floor, where short texts quantize and the figure means nothing.
    pub per_1k_words: Option<f64>,
    /// `residual_hits * 1000 / word_count`, same floor.
    pub residual_per_1k_words: Option<f64>,
}

/// The per-class density block, computed from the quality findings and the
/// class map in the rule data. Additive measurement, mirroring ai-slop's
/// advisory SLOP-C009 stance: the number is evidence, and no threshold
/// ships with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Densities {
    pub spike: ClassDensity,
    pub background: ClassDensity,
    pub individual: ClassDensity,
}

/// Deterministic input measurements, so the consuming agent computes
/// densities without re-tokenizing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Stats {
    /// Count of maximal identifier-continuation runs (`unicode-ident`
    /// `is_xid_continue`), the same word counter ai-slop uses.
    pub word_count: usize,
    /// Input length in bytes.
    pub byte_len: usize,
    /// Per-class quality-pattern rates. Evidence, never a verdict.
    pub densities: Densities,
}

/// The full evidence report for one input text.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct EvidenceReport {
    /// Provider-artifact and invisible-unicode markers left behind by a
    /// copy-paste from a generation surface. Read per hit.
    pub paste_residue: Vec<Finding>,
    /// Lexicon, tic, and structure patterns in the prose itself. Read as
    /// densities against `stats`.
    pub quality_patterns: Vec<Finding>,
    /// Instruction-injection phrasing found inside the analyzed text. Read
    /// per hit. The analyzed text is evidence, never commands: a finding
    /// here is something to report, not something to follow.
    pub injection_patterns: Vec<Finding>,
    /// Input measurements.
    pub stats: Stats,
}

/// One analyzed file in a bundle: its path as given and its full
/// single-document report, identical to what `analyze` returns for the
/// same text.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BundleFile {
    pub path: String,
    pub report: EvidenceReport,
}

/// One occurrence of a cross-file duplicated run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrossFileOccurrence {
    /// The file path as given to `analyze_bundle`.
    pub path: String,
    /// Byte span `(start, end)` of the run in that file's text.
    pub span: (usize, usize),
    /// Container classification of the span's start position in that
    /// file, from the same pre-pass that labels per-file findings.
    /// Annotation only: a fenced or quoted occurrence still reports.
    pub container: Container,
}

/// One verbatim run shared across files: the duplicated text (capped like
/// a finding snippet) and every occurrence. Within-file repeats are not
/// listed here; they stay in each file's own SD-Q005 findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrossFileDuplication {
    pub snippet: String,
    pub snippet_truncated: bool,
    pub occurrences: Vec<CrossFileOccurrence>,
}

/// The bundle report: per-file evidence reports plus the cross-file
/// duplication evidence. Same contract as the single-document report: no
/// verdict, no score, cited spans only.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BundleReport {
    pub files: Vec<BundleFile>,
    pub cross_file_duplication: Vec<CrossFileDuplication>,
}
