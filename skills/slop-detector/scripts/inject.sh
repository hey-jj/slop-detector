#!/bin/sh
# Print the slop-detector SKILL.md body with its YAML frontmatter stripped.
# Paste the output verbatim into a sub-agent or shell-job prompt.
set -eu
dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
skill="$dir/../SKILL.md"
[ -f "$skill" ] || { echo "inject.sh: $skill not found" >&2; exit 1; }
awk 'body { print; next } /^---[ \t]*$/ { fences++; if (fences == 2) body = 1 }' "$skill"
