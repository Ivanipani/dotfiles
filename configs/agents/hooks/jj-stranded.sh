#!/bin/sh
# Stop hook: in a rex-workspace set, send the agent back in while any of its work
# exists only on this machine, so a finished session leaves nothing stranded.
# Asks once per distinct stranded state, so a push that cannot succeed (offline,
# no access) does not loop; the agent is told to report it instead.
set -eu

input=$(cat)

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi

command -v jj >/dev/null 2>&1 || exit 0
command -v rex-workspace >/dev/null 2>&1 || exit 0
jj root >/dev/null 2>&1 || exit 0

# Uncommitted changes are jj-commit.sh's to handle first.
if [ "$(jj log -r @ --no-graph -T empty 2>/dev/null)" = "false" ]; then
  exit 0
fi

# Not inside a set: nothing to check.
rows=$(rex-workspace stranded --json 2>/dev/null) || exit 0

state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/agents/stranded"
session=$(printf '%s' "$input" | jq -r '.session_id // "unknown"')
marker="$state_dir/$session"

if [ "$(printf '%s' "$rows" | jq 'length')" = "0" ]; then
  rm -f "$marker"
  exit 0
fi

if [ -f "$marker" ] && [ "$(cat "$marker")" = "$rows" ]; then
  exit 0
fi
mkdir -p "$state_dir"
printf '%s' "$rows" >"$marker"

list=$(printf '%s' "$rows" | jq -r '.[] | "- \(.project): \(.change) \"\(if .description == "" then "(no description)" else .description end)\" (\(.commits) commit(s)\(if (.bookmarks | length) > 0 then ", on " + (.bookmarks | join(" ")) else "" end))"')

jq -n --arg list "$list" '{
  decision: "block",
  reason: ("Work in this workspace set exists only on this machine. Other agents, and remote sessions, cannot see it:\n\n" + $list + "\n\nFor each, in its project directory:\n\n1. Keep it: give every commit a description (`jj describe -r <change> -m ...`, conventional commits), then push it. If it is already on a bookmark under your own `agent/<workspace>/` prefix, run `jj git push -b <bookmark>`; otherwise name one under your prefix after the change, in a few words, and push it: `jj git push --named agent/<workspace>/<short-name>=<change>`. If that name is taken, pick another.\n2. Or drop it: `jj abandon <change>`, only if it is not worth keeping.\n\nRun `rex-workspace stranded` to check that nothing is left. Use jj, never git. If a push fails (offline, no access), do not retry in a loop: say so in your final response, naming the changes. Then stop.")
}'
