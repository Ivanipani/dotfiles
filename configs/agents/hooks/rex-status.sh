#!/bin/sh
# Report the agent's state to Rex (OSC 7501) so its tab spinner and session
# indicator follow the agent. Silent outside Rex or without a terminal.
# Usage: rex-status.sh [app]   (hook JSON on stdin)
set -eu

[ -n "${REX_BLOCK:-}" ] || exit 0

input=$(cat)
app=${1:-claude-code}
event=$(printf '%s' "$input" | jq -r '.hook_event_name // empty')
kind=$(printf '%s' "$input" | jq -r '.notification_type // empty')
message=$(printf '%s' "$input" | jq -r '.message // empty')

case "$event:$kind" in
  UserPromptSubmit:* | PostToolUse:*) state=working; extra="" ;;
  Stop:* | SubagentStop:*) state=done; extra="" ;;
  PermissionRequest:* | Notification:permission_prompt)
    state=blocked; extra=":kind=permission"; message=${message:-Needs permission} ;;
  Notification:elicitation_dialog | Notification:elicitation_url_dialog | Notification:agent_needs_input)
    state=blocked; extra=":kind=question"; message=${message:-Needs your input} ;;
  Notification:idle_prompt) state=idle; extra="" ;;
  SessionStart:*) state=idle; extra="" ;;
  *) exit 0 ;;
esac

# Rex values are single-line base64; the sequence caps msg at 2048 bytes.
msg=$(printf '%s' "$message" | tr -d '[:cntrl:]' | cut -c1-500 | base64 | tr -d '\n')
tty=${REX_STATUS_TTY:-/dev/tty}
printf '\033]7501;state=%s:app=%s%s%s\033\\' \
  "$state" "$app" "$extra" "${msg:+:msg=$msg}" >"$tty" 2>/dev/null || true
