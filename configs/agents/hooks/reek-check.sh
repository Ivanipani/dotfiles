#!/bin/sh
# Stop hook: in a project that uses reek (reek.toml at its root or in reekcfg/), format and lint
# the files changed in the jj working copy. Formatting is applied silently; lint
# failures are fed back to the agent to fix.
set -eu

input=$(cat)

# The hook fires again on the stop that follows the fix; don't loop.
if [ "$(printf '%s' "$input" | jq -r '.stop_hook_active // false')" = "true" ]; then
  exit 0
fi

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi

command -v reek >/dev/null 2>&1 || exit 0
command -v jj >/dev/null 2>&1 || exit 0
root=$(jj root 2>/dev/null) || exit 0
[ -f "$root/reek.toml" ] || [ -f "$root/reekcfg/reek.toml" ] || exit 0
cd "$root"

# Only check files that still exist; deleted paths are not valid targets.
files=$(jj diff --name-only 2>/dev/null | while IFS= read -r f; do
  [ -f "$f" ] && printf '%s\n' "$f"
done)
[ -n "$files" ] || exit 0

# shellcheck disable=SC2086 # one target per line, no spaces expected
set -f
IFS='
'
reek fmt $files >/dev/null 2>&1 || true

if output=$(reek lint $files 2>&1); then
  exit 0
fi

jq -n --arg out "$output" '{
  decision: "block",
  reason: ("`reek lint` failed on the changed files. Fix these findings, then stop:\n\n" + $out)
}'
