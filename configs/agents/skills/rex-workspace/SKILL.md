---
name: rex-workspace
description: Create, open, edit, close, pause, and delete workspace sets (jj workspaces across repos, one Rex session per set) with the rex-workspace CLI.
---

# rex-workspace

Rex counterpart of the zellij-workspace plugin, a native Rust CLI (`src/rex-workspace`) with a full-screen TUI when run bare. A set lives at `~/.local/share/workspaces/<set>/` with a `workspace.yaml` manifest, one jj workspace per project, and an `AGENTS.md`. The format is shared with zellij-workspace, so a set opens from either.

Opening a set creates session `ws-<set>`: a tab at the set root, then one tab per project holding `nvim` and the agent side by side, with `jjui` in a floating layer toggled by `alt+g`.

## Commands

Pass names explicitly when scripting. Missing arguments open the TUI (needs a TTY), so an agent without a terminal must always name them.

- `rex-workspace`: the full-screen manager.
- `rex-workspace ls [--json]`: sets, projects, session state (`open`, `paused`, `closed`, or none), and tab count.
- `rex-workspace new SET [REPO...]`: create the set from repos found under `~/github` and `~`, then open it.
- `rex-workspace open [SET]`: create the session, or switch to it if open (resuming it if paused). A closed set comes back with its saved tabs, splits and pane directories.
- `rex-workspace close [SET]`: save the session's layout to `<set>/.session.json` and kill the session, freeing all its processes. The set stays on disk. Floating layers are not saved; a pane started with a command (nvim) re-runs it, and any other pane comes back as a shell in its last directory.
- `rex-workspace pause [SET]` / `resume [SET]`: SIGSTOP / SIGCONT every process under the set's panes. A paused set uses no CPU but keeps its memory.
- `rex-workspace add [SET] [REPO...]` / `remove [SET] [PROJECT...]`: change membership; an open session gains or loses the tab.
- `rex-workspace new SET REPO... -m GOAL [--link URL]...`: also state what the set is for.
- `rex-workspace describe [SET]` prints the set's goal and links (`--json` for data); `-m GOAL` replaces the goal (empty clears it), `--link URL` appends a link, `--clear-links` drops them. Both go into `workspace.yaml` and a `## Goal` section of the set's `AGENTS.md`.
- `rex-workspace sync [SET]`: `jj git fetch` once per repository, then `jj rebase -b @ -d 'trunk()'` in every project that is behind trunk. It reports conflicts but does not resolve them, and skips repos with no remote trunk. It exits non-zero when any project fails or has conflicts. The manager shows conflicted projects as `conflict`. A rebase rewrites commits: `jj undo` inside a project reverts it.
- `rex-workspace delete [SET] --yes`: bookmark any uncommitted work as `saved-<set>`, forget the workspaces, delete the directory and kill the session.
- `rex-workspace tui [SET] --action open|new|add|remove|delete`: the manager, starting on an action.

`SET` defaults to the current `ws-` session. `remove` and `delete` are destructive: confirm the target with the user first. `close` ends every program in the session (unsaved editor buffers, running agents), so confirm it too.

## Manager keys

Chosen to avoid Rex's `ctrl+a`, `ctrl+h/j/k/l` and `alt+` chords. `j/k` move, `Enter` open, `n` new, `a` add repos, `x` remove projects, `D` delete (asks first), `c` close (asks first), `p` pause / resume, `s` sync, `e` edit the goal, `/` fuzzy filter (matches set names, project names and goals), `Tab` focus projects, `r` reload, `?` help, `q` quit. Pickers filter as you type; `Tab` marks, `Enter` accepts.

## Rex keys

One shared Rex session named `workspaces` hosts the manager for the whole app, created on first use. `ctrl+a w` jumps to it from any session and back again; inside the manager `q`/`Esc` also goes back, and opening a set switches to it while the manager stays running (it refreshes every few seconds). `alt+g` toggles jjui.
