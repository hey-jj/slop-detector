# Pattern data

Two layers live here.

`words/` and `policy.toml` are vendored verbatim from the ai-slop crate
(version 0.1.2, commit `9797c33`, `policy/words/*.txt` and `policy/policy.toml`).
They are the unmodified reference copy. Do not edit them. Re-vendor from ai-slop
to update.

`inbound/` is the slop-detector selection: `inbound.toml` is the loaded rule
table (id, category, mechanism, lexicon or patterns, boundary mode, per-rule
thresholds, guard text), and the `.txt` files beside it are the lexicons carried
from the vendored data with edits. Rules carried unchanged reference `words/`
directly from `inbound.toml`. Every loaded pattern lives in this directory or in
a file `inbound.toml` names. No pattern is hard-coded.
