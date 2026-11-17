#!/bin/sh
# Sourced by the quota/handoff hooks: where agent state lives on disk.
# Override with AGENT_STATE_DIR (tests, alternate machines).

state_dir() {
  printf '%s\n' "${AGENT_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/agents}"
}

# One handoff file per workspace, kept outside the repo so it never dirties the
# jj working copy. Takes the hook's cwd.
handoff_path() {
  root=$(cd "$1" 2>/dev/null && jj root 2>/dev/null) || root=$1
  slug=$(printf '%s' "$root" | tr -c 'A-Za-z0-9' '-')
  printf '%s/handoff/%s.md\n' "$(state_dir)" "$slug"
}
