#!/bin/sh
# Claude statusLine command. Claude only exposes subscription quota to the
# statusline, never to hooks, so cache it here for quota-guard.sh to read.
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

printf '%s' "$input" | jq -r '
  [ (.rate_limits.five_hour.used_percentage // empty | "5h:\(floor)%"),
    (.rate_limits.seven_day.used_percentage // empty | "7d:\(floor)%") ]
  | join(" ")'
