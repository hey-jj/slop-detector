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
    /// Participial-opener stop-list, lowercased.
    pub stoplist: Vec<String>,
    /// Participial-opener maximum clause length in bytes between the
    /// opener word and the comma.
    pub max_clause: usize,
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
    #[allow(dead_code)]
    guard: String,
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
            (Mechanism::ParticipialOpener, Some(path)) => lexicon(path)?
                .into_iter()
                .map(|t| t.to_ascii_lowercase())
                .collect(),
            (Mechanism::ParticipialOpener, None) => {
                return Err(format!(
                    "rule {id} is participial-opener but names no stoplist"
                ));
            }
            (_, Some(_)) => {
                return Err(format!(
                    "rule {id} names a stoplist but is not participial-opener"
                ));
            }
            (_, None) => Vec::new(),
        };
        if spec.max_clause.is_some() && spec.mechanism != Mechanism::ParticipialOpener {
            return Err(format!("rule {id}: max_clause needs participial-opener"));
        }
        if spec.exemptions.is_some() && spec.mechanism != Mechanism::WordSet {
            return Err(format!("rule {id}: exemptions need word-set"));
        }
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
        if !patterns.is_empty() && !matches!(spec.mechanism, Mechanism::WordSet | Mechanism::Regex)
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
        ];
        // The quality rules.
        let quality = [
            "SLOP-A001",
            "SLOP-O003",
            "SD-Q001",
            "SLOP-A003",
            "SLOP-A004",
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
            "SLOP-Q001",
            "SLOP-R001",
            "SLOP-O001",
            "SLOP-O002",
            "SLOP-O004",
            "SD-Q002",
            "SLOP-V001",
            "SLOP-V002",
            "SLOP-S003",
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
            "SLOP-Q001",
            "SLOP-R001",
            "SLOP-O001",
            "SLOP-O002",
            "SLOP-O004",
            "SD-Q002",
        ] {
            assert_eq!(class_of(id), Some(Class::Background), "{id}");
        }
        for id in ["SLOP-V001", "SLOP-V002", "SLOP-S003"] {
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
}
