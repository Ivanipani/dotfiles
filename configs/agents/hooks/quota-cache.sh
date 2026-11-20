#!/bin/sh
# Silent ccstatusline custom-command widget. Claude only exposes subscription
# quota to the statusline, never to hooks, so cache it for quota-guard.sh.
set -eu

. "$(dirname "$0")/agent-state.sh"

limits=$(jq -c '.rate_limits // empty')

if [ -n "$limits" ]; then
  dir="$(state_dir)/quota"
  mkdir -p "$dir"
  # Write-then-rename so a hook never reads a half-written file.
  printf '%s\n' "$limits" >"$dir/claude.json.$$"
  mv "$dir/claude.json.$$" "$dir/claude.json"
fi
