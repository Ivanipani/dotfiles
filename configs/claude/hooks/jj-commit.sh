#!/bin/sh
# Stop hook: when work is finished and the jj working copy still holds changes,
# send Claude back in to record them as a single conventional commit.
set -eu

input=$(cat)

# The hook fires again on the stop that follows the commit; don't loop.
if [ "$(printf '%s' "$input" | jq -r '.stop_hook_active // false')" = "true" ]; then
  exit 0
fi

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi

command -v jj >/dev/null 2>&1 || exit 0
jj root >/dev/null 2>&1 || exit 0

# Nothing to record if the working-copy commit is empty.
if [ "$(jj log -r @ --no-graph -T empty 2>/dev/null)" != "false" ]; then
  exit 0
fi

jq -n '{
  decision: "block",
  reason: "The jujutsu working copy (@) still has uncommitted changes. Record them now:\n\n1. Read the change with `jj status` and `jj diff`.\n2. Write ONE commit message in Conventional Commits style (`type(scope): description`) covering the whole change — see the conventional-commits skill. Prefer a single commit over several; only split if the working copy genuinely holds unrelated changes.\n3. Run `jj commit -m \"...\"` (a multi-line message needs repeated -m flags). If @ already has a description, replace it with one that describes the final state.\n\nUse jj, never git. Then stop."
}'
