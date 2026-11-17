#!/bin/sh
# SessionStart hook: if a previous agent left a recent handoff for this
# workspace (see quota-guard.sh), point the new session at it.
set -eu

. "$(dirname "$0")/agent-state.sh"

cwd=$(jq -r '.cwd // empty')
handoff=$(handoff_path "${cwd:-.}")

# Older than three days is stale: the work has moved on.
[ -n "$(find "$handoff" -mtime -3 2>/dev/null)" ] || exit 0

printf 'A previous agent stopped early (usage quota) and left a handoff for this workspace: %s\nRead it and continue from its next steps. Delete it once the work is finished.\n' "$handoff"
