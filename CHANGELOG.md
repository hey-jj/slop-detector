# Changelog

## [0.1.9] - 2026-10-03

### Documentation

- Rewrite rustdoc, code comments, and the manually synced skill rule
  reference in the house style for shipped prose.

### Changed

- Reword rule guard text in the inbound data. Matching patterns
  are unchanged.

## [0.1.8] - 2026-09-29

### Added

- `SD-Q010` hedging-litotes, an individual-class quality rule. It reads a
  claim carried by the negation of its opposite, or a stock understatement
  standing in for the verdict. One closed set over two shapes. The fixed
  shapes have no honest reading in confident prose: the negated adjectives
  behind `not` (`not inconsiderable`, `not unimportant`), the negated
  difficulty forms (`no small feat`, `no mean feat`, `in no small part`, `was
  no simple task`), `not without its challenges`, `not exactly trivial`,
  `isn't exactly simple`, `far from trivial`, `hardly
  surprising`, `less than stellar`, `leaves something to be desired`, `it
  would not be wrong to say`, `it is safe to say`, `it is not hard to see`,
  `to say the least`, `to put it mildly`, `not rocket science`, `not a walk
  in the park`, `not for the faint of heart`, `not to be underestimated`,
  and a comma-led `not to mention`. The measurable shapes can carry a
  literal count, rate, rank, or evidence gap: `not terribly`, `not uncommon`
  and eleven other negated adjectives, `not unheard of`, `not infrequently`, `not a
  trivial undertaking`, `no small amount`, `not without merit`, `not
  entirely clear`, `not exactly ideal`, `not quite right`, `not
  particularly`, `far from ideal`, `less than ideal`, `it is not
  unreasonable to`, `not the best`, `could be better`, and `room for
  improvement`. Every member led by `not` also takes `isn't`, `wasn't`,
  `aren't`, and `weren't`. Every adjective is listed and no prefix wildcard exists, so
  `not impossible`, `not unlike`, `not incorrect`, `not necessarily`, `not
  yet`, a bare `far from`, and a bare `hardly` stay silent, and a
  clause-initial `No simple task` is a quantifier and stays silent. The span
  is the phrase itself, so a hit inside a contrast span reports beside the
  contrast rule and the two-sentence reframe still reports `SLOP-C002`. In
  received text a run of hits marks prose that hedges by habit, and the
  honest collisions are frequent, so every hit reads against the document's
  purpose. `it goes without saying` stays with `SLOP-T001`.
- The skill reads the new rule under the individual class.

### Changed

- The lazy DFA caches behind the regex rules grow from 4 MiB to 8 MiB. The
  twenty patterns of `SD-Q010` pushed the reverse automaton's minimum past
  the old bound, which failed the embedded data compile. The cache is an
  upper bound on memory the automaton may use, and nothing else changes.

## [0.1.7] - 2026-09-02

### Added

