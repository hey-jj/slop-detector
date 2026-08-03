# slop-detector

Pattern detector for inbound text: emails, writing samples, proposals,
deck copy. It prints an evidence report where every finding cites a byte
span and a verbatim snippet, and the reader decides what the findings
mean.

## What it detects

The report splits findings into three categories, plus input measurements.

- `paste_residue`: residue left by copying out of a generation surface.
  Provider-attribution lines, chat-export citation artifacts, chat-tool
  tracking parameters, turn markers, citation-delimiter codepoints,
  invisible unicode, and positional typographic spaces.
- `injection_patterns`: phrasing that addresses an assistant, such as
  demands to disregard prior guidance or reveal hidden configuration.
- `quality_patterns`: formulaic-writing patterns, from the measured
  excess-vocabulary set through business-register density, structural
  scaffolding, agent-loop vocabulary, and verbatim self-duplication.
- `stats`: `word_count`, `byte_len`, and the per-class `densities`
  block with raw and residual rates per 1000 words.

Read each `paste_residue` and `injection_patterns` finding individually.
Read `quality_patterns` as densities against the word count. Every
finding carries a `container` label (`prose`, `fenced-code`,
`blockquote`, `quoted`, `heading`) from a linear heuristic pre-pass, so
the reader can discount fenced or quoted material. The label annotates
and never suppresses: the tool scans raw bytes with no markdown
segmentation.

Every rule is data, loaded from `data/inbound/inbound.toml`. The
reference lexicons in `data/words/` and `data/policy.toml` are vendored
from the [ai-slop](https://crates.io/crates/ai-slop) crate. The inbound
selection and its edits live in `data/inbound/`.

## CLI

The `slop-detector` binary reads file path arguments, or stdin when no
path is given, and prints the report as JSON on stdout.

```
slop-detector inbound.txt
cat inbound.txt | slop-detector
slop-detector deck-v1.txt deck-v2.txt
slop-detector --allow-term beekeeping paper.txt
```

One input produces the single-document report. Two or more paths produce
the bundle report: a full per-file report for each input plus
`cross_file_duplication`, the verbatim runs of ten or more words shared
between files, each occurrence cited by path and span. The repeatable
`--allow-term` flag labels findings matching a stated topic word with
`topic_term: true`. Labeled hits stay in the report and leave the
residual density figures.

Exit 0 means a report was produced. Exit 1 is a read or encoding error.
Each input over 4 MiB is rejected with exit 40. An input with zero
matches yields empty arrays. A closed output pipe ends the run quietly.

## Library

```rust
let report = slop_detector::analyze(&text);
for finding in &report.paste_residue {
    println!("{} at {:?}: {}", finding.rule_id, finding.span, finding.snippet);
}
let bundle = slop_detector::analyze_bundle(&docs); // (path, text) pairs
```

`analyze` is a pure, total function of the input text, and each per-file
report inside `analyze_bundle` equals the `analyze` output for that text.

## The coupled skill

The crate pairs with an agent skill, in the repository at
[`skills/slop-detector/`](https://github.com/hey-jj/slop-detector/tree/main/skills/slop-detector),
that directs a human-driven agent through the read: state the purpose,
run the tool, read residue and injection findings per hit, compute
quality densities per class, and report with cited spans, leaving the
conclusion to the human. Per-rule interpretation notes live in the
skill's `references/rules.md`.

## License

MIT OR Apache-2.0.
