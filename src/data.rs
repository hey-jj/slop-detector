//! Inbound rule-table loading.
//!
//! The loaded rule table is `data/inbound/inbound.toml`, embedded at build
//! time together with every lexicon it names. This module parses the table
//! into typed rules. Every pattern is data; no pattern is hard-coded here or
//! in the engine.

use serde::Deserialize;

pub const INBOUND_TOML: &str = include_str!("../data/inbound/inbound.toml");

/// Embedded lexicon files, keyed by their package-relative path as written
/// in `inbound.toml`. Rules carried unchanged from the vendored ai-slop data
/// reference `words/`; rules carried with edits reference `inbound/`.
const LEXICONS: &[(&str, &str)] = &[
    (
        "words/provider-attribution.txt",
        include_str!("../data/words/provider-attribution.txt"),
    ),
    (
        "words/tracking-params.txt",
        include_str!("../data/words/tracking-params.txt"),
    ),
    (
        "words/stock-openers.txt",
        include_str!("../data/words/stock-openers.txt"),
    ),
    (
        "words/era-overuse.txt",
        include_str!("../data/words/era-overuse.txt"),
    ),
    (
        "words/inflated-diction.txt",
        include_str!("../data/words/inflated-diction.txt"),
    ),
    (
        "words/intensifiers.txt",
        include_str!("../data/words/intensifiers.txt"),
    ),
    (
        "words/importance-adjectives.txt",
        include_str!("../data/words/importance-adjectives.txt"),
    ),
    (
        "words/hype-adjectives.txt",
        include_str!("../data/words/hype-adjectives.txt"),
    ),
    (
        "words/magnitude-claims.txt",
        include_str!("../data/words/magnitude-claims.txt"),
    ),
    (
        "words/audience-runway.txt",
        include_str!("../data/words/audience-runway.txt"),
    ),
    (
        "words/reassurance.txt",
        include_str!("../data/words/reassurance.txt"),
    ),
    (
        "words/significance-inflation.txt",
        include_str!("../data/words/significance-inflation.txt"),
    ),
    (
        "words/copula-avoidance.txt",
        include_str!("../data/words/copula-avoidance.txt"),
    ),
    (
        "words/vague-attribution.txt",
        include_str!("../data/words/vague-attribution.txt"),
    ),
    (
        "words/cutoff-disclaimers.txt",
        include_str!("../data/words/cutoff-disclaimers.txt"),
    ),
    (
        "words/assistant-voice.txt",
        include_str!("../data/words/assistant-voice.txt"),
    ),
    (
        "words/pleasantries.txt",
        include_str!("../data/words/pleasantries.txt"),
    ),
    (
        "inbound/provider-artifacts.txt",
        include_str!("../data/inbound/provider-artifacts.txt"),
    ),
    (
        "inbound/injection.txt",
        include_str!("../data/inbound/injection.txt"),
    ),
    (
        "inbound/spike.txt",
        include_str!("../data/inbound/spike.txt"),
    ),
    (
        "inbound/background-register.txt",
        include_str!("../data/inbound/background-register.txt"),
    ),
    (
        "inbound/transition-trio.txt",
        include_str!("../data/inbound/transition-trio.txt"),
    ),
    (
        "inbound/filler-meta.txt",
        include_str!("../data/inbound/filler-meta.txt"),
    ),
    (
        "inbound/participial-stoplist.txt",
        include_str!("../data/inbound/participial-stoplist.txt"),
    ),
    (
        "inbound/contrastive-stoplist.txt",
        include_str!("../data/inbound/contrastive-stoplist.txt"),
    ),
    (
        "inbound/provenance-markers.txt",
        include_str!("../data/inbound/provenance-markers.txt"),
    ),
    (
        "words/agent-loop.txt",
        include_str!("../data/words/agent-loop.txt"),
    ),
    (
        "inbound/tool-nouns.txt",
        include_str!("../data/inbound/tool-nouns.txt"),
    ),
];

/// Report routing for a rule's findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    PasteResidue,
    QualityPatterns,
    Injection,
}

/// How a rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    WordSet,
    Regex,
    Codepoint,
    PositionalSpace,
    ParticipialOpener,
    ContrastiveTail,
    SelfDuplication,
    CapabilityDenial,
    RationaleLeak,
}

/// Interpretive class annotation for quality rules. Not emitted
/// in the report: the output carries no tiers, the class map travels to the
/// skill through this data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// The measured excess-vocabulary set. Full density weight.
    Spike,
    /// Pre-LLM register staples. Counted, low weight.
    Background,
    /// Read per hit like residue.
    Individual,
}

