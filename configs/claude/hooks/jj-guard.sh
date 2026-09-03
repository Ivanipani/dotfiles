#!/bin/sh
# PreToolUse(Bash) hook: in a jj repo, git is the wrong tool — deny the call and
# name the jj equivalent, so the "use jujutsu" rule is enforced rather than just
# requested. Repos that jj does not manage are left alone.
set -eu

input=$(cat)

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi

command -v jj >/dev/null 2>&1 || exit 0
jj root >/dev/null 2>&1 || exit 0

cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')
[ -n "$cmd" ] || exit 0

# `jj git push` and friends are jj subcommands, not the git CLI.
cmd=$(printf '%s' "$cmd" | sed 's/jj[[:space:]][[:space:]]*git[[:space:]]/jj /g')

# Match git only where a command can start: line start, or after a separator.
printf '%s' "$cmd" |
  grep -Eq '(^|[;&|(`]|\$\()[[:space:]]*(sudo[[:space:]]+)?git([[:space:]]|$)' || exit 0

jq -n '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: "This repo is managed by jujutsu; git is blocked here. Use jj instead: `jj st` / `jj diff` for state, `jj log` for history, `jj commit -m` to record, `jj bookmark` for branches, `jj git push` / `jj git fetch` to talk to the remote. If the task genuinely needs the git CLI, stop and ask the user rather than working around this."
  }
}'
