#!/bin/sh
# Notification hook: raise a macOS banner whenever Claude Code is waiting on the
# user — a permission prompt, a question, or an idle input box.
set -eu

command -v osascript >/dev/null 2>&1 || exit 0

input=$(cat)
message=$(printf '%s' "$input" | jq -r '.message // "Waiting for your input"')
cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
subtitle=${cwd:+$(basename "$cwd")}

osascript \
  -e 'on run argv' \
  -e 'display notification (item 1 of argv) with title "Claude Code" subtitle (item 2 of argv) sound name "Ping"' \
  -e 'end run' \
  "$message" "$subtitle" >/dev/null 2>&1 || true