/// Match-position constraint for word-set rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    Anywhere,
    BlockStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Case {
    Sensitive,
    Insensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Boundary {
    None,
    Word,
}

/// One loaded rule, validated. `terms` holds the parsed lexicon for word-set
/// rules; `ranges` holds inclusive codepoint ranges for codepoint rules;
/// `codepoints` and `min_count` serve the positional-space mechanism.
#[derive(Debug)]
pub struct Rule {
    pub id: String,
    pub category: Category,
    pub mechanism: Mechanism,
    pub case: Case,
    pub boundary: Boundary,
    pub terms: Vec<String>,
    pub patterns: Vec<String>,
    pub ranges: Vec<(u32, u32)>,
    pub codepoints: Vec<u32>,
    pub min_count: usize,
    /// Codepoint-rule exemption: a U+FEFF at byte offset 0 is an editor
    /// byte-order mark, not residue, and never fires.
    pub exempt_leading_bom: bool,
    /// Codepoint-rule exemption: U+FE0E/U+FE0F immediately after a visible
    /// base character is an ordinary presentation selector and never fires.
    pub exempt_presentation_selector: bool,
    /// Codepoint-rule exemption: U+200C/U+200D between two joining-script
    /// characters or two pictographic characters is orthography, not
    /// residue, and never fires.
    pub exempt_joining_zwj: bool,
    /// Quality-rule class. Required for `quality_patterns`
    /// rules, absent everywhere else.
    pub class: Option<Class>,
    /// Match-position constraint for word-set rules.
    pub position: Position,
    /// Exemption phrases: a word-set hit fully contained in one of these
    /// phrases (checked case-insensitively in a nearby window) does not
    /// fire. Flattened from the per-term tables in the data.
    pub exemptions: Vec<String>,
    /// Deny-list, lowercased: the participial-opener stop-list, or the
    /// contrastive-tail imperative-opener list.
    pub stoplist: Vec<String>,
    /// Participial-opener maximum clause length in bytes between the
    /// opener word and the comma.
    pub max_clause: usize,
    /// Contrastive-tail noun-phrase byte cap.
    pub max_np: usize,
    /// Contrastive-tail clause walk-back window in bytes.
    pub clause_window: usize,
    /// Contrastive-tail second-person cue tokens, lowercased.
    pub second_person: Vec<String>,
    /// Self-duplication shingle order in words.
    pub shingle_words: usize,
    /// Self-duplication minimum verified run length in words.
    pub min_run_words: usize,
    /// Self-duplication per-document emission cap, longest runs first.
    pub max_reports: usize,
    /// Closed positive subjects, lowercased (capability-denial).
    pub subjects: Vec<String>,
    /// Determiners that open a positive noun-phrase subject, lowercased.
    /// Crossed with `tool_nouns` at compile time (capability-denial).
    pub determiners: Vec<String>,
    /// Closed negative subjects that carry their own negation, lowercased
    pub negative_subjects: Vec<String>,
    /// Determiners that open a negative noun-phrase subject, lowercased.
    /// Crossed with `tool_nouns` at compile time (capability-denial).
    pub negative_determiners: Vec<String>,
    /// The shared tool-noun set, lowercased. One file defines it and both
    /// clause-shape rules name that file.
    pub tool_nouns: Vec<String>,
    /// Coordinators skipped at a clause head before any test
    pub coordinators: Vec<String>,
    /// Negation phrases that can open a command, lowercased
    pub imperative_negations: Vec<String>,
    /// Negation phrases that only ever carry a finite verb, so a clause they
    /// head is never a command (capability-denial).
    pub finite_negations: Vec<String>,
    /// Subject-scope window in tokens: the positive subject's distance to
    /// its negation, and the negative subject's distance to its capability
    /// verb (capability-denial).
    pub negation_window: usize,
    /// Token distance after a negation inside which the capability verb must
    /// appear (capability-denial).
    pub verb_window: usize,
    /// Closed capability-verb set, split by form so the subjectless spelling
    /// can require an inflected verb (capability-denial).
    pub capability_verbs_base: Vec<String>,
    pub capability_verbs_s: Vec<String>,
    pub capability_verbs_ing: Vec<String>,
    /// Evidential hedge phrases (capability-denial). A `*` stands for any
    /// one word token.
    pub hedges: Vec<String>,
    /// Rationale-leak marker phrases, both families flattened.
    pub markers: Vec<String>,
}

#[derive(Deserialize)]
struct TableFile {
    #[serde(rename = "rule")]
    rules: Vec<RuleSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSpec {
    id: String,
    #[allow(dead_code)]
    name: String,
    category: Category,
    mechanism: Mechanism,
    lexicon: Option<String>,
    case: Option<Case>,
    boundary: Option<Boundary>,
    patterns: Option<Vec<String>>,
    ranges: Option<Vec<String>>,
    codepoints: Option<Vec<String>>,
    min_count: Option<usize>,
    exempt_leading_bom: Option<bool>,
    exempt_presentation_selector: Option<bool>,
    exempt_joining_zwj: Option<bool>,
    class: Option<Class>,
    /// Informational weight note carried to the skill (e.g. the SLOP-I001
    /// lowest-weight ruling). Not evaluated by the engine.
    #[allow(dead_code)]
    weight: Option<String>,
    position: Option<Position>,
    exemptions: Option<std::collections::BTreeMap<String, Vec<String>>>,
    stoplist: Option<String>,
    max_clause: Option<usize>,
    max_np: Option<usize>,
    clause_window: Option<usize>,
    second_person: Option<Vec<String>>,
    shingle_words: Option<usize>,
    min_run_words: Option<usize>,
    max_reports: Option<usize>,
    subjects: Option<Vec<String>>,
    determiners: Option<Vec<String>>,
    negative_subjects: Option<Vec<String>>,
    negative_determiners: Option<Vec<String>>,
    coordinators: Option<Vec<String>>,
    tool_nouns_lexicon: Option<String>,
    imperative_negations: Option<Vec<String>>,
    finite_negations: Option<Vec<String>>,
    negation_window: Option<usize>,
    verb_window: Option<usize>,
    capability_verbs_base: Option<Vec<String>>,
    capability_verbs_s: Option<Vec<String>>,
    capability_verbs_ing: Option<Vec<String>>,
    hedges: Option<Vec<String>>,
    markers: Option<Vec<String>>,
    #[allow(dead_code)]
    guard: String,
}

/// Lowercase a term list, rejecting an empty list under a mechanism that
/// requires it.
fn word_list(id: &str, field: &str, list: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let terms: Vec<String> = list
        .unwrap_or_default()
        .iter()
        .map(|t| t.to_lowercase())
        .collect();
    if terms.is_empty() {
        return Err(format!(
            "rule {id}: {field} is required and must be non-empty"
        ));
    }
    Ok(terms)
}

fn lexicon(path: &str) -> Result<Vec<String>, String> {
    let raw = LEXICONS
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, body)| *body)
        .ok_or_else(|| format!("lexicon {path} is not embedded"))?;
    let terms: Vec<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    if terms.is_empty() {
        return Err(format!("lexicon {path} has no terms"));
    }
    Ok(terms)
}

