---
name: rex-workspace
description: Create, open, edit, and delete workspace sets (jj workspaces across repos, one Rex session per set) with the rex-workspace CLI.
---

# rex-workspace

Rex counterpart of the zellij-workspace plugin, a native Rust CLI (`src/rex-workspace`) with a full-screen TUI when run bare. A set lives at `~/.local/share/workspaces/<set>/` with a `workspace.yaml` manifest, one jj workspace per project, and an `AGENTS.md`. The format is shared with zellij-workspace, so a set opens from either.

Opening a set creates session `ws-<set>`: a tab at the set root, then one tab per project holding `nvim` and the agent side by side, with `jjui` in a floating layer toggled by `alt+g`.

## Commands

Pass names explicitly when scripting. Missing arguments open the TUI (needs a TTY), so an agent without a terminal must always name them.

- `rex-workspace`: the full-screen manager.
- `rex-workspace ls [--json]`: sets, projects, whether a session is open, and its tab count.
- `rex-workspace new SET [REPO...]`: create the set from repos found under `~/github` and `~`, then open it.
- `rex-workspace open [SET]`: create the session, or switch to it if open.
- `rex-workspace add [SET] [REPO...]` / `remove [SET] [PROJECT...]`: change membership; an open session gains or loses the tab.
- `rex-workspace delete [SET] --yes`: bookmark any uncommitted work as `saved-<set>`, forget the workspaces, delete the directory and kill the session.
- `rex-workspace tui [SET] --action open|new|add|remove|delete`: the manager, starting on an action.

`SET` defaults to the current `ws-` session. `remove` and `delete` are destructive: confirm the target with the user first.

## Manager keys

Chosen to avoid Rex's `ctrl+a`, `ctrl+h/j/k/l` and `alt+` chords. `j/k` move, `Enter` open, `n` new, `a` add repos, `x` remove projects, `D` delete (asks first), `/` fuzzy filter (matches set and project names), `Tab` focus projects, `r` reload, `?` help, `q` quit. Pickers filter as you type; `Tab` marks, `Enter` accepts.

## Rex keys

`ctrl+a w` enters workspace mode: `o` manager, `n` new, `a` add, `x` remove, `d` delete, `j` jjui. Each opens the manager in a near-fullscreen float; outside a `ws-` session the manager asks you to pick a set instead of failing.
