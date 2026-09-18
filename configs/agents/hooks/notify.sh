#!/bin/sh
# Raise a banner for client lifecycle notifications.
set -eu

input=$(cat)
kind=$(printf '%s' "$input" | jq -r '.notification_type // .hook_event_name // empty')
message=$(printf '%s' "$input" | jq -r '.message // .notification_data.message // empty')

# The payload does not always carry a message; the type always says enough.
if [ -z "$message" ]; then
  case "$kind" in
    permission_prompt | PermissionRequest) message="Needs permission to continue" ;;
    Stop) message="Finished responding" ;;
    idle_prompt) message="Waiting for your input" ;;
    agent_needs_input) message="An agent needs your input" ;;
    agent_completed | SubagentStop) message="An agent finished" ;;
    elicitation_dialog | elicitation_url_dialog) message="An MCP server is asking for input" ;;
    quota_auto_resume_fired) message="Quota reset — the session resumed" ;;
    quota_auto_resume_stale) message="Quota reset, but the session is too old to resume" ;;
    *) message=${kind:-Waiting for your input} ;;
  esac
fi

# Codex has lifecycle events instead of Claude's Notification event.
if [ "${1:-}" = "Codex" ]; then
  case "$kind" in
    Stop | SubagentStop) state=completed ;;
    *) state=waiting ;;
  esac
  "$(dirname "$0")/attention.sh" "$state"
fi
command -v osascript >/dev/null 2>&1 || exit 0

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
subtitle=${cwd:+$(basename "$cwd")}

osascript \
  -e 'on run argv' \
  -e 'display notification (item 1 of argv) with title (item 3 of argv) subtitle (item 2 of argv) sound name "Ping"' \
  -e 'end run' \
  "$message" "$subtitle" "${1:-Claude Code}" >/dev/null 2>&1 || true
