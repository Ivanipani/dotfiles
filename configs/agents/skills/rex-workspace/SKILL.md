---
name: rex-workspace
description: Create, open, edit, and delete workspace sets (jj workspaces across repos, one Rex session per set) with the rex-workspace CLI.
---

# rex-workspace

Rex counterpart of the zellij-workspace plugin. A set lives at `~/.local/share/workspaces/<set>/` with a `workspace.yaml` manifest, one jj workspace per project, and an `AGENTS.md`. The format is shared with zellij-workspace, so a set opens from either.

Opening a set creates session `ws-<set>`: a tab at the set root, then one tab per project holding `nvim` and the agent side by side, with `jjui` in a floating layer toggled by `alt+g`.

## Commands

Pass names explicitly; without them the command opens an fzf picker, which needs a TTY.

- `rex-workspace ls`: sets, their projects, and whether a session is open.
- `rex-workspace new SET [REPO...]`: create the set from repos found under `~/github` and `~`, then open it.
- `rex-workspace open [SET]`: create the session, or switch to it if open.
- `rex-workspace add [SET] [REPO...]` / `remove [SET] [PROJECT...]`: change membership; an open session gains or loses the tab.
- `rex-workspace delete [SET] --yes`: bookmark any uncommitted work as `saved-<set>`, forget the workspaces, delete the directory and kill the session.

`SET` defaults to the current `ws-` session. `remove` and `delete` are destructive: confirm the target with the user first.

## Keys

`ctrl+a w` enters workspace mode: `o` open, `n` new, `a` add, `x` remove, `d` delete, `j` jjui.
