#!/bin/sh
# SessionStart hook: stdout from this event becomes context Claude can see, so
# hand it the jj state up front instead of paying for it in tool calls later.
set -eu

input=$(cat)

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi

command -v jj >/dev/null 2>&1 || exit 0
jj root >/dev/null 2>&1 || exit 0

printf 'This repo is managed by jujutsu, not git. State at session start:\n\n'

printf '$ jj log -r "ancestors(@, 4)"\n'
jj --no-pager log -r 'ancestors(@, 4)' --no-graph -T \
  'change_id.shortest(8) ++ " " ++ if(bookmarks, bookmarks.join(" ") ++ " | ", "") ++ if(description, description.first_line(), "(no description set)") ++ "\n"' \
  2>/dev/null || true

printf '\n$ jj status\n'
# A large working copy can list hundreds of paths; the shape is what matters.
status=$(jj --no-pager status 2>/dev/null || true)
printf '%s\n' "$status" | head -25
if [ "$(printf '%s\n' "$status" | wc -l)" -gt 25 ]; then
  printf '... (truncated; run `jj status` for the full list)\n'
fi