- `SD-Q009` decision-attribution, an individual-class quality rule. It reads
  a decision credited to a role noun in the third person. Two closed role
  sets. With an open role (owner, maintainer, author, principal, proxy,
  operator, orchestrator, human, lead, user, reviewer) it reads the
  possessive on a decision noun (`Owner's ruling, 2026-08-20`, `the
  maintainer's call`), a verdict verb (`the owner ruled`, `the proxy signed
  off`), the by-form (`requested by the user`), the hyphen compound
  (`owner-flagged`), `per the owner`, `at the user's request`, and `on the
  principal's instruction`. With a ledger role (owner, maintainer,
  principal, proxy, orchestrator, lead) it also reads the loose verbs (`the
  owner wants`, `the owner has ruled`), the loose possessives (`the owner's
  request`, `in the owner's stead`), and the bare compound (`owner
  decision`, `owner-proxy ruling`). Three shapes carry no role: a decision noun on an ISO date
  (`ruling (2026-08-20)`), a line-start label (`Ruling:`, `Decision:`), and
  the second person aimed at the signer (`per your ruling`, `as you directed`). A
  trailing ISO date joins the span, so a dated stamp reports once. In
  received text the shape is agent-loop residue: an agent drafting in a
  person's name records where the choice came from, and the person named
  never writes about themselves that way. Minutes, contracts, and consent
  flows carry the same nouns honestly, so every hit reads against the
  document's purpose. The loose verbs stay off the open roles, so `the user
  asked` and `the author said` stay silent, and the verb sets carry verdict
  verbs only, so `user-defined` and `set by the user` stay silent. The
  roleless verb-plus-date stamp stays with `SD-Q006`. A four-week sweep of
  session transcripts supplied the spellings.
- The skill reads the new rule beside the ledger stamp under the individual
  class.

## [0.1.6] - 2026-08-20

### Added

- SD-Q007 reads a fourth family-1 shape, the coordinated denial. `and never
  detect authorship` at the tail of a sentence is a command read alone, and a
  denial when an earlier segment of the same sentence names the thing, because
  that is the subject it continues. Four conditions, all required: the
  coordinator is `and`, the negation heads the segment past the coordinator
  skip, the verb it governs is a base form from the closed capability set, and
  an earlier segment carries a closed-set subject. `The rules read text and
  never detect authorship.` reports the segment `never detect authorship`.
  `but`, `so`, and a sentence boundary all keep the command reading.
- SD-Q007's open hedge forms take up to three words between `no` and the
  copula, so `no one single finding is evidence` reports where the two-word
  cap left it silent. The head noun is still the last of those words, so
  `no one single sample is evidence` stays about a sample and the adjacency
  arm stays unavailable. Four words is the recorded miss.
- A guard-prose gate in the test suite. It walks every rule's `guard` and
  `weight` text from the loaded table and fails on the mechanical classes: em
  dash, en dash, semicolon, contrast scaffolding, and house filler. It reads
  those two fields and nothing else, so a semicolon inside a regex character
  class never reaches it. A guard quoting a term the table declares is exempt,
  which is how the SLOP-A001 guard names `delve` and `tapestry` and stays
  clean. Punctuation sits outside the exemption's reach. The first run found
  three contrast-scaffolding sites, in the SLOP-V004, SD-Q005, and SD-Q007
  guards, and all three are rewritten.

### Changed

- SD-Q004 leaves participial adjuncts alone. The tail in `She listened, never
  judging anyone.` describes the manner of her listening, so it is an adjunct.
  The exemption is narrow: the negation and the `-ing` word have to be
  adjacent, so `not the beginning` and `not a building` still report, and
  `nothing`, `anything`, `something`, `everything`, and `during` are denied
  it.
- SLOP-V002 gains `fair hit`, the concession that grants the reader's point
  before carrying on. The sports and gaming sense is literal, and the entry is
  a substring so it reaches `unfair hit`. Both fire and the reader absorbs
  them. `fair point`, `fair enough`, `fair cop`, and `fair knock` are not
  carried.
- SLOP-V002 drops `good catch` and `great catch` as ordinary thread speech. A
  reviewer who writes them means them, and the hit still moved the
  per-1000-words figure the reader is told to trust. `Good catch, updated the
  doc.` is now silent, and the paired form still reports: `Good catch! You're
  absolutely right.` fires on the second phrase.
- SLOP-T002's marker and decoration skip sets are the fleet-wide ones, held
  as two named constants so a future harvest edits both together. The marker
  set gains `‣`, `⁃`, and `∙` beside `•`. The decoration set widens to the
  arrows, miscellaneous technical, geometric shapes, and the rest of the
  shared table. The geometric shapes are what the measurement turned on: the
  nested-list glyphs a paste carries led 3,474 lines of the fleet corpus, and
  that is the received text this tool reads. The set is carried whole because
  the walk stops at the first character it does not cover, so one uncovered
  glyph would defeat every covered glyph beside it and a pasted run mixes
  them. `‣`, `⁃`, and `∙` are in on cost asymmetry and set completeness,
  never on measured need, their combined corpus population being one. `·`
  stays out of both, since the middle dot separates words inside a line. The set is kept apart from the
  pictographic test SD-R003 reads, so a zero-width joiner between two
  geometric shapes is still residue.
- SLOP-T002's block-start test walks past everything a writer puts in front
  of the first word: whitespace, the unordered markers, an ordered marker
  (`1.`, `2)`), a blockquote `>`, a heading `#`, and a leading emoji run. A
  leading emoji or markdown opener used to defeat the test and silence the
  rule. The walk still has to reach a line start or a terminal, so
  `C# Moreover` stays silent. An ordered marker is a digit run opening its
  line, so `See item 3. Moreover` fires on its sentence-ending period and
  `See item 4) Moreover` does not. Periods go through the engine's terminal
  test, so `See e.g. moreover` no longer reports mid-sentence.
- The published crate carries `skills/**`, `tests/**`, and this changelog. The
  test suite is the executable specification of a policy product, and the
  README advertised a paired skill the crate did not ship.

### Documentation

- The skill and `references/rules.md` carry the coordinated denial, the wider
  hedge forms, and the participial exemption.

## [0.1.5] - 2026-08-20

### Added

- SD-Q007 proleptic-capability-denial: the pre-rebuttal stack in received
  text, read one clause at a time. A denial of a capability nobody claimed
  needs a closed-set subject and a capability verb from a closed list, so
  denying a function (`the check does not fire`) reads as the scope fact it
  is. Three spellings, one of them subjectless for the fragment in the middle
  of a stack. Two qualifying clauses in one block report, and so does a single
  one standing beside an affirmative self-description of the same thing.
  Commands are excluded one clause at a time. A denial reports its
  coordinator-cut segment and an evidential hedge reports its whole comma
  clause.
