# Pattern data

Two layers live here.

`words/` and `policy.toml` are vendored verbatim from the ai-slop crate.
They are the unmodified reference copy. Do not edit them. Re-vendor from
ai-slop to update. `words/` matches ai-slop 0.1.6 `policy/words/*.txt`
(the 0.1.6 refresh added `agent-loop.txt` and moved the two
request-reference phrases there out of `assistant-offers.txt`).
`policy.toml` is the 0.1.6 snapshot (policy version 1.2.0). Its `digest`
field is empty in the vendored source by ai-slop convention: ai-slop
computes the value at its own build, and nobody hand-writes it.

`inbound/` is the slop-detector selection: `inbound.toml` is the loaded rule
table (id, category, mechanism, lexicon or patterns, boundary mode, per-rule
thresholds, guard text), and the `.txt` files beside it are the lexicons carried
from the vendored data with edits. Rules carried unchanged reference `words/`
directly from `inbound.toml`. Every loaded pattern lives in this directory or in
a file `inbound.toml` names. No pattern is hard-coded.
