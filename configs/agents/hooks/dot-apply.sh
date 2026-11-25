#!/bin/sh
# Stop hook, project-scoped via .claude/settings.json and .codex/hooks.json: give agents one chance to activate dotfile changes before finishing.
set -eu

input=$(cat)
[ "$(printf '%s' "$input" | jq -r '.stop_hook_active // false')" != "true" ] || exit 0

cwd=$(printf '%s' "$input" | jq -r '.cwd // empty')
[ -n "$cwd" ] || exit 0
cd "$cwd" || exit 0
command -v jj >/dev/null 2>&1 || exit 0
root=$(jj root 2>/dev/null) || exit 0

# Scope this shared hook to the main dotfiles checkout: applying from a workspace relinks $HOME into it.
[ -f "$root/bin/dot" ] && [ -f "$root/configs/agents/hooks/dot-apply.sh" ] || exit 0
[ ! -f "$root/.jj/repo" ] || exit 0

jq -n '{
  decision: "block",
  reason: "Before finishing this dotfiles task, if you changed configuration, attempt `dot apply` from the project root so the requested settings take effect on the system. If you changed Rust crates in src/, run `task deploy` first, then `dot apply`. If you already attempted these commands for the final changes, do not repeat them. Use the normal permission flow if required. Resolve routine failures where possible, and report any remaining failure or setting that still needs a reload/restart in your final response. If this task made no configuration or Rust changes, no apply is needed. Then finish."
}'