- SD-Q008 rationale-leak: design reasoning left in text meant for a reader,
  in two marker families, the bargain behind a choice and an instruction on
  how to take the text. Both need a tool noun anywhere in the same sentence,
  so `This poem should be read as an elegy` stays silent. One finding per
  marker.
- SD-Q004 gains the `and not` spelling before a preposition or article. The
  `or not` and `but not` spellings stay hand-read, because `whether or not the
  flag is present` is an honest open condition wearing the same letters.
- `inbound/tool-nouns.txt`, the shared closed set both clause-shape rules
  name, so the subject set and the anchor set cannot drift apart.

### Documentation

- The skill carries the denial stack as a named shape, states that the tool
  run and the agent read are one procedure, and records what the finding count
  within a block says about which arm fired.

## [0.1.4] - 2026-08-18

### Added

- SD-Q006 ledger-stamp: orchestration-ledger stamp diction, meaning a verdict
  or measurement verb directly against a bare ISO date, or the owner-verdict
  phrase with its date. The verb set leaves out the three transactional verbs
  that stamp everyday operations mail, and both arms require the date, because
  ordinary English produces the dateless shapes on its own.
- SD-J001 context-break: the four chat-template control tokens, the
  new-instructions and system-override banners in any casing, the caps-only
  scanner line, and the caps-only resume-injection phrase. Word-bounded, so
  the everyday lowercase recruiting sentence never fires.
- Seven injection phrases sourced from published 2026 analyses of malicious
  agent skills and resume injection.

### Documentation

- The invisible-unicode guard records a standing non-rule: statistical
  token-choice watermarks insert no codepoints, so no invisible-character
  finding speaks to them.

## [0.1.3] - 2026-08-02

### Added

- Bundle mode. Two or more paths produce a full per-file report for each
  input plus `cross_file_duplication`, the verbatim runs of ten or more words
  shared between files, each occurrence cited by path and span.
- SD-Q005 self-duplication: verbatim repeated runs inside one document.
- A `container` label on every finding (`prose`, `fenced-code`, `blockquote`,
  `quoted`, `heading`) from a linear heuristic pre-pass. The label annotates
  and never suppresses.
- The `densities` stats block, with raw and residual rates per 1000 words per
  class, and the repeatable `--allow-term` flag, which labels findings
  matching a stated topic word with `topic_term: true` and leaves them out of
  the residual figures.
- The inbound selection gains SLOP-V004 agent-loop vocabulary, SLOP-A005
  metaphor-reach idioms, and SLOP-C008 not-X-but-Y pairs.

### Changed

- SD-Q004 reads a bounded terminal test for `.`, so an abbreviation-internal
  period (`the U.S.`) neither closes a contrastive tail early nor truncates
  the recovered clause.

## [0.1.2] - 2026-08-02

### Added

- SD-Q004 contrastive-negation: the trailing `, not X.` and `, never X.` tag
  closing its sentence, plus the about-reframe and copular not-X-but-Y
  patterns. The recovered clause goes through a suppression classifier first,
  because reply-by-Friday-not-Monday mail is the dominant legitimate use.
- SD-Q003 `provenance-markers`: a submission that describes itself as a
  rewrite of another project, as a match against a reference API, or as a
  feature-for-feature copy.

## [0.1.1] - 2026-08-02

### Documentation

- The README and the skill drop their own formulaic register. The
  contrastive-negation caveat is catalogued in `references/rules.md` ahead of
  the rule that reads it.

## [0.1.0] - 2026-08-02

Initial public release.

### Added

- The evidence report, in three finding categories plus input measurements:
  `paste_residue` for what a copy out of a generation surface leaves behind,
  `injection_patterns` for phrasing that addresses an assistant,
  `quality_patterns` for formulaic-writing patterns, and `stats` for the
  density denominators. Every finding cites a byte span and a verbatim
  snippet, and the report carries no verdict.
- Every rule as data, loaded from `data/inbound/inbound.toml`. The reference
  lexicons in `data/words/` and `data/policy.toml` are vendored from the
  ai-slop crate, and the inbound selection and its edits live in
  `data/inbound/`.
- The `slop-detector` binary, reading a path or stdin and printing the report
  as JSON, and the `analyze` library entry point, a pure total function of the
  input text.
- The coupled agent skill in `skills/slop-detector/`, which directs the read:
  state the purpose, run the tool, take residue and injection findings per
  hit, compute quality densities per class, and report with cited spans,
  leaving the conclusion to the human.
