#!/bin/sh
# Usage: quota-guard.sh claude|codex
# UserPromptSubmit / PostToolUse / Stop hook: once usage quota passes
# AGENT_QUOTA_THRESHOLD percent (default 90), tell the agent to stop and write a
# handoff so another agent can resume. Stop is blocked until the handoff exists.
set -eu

. "$(dirname "$0")/agent-state.sh"

client=${1:?usage: quota-guard.sh claude|codex}
threshold=${AGENT_QUOTA_THRESHOLD:-90}
input=$(cat)
now=$(date +%s)

field() { printf '%s' "$input" | jq -r "$1 // empty"; }

# Highest used percent in a Codex rollout's newest rate_limits event, ignoring
# windows that have already reset. Rate limits ride on token_count events.
codex_used() {
  tail -c 1048576 "$1" | grep '"rate_limits"' | tail -n 1 | jq -r --argjson now "$now" '
    .payload.rate_limits | [.primary, .secondary]
    | map(select(. != null and .resets_at > $now) | .used_percent)
    | max // empty | floor' 2>/dev/null || true
}

case "$client" in
  claude)
    # The statusline caches the quota; hooks never receive it.
    cache="$(state_dir)/quota/claude.json"
    [ -f "$cache" ] || exit 0
    used=$(jq -r --argjson now "$now" '
      [.five_hour, .seven_day] | map(select(. != null and .resets_at > $now) | .used_percentage)
      | max // empty | floor' "$cache" 2>/dev/null || true)
    ;;
  codex)
    transcript=$(field .transcript_path)
    [ -f "$transcript" ] || exit 0
    # Quota is account-wide, so a fresh session with no token_count yet can
    # borrow it from the most recent rollouts beside its own.
    sessions=${transcript%%/sessions/*}/sessions
    used=
    for rollout in "$transcript" $(find "$sessions" -name 'rollout-*.jsonl' -mtime -1 -exec ls -t {} + 2>/dev/null | head -n 3); do
      used=$(codex_used "$rollout")
      [ -n "$used" ] && break
    done
    ;;
  *) exit 0 ;;
esac

[ -n "${used:-}" ] && [ "$used" -ge "$threshold" ] || exit 0

event=$(field .hook_event_name)
cwd=$(field .cwd)
handoff=$(handoff_path "${cwd:-.}")
marker="$(state_dir)/sessions/$(field .session_id)"
mkdir -p "$(dirname "$marker")" "$(dirname "$handoff")"

instructions="Write the handoff to $handoff (overwrite it) so another agent, Claude or Codex, can resume without this conversation. Include: the goal; what is done; what is in progress, with jj change IDs; the exact next steps; files touched; decisions and gotchas. Record working-copy changes with jj first. Then stop."

case "$event" in
  Stop)
    # The marker predates the handoff: written after it means the job is done.
    if [ "$(field .stop_hook_active)" = "true" ] ||
      { [ -f "$marker" ] && [ -n "$(find "$handoff" -newer "$marker" 2>/dev/null)" ]; }; then
      exit 0
    fi
    : >"$marker"
    jq -n --arg r "Usage quota is at ${used}%. Before stopping: $instructions" \
      '{decision: "block", reason: $r}'
    ;;
  UserPromptSubmit | PostToolUse)
    # Nudge once per session; the marker doubles as the Stop hook's baseline.
    [ -f "$marker" ] && exit 0
    : >"$marker"
    find "$(dirname "$marker")" -type f -mtime +7 -delete 2>/dev/null || true
    jq -n --arg e "$event" --arg c "Usage quota is at ${used}%, nearly exhausted. Do not start new work. $instructions" \
      '{hookSpecificOutput: {hookEventName: $e, additionalContext: $c}}'
    ;;
esac
