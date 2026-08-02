# Pattern data

Two layers live here.

`words/` and `policy.toml` are vendored verbatim from the ai-slop crate.
They are the unmodified reference copy. Do not edit them. Re-vendor from
ai-slop to update. `words/` matches ai-slop 0.1.5 `policy/words/*.txt`
(the 0.1.5 refresh added `provenance-oblique.txt` and changed no other
file). `policy.toml` remains the 0.1.2 snapshot (commit `9797c33`,
`policy/policy.toml`) pending the coordinated re-vendor.

`inbound/` is the slop-detector selection: `inbound.toml` is the loaded rule
table (id, category, mechanism, lexicon or patterns, boundary mode, per-rule
thresholds, guard text), and the `.txt` files beside it are the lexicons carried
from the vendored data with edits. Rules carried unchanged reference `words/`
directly from `inbound.toml`. Every loaded pattern lives in this directory or in
a file `inbound.toml` names. No pattern is hard-coded.