/// An exemption phrase must be non-empty and lead with an ASCII byte, so
/// the scan's one-byte advance past a match always lands on a character
/// boundary.
fn exemption_phrase_ok(p: &str) -> bool {
    p.as_bytes().first().is_some_and(|b| b.is_ascii())
}

fn parse_codepoint(s: &str) -> Result<u32, String> {
    let v = u32::from_str_radix(s, 16).map_err(|e| format!("codepoint {s}: {e}"))?;
    if char::from_u32(v).is_none() {
        return Err(format!("codepoint {s} is not a scalar value"));
    }
    Ok(v)
}

fn parse_range(s: &str) -> Result<(u32, u32), String> {
    let (lo, hi) = match s.split_once('-') {
        Some((lo, hi)) => (parse_codepoint(lo)?, parse_codepoint(hi)?),
        None => {
            let v = parse_codepoint(s)?;
            (v, v)
        }
    };
    if lo > hi {
        return Err(format!("range {s} is inverted"));
    }
    Ok((lo, hi))
}

/// Parse and validate the embedded rule table.
pub fn load() -> Result<Vec<Rule>, String> {
    let table: TableFile =
        toml::from_str(INBOUND_TOML).map_err(|e| format!("inbound.toml: {e}"))?;
    let mut rules = Vec::with_capacity(table.rules.len());
    let mut seen = std::collections::HashSet::new();
    for spec in table.rules {
        if !seen.insert(spec.id.clone()) {
            return Err(format!("duplicate rule id {}", spec.id));
        }
        let id = spec.id;
        let terms = match (spec.mechanism, &spec.lexicon) {
            (Mechanism::WordSet, Some(path)) => lexicon(path)?,
            (Mechanism::WordSet, None) => {
                return Err(format!("rule {id} is word-set but names no lexicon"));
            }
            (_, Some(_)) => {
                return Err(format!("rule {id} names a lexicon but is not word-set"));
            }
            (_, None) => Vec::new(),
        };
        if (spec.category == Category::QualityPatterns) != spec.class.is_some() {
            return Err(format!(
                "rule {id}: quality_patterns rules carry a class, others must not"
            ));
        }
        let stoplist = match (spec.mechanism, &spec.stoplist) {
            (Mechanism::ParticipialOpener | Mechanism::ContrastiveTail, Some(path)) => {
                lexicon(path)?
                    .into_iter()
                    .map(|t| t.to_ascii_lowercase())
                    .collect()
            }
            (Mechanism::ParticipialOpener, None) => {
                return Err(format!(
                    "rule {id} is participial-opener but names no stoplist"
                ));
            }
            (Mechanism::ContrastiveTail, None) => {
                return Err(format!(
                    "rule {id} is contrastive-tail but names no stoplist"
                ));
            }
            (_, Some(_)) => {
                return Err(format!(
                    "rule {id} names a stoplist but its mechanism takes none"
                ));
            }
            (_, None) => Vec::new(),
        };
        if spec.max_clause.is_some() && spec.mechanism != Mechanism::ParticipialOpener {
            return Err(format!("rule {id}: max_clause needs participial-opener"));
        }
        if (spec.max_np.is_some() || spec.clause_window.is_some() || spec.second_person.is_some())
            && spec.mechanism != Mechanism::ContrastiveTail
        {
            return Err(format!(
                "rule {id}: max_np, clause_window, and second_person need contrastive-tail"
            ));
        }
        let dup_params = (spec.shingle_words, spec.min_run_words, spec.max_reports);
        let (shingle_words, min_run_words, max_reports) = match (spec.mechanism, dup_params) {
            (Mechanism::SelfDuplication, (Some(k), Some(floor), Some(cap))) => {
                if k == 0 || floor < k {
                    return Err(format!(
                        "rule {id}: shingle_words must be at least 1 and \
                         min_run_words at least shingle_words"
                    ));
                }
                (k, floor, cap)
            }
            (Mechanism::SelfDuplication, _) => {
                return Err(format!(
                    "rule {id}: self-duplication requires shingle_words, \
                     min_run_words, and max_reports"
                ));
            }
            (_, (None, None, None)) => (0, 0, 0),
            _ => {
                return Err(format!(
                    "rule {id}: shingle_words, min_run_words, and max_reports \
                     need self-duplication"
                ));
            }
        };
        if spec.exemptions.is_some() && spec.mechanism != Mechanism::WordSet {
            return Err(format!("rule {id}: exemptions need word-set"));
        }
        // The two clause-shape mechanisms. They share the tool-noun set and
        // nothing else: capability-denial builds subjects from it, and
        // rationale-leak anchors on it wherever it appears in the sentence.
        // A parameter belonging to the other mechanism is a data error.
        let denial = spec.mechanism == Mechanism::CapabilityDenial;
        let rationale = spec.mechanism == Mechanism::RationaleLeak;
        if spec.tool_nouns_lexicon.is_some() && !(denial || rationale) {
            return Err(format!(
                "rule {id}: tool_nouns_lexicon needs capability-denial or rationale-leak"
            ));
        }
        if (spec.subjects.is_some()
            || spec.determiners.is_some()
            || spec.negative_subjects.is_some()
            || spec.negative_determiners.is_some()
            || spec.coordinators.is_some()
            || spec.imperative_negations.is_some()
            || spec.finite_negations.is_some()
            || spec.negation_window.is_some()
            || spec.verb_window.is_some()
            || spec.capability_verbs_base.is_some()
            || spec.capability_verbs_s.is_some()
            || spec.capability_verbs_ing.is_some()
            || spec.hedges.is_some())
            && !denial
        {
            return Err(format!(
                "rule {id}: the subject, negation, and capability-verb \
                 parameters need capability-denial"
            ));
        }
        if spec.markers.is_some() && !rationale {
            return Err(format!("rule {id}: markers need rationale-leak"));
        }
        let tool_nouns = match (denial || rationale, &spec.tool_nouns_lexicon) {
            (true, Some(path)) => lexicon(path)?
                .into_iter()
                .map(|t| t.to_lowercase())
                .collect(),
            (true, None) => {
                return Err(format!("rule {id} names no tool_nouns_lexicon"));
            }
            (false, _) => Vec::new(),
        };
        // Every capability-denial parameter is required for that mechanism
        // and absent everywhere else, which the check above already enforced.
        let req = |field: &str, list: Option<Vec<String>>| -> Result<Vec<String>, String> {
            if denial {
                word_list(&id, field, list)
            } else {
                Ok(Vec::new())
            }
        };
        let subjects = req("subjects", spec.subjects)?;
        let determiners = req("determiners", spec.determiners)?;
        let negative_subjects = req("negative_subjects", spec.negative_subjects)?;
        let negative_determiners = req("negative_determiners", spec.negative_determiners)?;
        let coordinators = req("coordinators", spec.coordinators)?;
        let imperative_negations = req("imperative_negations", spec.imperative_negations)?;
        let finite_negations = req("finite_negations", spec.finite_negations)?;
        let capability_verbs_base = req("capability_verbs_base", spec.capability_verbs_base)?;
        let capability_verbs_s = req("capability_verbs_s", spec.capability_verbs_s)?;
        let capability_verbs_ing = req("capability_verbs_ing", spec.capability_verbs_ing)?;
        let hedges = req("hedges", spec.hedges)?;
        let window = |field: &str, given: Option<usize>| match (denial, given) {
            (true, Some(0)) | (true, None) => Err(format!(
                "rule {id}: capability-denial requires a {field} of at least 1"
            )),
            (true, Some(w)) => Ok(w),
            (false, _) => Ok(0),
        };
        let negation_window = window("negation_window", spec.negation_window)?;
        let verb_window = window("verb_window", spec.verb_window)?;
        let markers = if rationale {
            word_list(&id, "markers", spec.markers)?
        } else {
            Vec::new()
        };
        let exemptions: Vec<String> = spec
            .exemptions
            .unwrap_or_default()
            .into_values()
            .flatten()
            .map(|p| p.to_lowercase())
            .collect();
        // The exemption-window scan advances one byte past each phrase
        // match, which is only boundary-safe when the phrase leads with an
        // ASCII byte. Every shipped phrase does; hold future data to it.
        if let Some(p) = exemptions.iter().find(|p| !exemption_phrase_ok(p)) {
            return Err(format!(
                "rule {id}: exemption phrase {p:?} must lead with an ASCII character"
            ));
        }
        let patterns = spec.patterns.unwrap_or_default();
        if !patterns.is_empty()
            && !matches!(
                spec.mechanism,
                Mechanism::WordSet | Mechanism::Regex | Mechanism::ContrastiveTail
            )
        {
            return Err(format!("rule {id} carries patterns but is not a text rule"));
        }
        if spec.mechanism == Mechanism::Regex && patterns.is_empty() {
            return Err(format!("rule {id} is regex but has no patterns"));
        }
        let ranges = spec
            .ranges
            .unwrap_or_default()
            .iter()
            .map(|s| parse_range(s))
            .collect::<Result<Vec<_>, _>>()?;
        if (spec.mechanism == Mechanism::Codepoint) != !ranges.is_empty() {
            return Err(format!("rule {id}: ranges and mechanism disagree"));
        }
        let codepoints = spec
            .codepoints
            .unwrap_or_default()
            .iter()
            .map(|s| parse_codepoint(s))
            .collect::<Result<Vec<_>, _>>()?;
        if (spec.mechanism == Mechanism::PositionalSpace) != !codepoints.is_empty() {
            return Err(format!("rule {id}: codepoints and mechanism disagree"));
        }
        if spec.min_count.is_some() && spec.mechanism != Mechanism::PositionalSpace {
            return Err(format!("rule {id}: min_count needs positional-space"));
        }
        if (spec.exempt_leading_bom.is_some()
            || spec.exempt_presentation_selector.is_some()
            || spec.exempt_joining_zwj.is_some())
            && spec.mechanism != Mechanism::Codepoint
        {
            return Err(format!("rule {id}: codepoint exemptions need codepoint"));
        }
        rules.push(Rule {
            id,
            category: spec.category,
            mechanism: spec.mechanism,
            case: spec.case.unwrap_or(Case::Sensitive),
            boundary: spec.boundary.unwrap_or(Boundary::None),
            terms,
            patterns,
            ranges,
            codepoints,
            min_count: spec.min_count.unwrap_or(1),
            exempt_leading_bom: spec.exempt_leading_bom.unwrap_or(false),
            exempt_presentation_selector: spec.exempt_presentation_selector.unwrap_or(false),
            exempt_joining_zwj: spec.exempt_joining_zwj.unwrap_or(false),
            class: spec.class,
            position: spec.position.unwrap_or(Position::Anywhere),
            exemptions,
            stoplist,
            max_clause: spec.max_clause.unwrap_or(100),
            max_np: spec.max_np.unwrap_or(60),
            clause_window: spec.clause_window.unwrap_or(240),
            second_person: spec
                .second_person
                .unwrap_or_default()
                .iter()
                .map(|t| t.to_lowercase())
                .collect(),
            shingle_words,
            min_run_words,
            max_reports,
            subjects,
            determiners,
            negative_subjects,
            negative_determiners,
            coordinators,
            tool_nouns,
            imperative_negations,
            finite_negations,
            negation_window,
            verb_window,
            capability_verbs_base,
            capability_verbs_s,
            capability_verbs_ing,
            hedges,
            markers,
        });
    }
    if rules.is_empty() {
        return Err("inbound.toml has no rules".to_string());
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_loads_and_holds_the_full_rule_set() {
        let rules = load().expect("embedded table loads");
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        // The residue and injection rules.
        let residue = [
            "SLOP-P001",
            "SLOP-P002",
            "SLOP-P004",
            "SD-R001",
            "SD-R002",
            "SD-R003",
            "SD-R004",
            "SLOP-J001",
            "SD-J001",
        ];
        // The quality rules.
        let quality = [
            "SLOP-A001",
            "SLOP-O003",
            "SD-Q001",
            "SLOP-A003",
            "SLOP-A004",
            "SLOP-A005",
            "SLOP-I001",
            "SLOP-I002",
            "SLOP-I003",
            "SLOP-I004",
            "SLOP-T001",
            "SLOP-T002",
            "SLOP-T003",
            "SLOP-C001",
            "SLOP-C002",
            "SLOP-C003",
            "SLOP-C004",
            "SLOP-C005",
            "SLOP-C006",
            "SLOP-C008",
            "SLOP-Q001",
            "SLOP-R001",
            "SLOP-O001",
            "SLOP-O002",
            "SLOP-O004",
            "SD-Q002",
            "SD-Q004",
            "SLOP-V001",
            "SLOP-V002",
            "SLOP-V004",
            "SLOP-S003",
            "SD-Q003",
            "SD-Q005",
            "SD-Q006",
            "SD-Q007",
            "SD-Q008",
        ];
        let expected: Vec<&str> = residue.iter().chain(quality.iter()).copied().collect();
        assert_eq!(ids, expected);
        // Omitted or not loaded by design; see the rule guards: SD-R005,
        // signature-lines, the mechanical house-style family, dropped
        // empty-qualifiers (I005), the outbound-purpose families
        // (first-person, verification-claims, impact-framing, scrub,
        // assistant-offers), and the clarity-meta set (R002).
        for absent in [
            "SD-R005",
            "SLOP-S001",
            "SLOP-I005",
            "SLOP-F001",
            "SLOP-F002",
            "SLOP-F003",
            "SLOP-W001",
            "SLOP-V003",
            "SLOP-R002",
        ] {
            assert!(!ids.contains(&absent), "{absent} must not be loaded");
        }
        assert!(!ids.iter().any(|id| id.starts_with("SLOP-M")));
    }

    #[test]
    fn quality_classes_match_the_spec_re_tier() {
        let rules = load().unwrap();
        let class_of = |id: &str| {
            rules
                .iter()
                .find(|r| r.id == id)
                .unwrap_or_else(|| panic!("{id} missing"))
                .class
        };
        for id in ["SLOP-A001", "SLOP-O003"] {
            assert_eq!(class_of(id), Some(Class::Spike), "{id}");
        }
        for id in [
            "SD-Q001",
            "SLOP-A003",
            "SLOP-A004",
            "SLOP-A005",
            "SLOP-I001",
            "SLOP-I002",
            "SLOP-I003",
            "SLOP-I004",
            "SLOP-T001",
            "SLOP-T002",
            "SLOP-T003",
            "SLOP-C001",
            "SLOP-C002",
            "SLOP-C003",
            "SLOP-C004",
            "SLOP-C005",
            "SLOP-C006",
            "SLOP-C008",
            "SLOP-Q001",
            "SLOP-R001",
            "SLOP-O001",
            "SLOP-O002",
            "SLOP-O004",
            "SD-Q002",
            "SD-Q004",
        ] {
            assert_eq!(class_of(id), Some(Class::Background), "{id}");
        }
        for id in [
            "SLOP-V001",
            "SLOP-V002",
            "SLOP-V004",
            "SLOP-S003",
            "SD-Q003",
            "SD-Q005",
            "SD-Q006",
            "SD-Q007",
            "SD-Q008",
        ] {
            assert_eq!(class_of(id), Some(Class::Individual), "{id}");
        }
        // Residue and injection rules carry no class.
        for r in &rules {
            assert_eq!(
                r.class.is_some(),
                r.category == Category::QualityPatterns,
                "{}",
                r.id
            );
        }
    }

    #[test]
    fn spike_list_matches_the_settled_set() {
        let rules = load().unwrap();
        let a001 = rules.iter().find(|r| r.id == "SLOP-A001").unwrap();
        let mut terms = a001.terms.clone();
        terms.sort();
        assert_eq!(
            terms,
            [
                "commendable",
                "delve",
                "delved",
                "delves",
                "delving",
                "embark",
                "embarked",
                "embarking",
                "embarks",
                "intricacies",
                "intricate",
                "myriad",
                "plethora",
                "tapestry",
                "testament",
            ]
        );
        // meticulous stays in hype-adjectives (see the SLOP-I003 guard), and
        // the background register holds the demoted ornamental terms, not
        // the spike terms.
        assert!(!terms.iter().any(|t| t.starts_with("meticulous")));
        let i003 = rules.iter().find(|r| r.id == "SLOP-I003").unwrap();
        assert!(i003.terms.contains(&"meticulous".to_string()));
        let q001 = rules.iter().find(|r| r.id == "SD-Q001").unwrap();
        for kept in [
            "leverage", "robust", "seamless", "foster", "empower", "unlock", "elevate",
        ] {
            assert!(q001.terms.contains(&kept.to_string()), "{kept}");
        }
        for moved in [
            "delve",
            "tapestry",
            "testament",
            "myriad",
            "plethora",
            "embark",
        ] {
            assert!(!q001.terms.contains(&moved.to_string()), "{moved}");
        }
    }

    #[test]
    fn exemption_phrases_must_lead_with_ascii() {
        assert!(exemption_phrase_ok("cpu utilization"));
        assert!(exemption_phrase_ok("highly available"));
        assert!(!exemption_phrase_ok(""));
        assert!(!exemption_phrase_ok("\u{00E9}tude complete"));
        // Every shipped exemption passes the guard, so load succeeds.
        let rules = load().unwrap();
        for r in &rules {
            assert!(
                r.exemptions.iter().all(|p| exemption_phrase_ok(p)),
                "{}",
                r.id
            );
        }
    }

    #[test]
    fn c003_carries_only_the_anchored_forms() {
        let rules = load().unwrap();
        let c003 = rules.iter().find(|r| r.id == "SLOP-C003").unwrap();
        assert_eq!(c003.patterns.len(), 4);
        // Corpus calibration: the bare rather-than pattern is dropped.
        assert!(!c003.patterns.contains(&r"(?i)\brather than\b".to_string()));
        assert!(c003
            .patterns
            .contains(&r"(?i)\brather than (simply|merely|just)\b".to_string()));
    }

    #[test]
    fn r003_declares_the_joining_zwj_exemption() {
        let rules = load().unwrap();
        let r003 = rules.iter().find(|r| r.id == "SD-R003").unwrap();
        assert!(r003.exempt_joining_zwj);
    }

    #[test]
    fn transition_trio_and_filler_edits_hold() {
        let rules = load().unwrap();
        let t002 = rules.iter().find(|r| r.id == "SLOP-T002").unwrap();
        let mut terms = t002.terms.clone();
        terms.sort();
        assert_eq!(terms, ["additionally", "furthermore", "moreover"]);
        assert_eq!(t002.position, Position::BlockStart);
        let t001 = rules.iter().find(|r| r.id == "SLOP-T001").unwrap();
        assert!(!t001.terms.contains(&"overall".to_string()));
        assert!(t001.terms.contains(&"in conclusion".to_string()));
    }

    #[test]
    fn edited_artifact_lexicon_drops_turn_literals_and_widens_the_cdn_host() {
        let rules = load().unwrap();
        let p002 = rules.iter().find(|r| r.id == "SLOP-P002").unwrap();
        assert!(!p002.terms.iter().any(|t| t.starts_with("turn0")));
        assert!(!p002.terms.iter().any(|t| t.starts_with("turn1")));
        assert!(!p002.terms.iter().any(|t| t.starts_with("turn2")));
        assert!(p002.terms.contains(&"oaiusercontent.com".to_string()));
        assert!(!p002.terms.contains(&"files.oaiusercontent.com".to_string()));
    }

    #[test]
    fn r004_min_count_is_three() {
        let rules = load().unwrap();
        let r004 = rules.iter().find(|r| r.id == "SD-R004").unwrap();
        assert_eq!(r004.min_count, 3);
        // U+00A0 is deliberately absent: the HTML-nbsp-between-words case
        // fails the hard-evidence bar.
        assert_eq!(r004.codepoints, [0x202F, 0x2003, 0x2009]);
    }

    #[test]
    fn r002_is_restricted_to_the_citation_delimiters() {
        let rules = load().unwrap();
        let r002 = rules.iter().find(|r| r.id == "SD-R002").unwrap();
        assert_eq!(r002.ranges, [(0xE200, 0xE202)]);
    }

    #[test]
    fn r003_excludes_soft_hyphen_and_bidi_controls() {
        let rules = load().unwrap();
        let r003 = rules.iter().find(|r| r.id == "SD-R003").unwrap();
        let covered = |cp: u32| r003.ranges.iter().any(|&(lo, hi)| lo <= cp && cp <= hi);
        for cp in [
            0x00AD, 0x061C, 0x200E, 0x200F, 0x202C, 0x202E, 0x2066, 0x2069,
        ] {
            assert!(!covered(cp), "U+{cp:04X} must not be scanned");
        }
        for cp in [0x200B, 0x200C, 0x200D, 0x2060, 0xFEFF, 0xE0041] {
            assert!(covered(cp), "U+{cp:04X} must stay scanned");
        }
        assert!(r003.exempt_leading_bom);
        assert!(r003.exempt_presentation_selector);
    }

    #[test]
    fn j001_uses_the_trimmed_word_bounded_inbound_lexicon() {
        let rules = load().unwrap();
        let j001 = rules.iter().find(|r| r.id == "SLOP-J001").unwrap();
        assert_eq!(j001.boundary, Boundary::Word);
        assert!(!j001.terms.contains(&"you are now".to_string()));
        assert!(j001
            .terms
            .contains(&"ignore previous instructions".to_string()));
        assert!(j001.terms.contains(&"system prompt".to_string()));
    }

    #[test]
    fn q004_carries_the_c007_suppression_params() {
        let rules = load().unwrap();
        let q004 = rules.iter().find(|r| r.id == "SD-Q004").unwrap();
        assert_eq!(q004.mechanism, Mechanism::ContrastiveTail);
        assert_eq!(q004.class, Some(Class::Background));
        assert_eq!(q004.max_np, 60);
        assert_eq!(q004.clause_window, 240);
        assert_eq!(q004.second_person, ["you", "your", "you're", "yours"]);
        // The imperative-opener deny-list, same as SLOP-C007's param.
        for opener in ["do", "don't", "never", "use", "keep", "please"] {
            assert!(q004.stoplist.contains(&opener.to_string()), "{opener}");
        }
        assert_eq!(q004.stoplist.len(), 18);
        // The T2-T4 trigger regexes plus the and-not spelling ride the
        // shared regex pass.
        assert_eq!(q004.patterns.len(), 5);
        // The conjunction is `and` alone: the or-spelling fires on the honest
        // idiom "whether or not", and the engine takes no look-behind.
        assert!(q004.patterns.iter().any(|p| p.contains(r"\band\s{1,8}not")));
        assert!(!q004.patterns.iter().any(|p| p.contains("(and|or)")));
    }

    #[test]
    fn q007_and_q008_share_one_tool_noun_set() {
        let rules = load().unwrap();
        let q007 = rules.iter().find(|r| r.id == "SD-Q007").unwrap();
        assert_eq!(q007.mechanism, Mechanism::CapabilityDenial);
        assert_eq!(q007.negation_window, 4);
        assert_eq!(q007.verb_window, 3);
        assert!(q007.hedges.contains(&"no * is evidence".to_string()));
        assert!(q007.markers.is_empty());
        // The subject set: `that` collides with the relativizer and `they`
        // was never carried. The negative subjects take their own
        // determiners, one of which is several words.
        assert_eq!(q007.subjects, ["it", "this"]);
        assert_eq!(q007.determiners, ["the"]);
        assert!(q007
            .negative_determiners
            .contains(&"none of the".to_string()));
        // Capability verbs gate every spelling, split by form so the
        // subjectless spelling can demand an inflected verb. Function verbs
        // are absent by design, because denying one states a scope fact.
        let all_verbs: Vec<&String> = q007
            .capability_verbs_base
            .iter()
            .chain(&q007.capability_verbs_s)
            .chain(&q007.capability_verbs_ing)
            .collect();
        assert_eq!(q007.capability_verbs_base.len(), 17);
        assert_eq!(q007.capability_verbs_s.len(), 17);
        assert_eq!(q007.capability_verbs_ing.len(), 17);
        for verb in ["detect", "scores", "guaranteeing", "replace", "identifies"] {
            assert!(all_verbs.iter().any(|v| *v == verb), "{verb}");
        }
        for excluded in [
            "find", "fire", "fires", "catch", "block", "validate", "verify",
        ] {
            assert!(!all_verbs.iter().any(|v| *v == excluded), "{excluded}");
        }
        // The two negation classes: only the imperative-capable set can open
        // a command, and bare `not` is in neither.
        assert_eq!(q007.imperative_negations, ["do not", "don't", "never"]);
        assert!(q007.finite_negations.contains(&"does not".to_string()));
        assert!(!q007.finite_negations.contains(&"never".to_string()));
        // The coordinator list serves the imperative test and the subject
        // match alike, so `and it does not ...` reads like `it does not ...`.
        assert_eq!(q007.coordinators, ["and", "or", "but", "yet", "so", "nor"]);
        // The noun-object negations belong to family 2, not family 1.
        for dropped in ["makes no", "carries no", "has no", "not", "no"] {
            let held = q007
                .imperative_negations
                .iter()
                .chain(&q007.finite_negations)
                .any(|n| n == dropped);
            assert!(!held, "{dropped}");
        }
        // One tool-noun set, defined in one file and named by both rules.
        let q008 = rules.iter().find(|r| r.id == "SD-Q008").unwrap();
        assert_eq!(q008.mechanism, Mechanism::RationaleLeak);
        assert_eq!(q007.tool_nouns, q008.tool_nouns);
        assert_eq!(q007.tool_nouns, lexicon("inbound/tool-nouns.txt").unwrap());
        for noun in [
            "tool", "linter", "crate", "rule", "check", "gate", "detector", "guard", "report",
            "finding", "score", "output", "result", "test",
        ] {
            assert!(q007.tool_nouns.contains(&noun.to_string()), "{noun}");
            let plural = format!("{noun}s");
            assert!(q007.tool_nouns.contains(&plural), "{plural}");
        }
        assert!(q008.imperative_negations.is_empty());
        assert!(q008.finite_negations.is_empty());
        assert!(q008.markers.contains(&"by design".to_string()));
        // The rationale-leak anchor is the tool noun alone: no subject set
        // is loaded for it.
        assert!(q008.subjects.is_empty());
        assert!(q008.determiners.is_empty());
        // No other rule carries the clause-shape parameters.
        for r in rules
            .iter()
            .filter(|r| r.id != "SD-Q007" && r.id != "SD-Q008")
        {
            assert!(r.subjects.is_empty(), "{}", r.id);
            assert!(r.tool_nouns.is_empty(), "{}", r.id);
        }
    }

    #[test]
    fn q005_carries_the_u001_duplication_params() {
        let rules = load().unwrap();
        let q005 = rules.iter().find(|r| r.id == "SD-Q005").unwrap();
        assert_eq!(q005.mechanism, Mechanism::SelfDuplication);
        assert_eq!(q005.class, Some(Class::Individual));
        assert_eq!(q005.shingle_words, 8);
        assert_eq!(q005.min_run_words, 10);
        assert_eq!(q005.max_reports, 20);
        assert!(q005.patterns.is_empty());
        // The params are exclusive to the mechanism: no other rule carries
        // them.
        for r in rules.iter().filter(|r| r.id != "SD-Q005") {
            assert_eq!(r.shingle_words, 0, "{}", r.id);
        }
    }

    #[test]
    fn v004_loads_the_vendored_agent_loop_lexicon_as_individual() {
        let rules = load().unwrap();
        let v004 = rules.iter().find(|r| r.id == "SLOP-V004").unwrap();
        assert_eq!(v004.class, Some(Class::Individual));
        assert_eq!(v004.boundary, Boundary::Word);
        for term in [
            "this turn",
            "as requested",
            "per your request",
            "point me at",
        ] {
            assert!(v004.terms.contains(&term.to_string()), "{term}");
        }
        // The construction patterns: Flagged-for is case-sensitive by
        // design (no (?i) prefix), the all-N-confirmed shape is not.
        assert_eq!(v004.patterns.len(), 2);
        assert!(v004.patterns[0].starts_with(r"\bFlagged for"));
        // The refreshed vendored assistant-offers lexicon no longer carries
        // the request-reference phrases (they moved to agent-loop.txt in
        // ai-slop 0.1.6); that lexicon stays unloaded inbound either way.
        let agent_loop = lexicon("words/agent-loop.txt").unwrap();
        assert_eq!(agent_loop.len(), 10);
        assert_eq!(v004.terms, agent_loop, "loaded unmodified from words/");
    }

    #[test]
    fn a005_and_c008_carry_the_ai_slop_regex_sets_as_background() {
        let rules = load().unwrap();
        let a005 = rules.iter().find(|r| r.id == "SLOP-A005").unwrap();
        assert_eq!(a005.mechanism, Mechanism::Regex);
        assert_eq!(a005.class, Some(Class::Background));
        assert_eq!(a005.patterns.len(), 8);
        let c008 = rules.iter().find(|r| r.id == "SLOP-C008").unwrap();
        assert_eq!(c008.mechanism, Mechanism::Regex);
        assert_eq!(c008.class, Some(Class::Background));
        assert_eq!(c008.patterns.len(), 4);
    }

    #[test]
    fn q003_carries_the_provenance_vocabulary_as_individual() {
        let rules = load().unwrap();
        let q003 = rules.iter().find(|r| r.id == "SD-Q003").unwrap();
        assert_eq!(q003.class, Some(Class::Individual));
        assert_eq!(q003.boundary, Boundary::Word);
        for term in [
            "provenance",
            "reimplemented",
            "reference implementation",
            "kept for api parity",
        ] {
            assert!(q003.terms.contains(&term.to_string()), "{term}");
        }
        assert_eq!(q003.patterns.len(), 4);
        // No scrub rule loads inbound, so the W001 de-dup exemption is
        // deliberately absent.
        assert!(q003.exemptions.is_empty());
    }
}
