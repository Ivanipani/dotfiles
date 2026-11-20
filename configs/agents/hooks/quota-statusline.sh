#!/bin/sh
# Claude statusLine command. Claude only exposes subscription quota to the
# statusline, never to hooks, so cache it here for quota-guard.sh to read,
# then render the line with ccstatusline.
set -eu

. "$(dirname "$0")/agent-state.sh"

input=$(cat)
limits=$(printf '%s' "$input" | jq -c '.rate_limits // empty')

if [ -n "$limits" ]; then
  dir="$(state_dir)/quota"
  mkdir -p "$dir"
  # Write-then-rename so a hook never reads a half-written file.
  printf '%s\n' "$limits" >"$dir/claude.json.$$"
  mv "$dir/claude.json.$$" "$dir/claude.json"
fi

# bunx is faster than npx when present.
if command -v bunx >/dev/null 2>&1; then
  printf '%s' "$input" | bunx -y ccstatusline@latest
else
  printf '%s' "$input" | npx -y ccstatusline@latest
fi
