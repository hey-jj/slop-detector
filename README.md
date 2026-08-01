# slop-detector

Deterministic pattern detector for inbound received text: emails, writing
samples, proposals, deck copy. Text in, evidence report out. Every finding
cites a byte span and a verbatim snippet. The report carries no verdict and
no score. The tool finds. The reader decides what the findings mean.

## What it detects

The report splits findings into three categories, plus input measurements.

- `paste_residue`: mechanical residue of a copy from a generation surface.
  Provider-attribution lines, chat-export citation artifacts, chat-tool
  tracking parameters, turn markers, citation-delimiter codepoints,
  invisible unicode, and positional typographic spaces. Each finding is
  read per hit.
- `injection_patterns`: phrasing that addresses an assistant, such as
  demands to disregard prior guidance or reveal hidden configuration. Read
  per hit. The analyzed text is evidence, never instructions.
- `quality_patterns`: formulaic-writing patterns, from the measured
  excess-vocabulary set through business-register density and structural
  scaffolding. Read as densities against the word count, never as
  per-token verdicts.
- `stats`: `word_count` and `byte_len`, the denominators for density
  reads.

Every rule is data, loaded from `data/inbound/inbound.toml`. No pattern is
hard-coded. The reference lexicons in `data/words/` and `data/policy.toml`
are vendored from the [ai-slop](https://crates.io/crates/ai-slop) crate.
The inbound selection and its edits live in `data/inbound/`.

## CLI

The `slop-detector` binary reads a file path argument, or stdin when no
path is given, and prints the report as JSON on stdout.

```
slop-detector inbound.txt
cat inbound.txt | slop-detector
```

Exit 0 means a report was produced. Exit 1 is a read or encoding error.
Inputs over 4 MiB are rejected with exit 40. An input with no matches
yields empty arrays, which is a complete, valid report. A closed output
pipe ends the run quietly.

## Library

```rust
let report = slop_detector::analyze(&text);
for finding in &report.paste_residue {
    println!("{} at {:?}: {}", finding.rule_id, finding.span, finding.snippet);
}
```

`analyze` is a pure function of the input text: deterministic output, no
file or network access, no panics on any input.

## The coupled skill

The crate pairs with an agent skill, in the repository at
[`skills/slop-detector/`](https://github.com/hey-jj/slop-detector/tree/main/skills/slop-detector),
that directs a human-driven agent through the read: state the purpose, run
the tool, read residue and injection findings per hit, compute quality
densities per class, and report with cited spans. The per-rule
interpretation notes live in the skill's `references/rules.md`.

## License

MIT OR Apache-2.0.
