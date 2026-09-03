#!/bin/sh
# Notification hook: raise a macOS banner whenever Claude Code is waiting on the
# user — a permission prompt, a question, or an idle input box.
set -eu

command -v osascript >/dev/null 2>&1 || exit 0

input=$(cat)
kind=$(printf '%s' "$input" | jq -r '.notification_type // empty')
message=$(printf '%s' "$input" | jq -r '.message // .notification_data.message // empty')

# The payload does not always carry a message; the type always says enough.
if [ -z "$message" ]; then
  case "$kind" in
    permission_prompt) message="Needs permission to continue" ;;
    idle_prompt) message="Waiting for your input" ;;
    agent_needs_input) message="An agent needs your input" ;;
    agent_completed) message="An agent finished" ;;
    elicitation_dialog | elicitation_url_dialog) message="An MCP server is asking for input" ;;
    quota_auto_resume_fired) message="Quota reset — the session resumed" ;;
    quota_auto_resume_stale) message="Quota reset, but the session is too old to resume" ;;
    *) message=${kind:-Waiting for your input} ;;
  esac
fi

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
subtitle=${cwd:+$(basename "$cwd")}

osascript \
  -e 'on run argv' \
  -e 'display notification (item 1 of argv) with title "Claude Code" subtitle (item 2 of argv) sound name "Ping"' \
  -e 'end run' \
  "$message" "$subtitle" >/dev/null 2>&1 || true
