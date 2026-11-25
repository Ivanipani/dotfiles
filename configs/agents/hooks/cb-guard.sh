#!/bin/sh
# PreToolUse(Bash) hook: in a repo whose `main` conveyor-belt owns, deny an agent's push of
# `main` and point at `cb submit`. Repos are enrolled one `owner/name` per line in
# ~/.config/conveyor-belt/repos; without that file the hook does nothing. It runs only in
# agent sessions, so a person's own shell is never affected.
set -eu

repos="${XDG_CONFIG_HOME:-$HOME/.config}/conveyor-belt/repos"
[ -r "$repos" ] || exit 0

input=$(cat)

cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')
# A push only where a command can start: line start, or after a separator.
push=$(printf '%s' "$cmd" |
  grep -Eo '(^|[;&|(`]|\$\()[[:space:]]*jj[[:space:]]([^;&|]*[[:space:]])?git[[:space:]]+push([[:space:]][^;&|]*)?' || true)
[ -n "$push" ] || exit 0

# Pushes `main` when it names it, pushes everything, or names nothing (tracked bookmarks).
names_main='(-b|--bookmark|--named)[=[:space:]]+["'\'']?(exact:)?main(=|["'\'']|[[:space:]]|$)'
everything='[[:space:]]--(all|tracked)([[:space:]]|$)'
selective='[[:space:]](-b|--bookmark|--named|-c|--change|-r|--revisions)([=[:space:]]|$)'
if ! printf '%s' "$push" | grep -Eq -e "$names_main" -e "$everything"; then
  printf '%s' "$push" | grep -Eq -e "$selective" && exit 0
fi

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
if [ -n "$cwd" ]; then
  cd "$cwd" || exit 0
fi
command -v jj >/dev/null 2>&1 || exit 0
origin=$(jj git remote list 2>/dev/null | sed -n 's/^origin[[:space:]][[:space:]]*//p' | sed 's/\.git$//')
[ -n "$origin" ] || exit 0
repo=
while read -r name; do
  case "$name" in "" | \#*) continue ;; esac
  case "$origin" in
    *[:/]"$name") repo=$name ;;
  esac
done < "$repos"
[ -n "$repo" ] || exit 0

jq -n --arg repo "$repo" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: ("conveyor-belt owns `main` of " + $repo + "; agents do not push it. Submit the change instead: `cb submit --land` (add `-r <revset>` for a revision other than `@-`), then `cb status <request> --wait`. Push other bookmarks by name (`jj git push -b <bookmark>` or `--named <name>=<change>`). See the `conveyor-belt` skill. If the belt is down, tell the user; pushing `main` by hand is theirs to do.")
  }
}'
