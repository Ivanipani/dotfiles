# dotfiles

Personal configuration for macOS and Linux, driven by `dot` — a
code-first engine that lives in this repo.

Each subsystem owns a `module.py` next to its config. A module never touches
the filesystem; it **yields actions**, and the engine decides whether anything
runs.

The front end is [Typer](https://typer.tiangolo.com) for the command surface,
[Rich](https://rich.readthedocs.io) for tables and diffs, and
[Textual](https://textual.textualize.io) for `dot ui`.

```
module.plan(host) -> [Action]  ->  reconcile(actions, state, disk)  ->  status | diff | apply
```

## Quick start

Only `uv` is required. Everything else bootstraps from it.

```sh
./bin/dot doctor    # are the binaries I depend on present?
./bin/dot status    # how does this machine differ from the repo?
./bin/dot diff      # show me exactly what would change
./bin/dot apply     # make the machine match
```

After the first `apply`, `dot` is on your `PATH` (`~/.local/bin/dot`)

## Commands

| command | what it does |
| --- | --- |
| `dot status [module]` | drift table for the whole machine; exits non-zero if anything drifted |
| `dot diff [module]` | unified diff of what `apply` would change |
| `dot plan [module]` | the raw action list a module produces |
| `dot apply [module]` | reconcile; `-n` dry run, `-f` overwrite unmanaged/hand-edited files |
| `dot install [module]` | install declared packages (slow, networked — deliberately separate from `apply`) |
| `dot doctor [module]` | check the binaries each module declares in `requires` |
| `dot adopt <path> <module>` | move an existing file into the repo and link it back |
| `dot prune` | remove entries no plan produces any more |
| `dot remove <module>` | unwind a module |
| `dot facts` | the resolved host + merged vars (use this to debug templates) |
| `dot ui` | full-screen drift browser (Textual) |

Every command takes an optional module list (`dot status zsh vcs`), `-h/--help`
is grouped into **Inspect** and **Change**, and `status`/`diff` exit non-zero
while anything is drifted — so they drop straight into a pre-commit hook or CI.

## The TUI

```sh
dot ui
```

A module list with per-module drift badges on the left, the drift table on the
right, and the unified diff for the highlighted row underneath.

| key | action |
| --- | --- |
| `r` | rescan |
| `a` | apply the selected module |
| `A` | apply everything |
| `f` | show/hide entries that are already `ok` |
| `q` | quit |

Applies run on a worker thread and the view rescans itself when they finish.

## Layout

```
src/dot/          the engine: facts, actions, reconcile, cli, tui
configs/<name>/   one module per subsystem: module.py next to the config it owns
vars/             the layered host/os/profile vars every plan reads
bin/dot           launcher; `tools` symlinks it into ~/.local/bin
tests/            snapshots plus a full apply into a throwaway $HOME
```

## Adding a module

Drop a `module.py` in a directory under `configs/`. Discovery is automatic.

```python
from dot import Dir, Module, Render, Symlink, brew


class Foo(Module):
    name = "foo"
    requires = ("foo",)  # dot doctor checks these
    after = ()  # modules that must be applied first

    def enabled(self, host):
        return host.macos and host.has("gui")

    def packages(self, host):
        yield brew("foo")

    def plan(self, host):
        cfg = host.xdg_config / "foo"
        yield Dir(cfg)
        yield Symlink(self.path("static.conf"), cfg / "static.conf")
        yield Render(self.path("templates/foo.conf.j2"), cfg / "foo.conf")
```

Actions: `Dir`, `Symlink`, `Render`, `Fetch`, `Run`. Anything imperative
subclasses `Action` and implements `check`/`apply`, so it stays inside the
reconcile loop instead of escaping into shell. `Run` requires an `unless`
predicate — without one it cannot be idempotent and reports an error.

## Composition

`plan()` is a pure function of a single frozen `Host`. Everything you may
branch on lives there: `host.os`, `host.macos`, `host.profile`, `host.tags`,
`host.var("some.key")`.

Vars deep-merge in a fixed order:

```
vars/default.toml
vars/os/{macos,linux}.toml
vars/profile/{personal,work}.toml
vars/host/<hostname>.toml
~/.config/dot/local.toml          # never committed: machine-local + secrets
```

Select a profile with `DOT_PROFILE=work dot status`, or set `profile = "work"`
in a host/local file.

`local.toml` is load-bearing, not just for secrets: **this repo is public**, so
anything that names internal hosts or paths goes there. `claude.auto_mode` is
the worked example — Claude Code generates that block from whatever repo it was
pointed at, and `configs/claude/templates/settings.json.j2` renders it back out
of local vars, so the committed template stays free of it.

## Symlinked vs rendered

Both end up as symlinks in your home directory, but they behave differently
when you edit them:

- **Symlinked** (`configs/zsh/common/*.zsh`, `configs/nvim/config`,
  `configs/zellij/config.kdl`) —
  edits in the repo are live immediately. Use this for anything you touch often.
- **Rendered** (`configs/*/templates/*.j2`) — output goes to `.build/<fingerprint>/`
  and the destination links at that. Edit the **template**, then `dot apply`.
  Use this only where the content genuinely varies by os/profile/host.

`Render(..., mode="copy")` writes the destination directly for the rare app
that rewrites its own config and chokes on symlinks. Copy mode tracks content
hashes, so it can tell a template change (`stale`, safe) from a hand edit
(`edited`, refused) from both (`conflict`, refused). `~/.claude/settings.json`
is the case that earns it: Claude Code rewrites that file itself, so copy mode
surfaces its writes as `edited` instead of quietly losing them.

## Scripts a shell calls

`DOTFILE_DIR` is the repo root, and it exists to *open* the repo (`ec`) —
never to reach into its layout. Anything a shell needs to **run** is symlinked
onto `PATH` by the module that owns it (`configs/zellij/scripts/*` and
`configs/zsh/scripts/*` land in `~/scripts`) and is called by bare name.

The point is that the plan knows about it. A `$DOTFILE_DIR/some/path` in a
shell fragment is invisible to `dot` and fails at runtime the day the file
moves; a `Symlink` action shows up in `dot status` as `dangling` instead.

## Drift states

| state | meaning | fixed by |
| --- | --- | --- |
| `ok` | matches | — |
| `absent` | nothing at the destination | `apply` |
| `stale` | template output changed | `apply` |
| `dangling` | links at us, but the source is gone | `apply` |
| `hijacked` | links somewhere else (detail shows where, and if it is broken) | `apply` |
| `clobbered` | a real file or directory is in the way | `apply -f` (backed up first) |
| `edited` | copy-mode file edited by hand | `apply -f` (backed up first) |
| `conflict` | template *and* file both changed | `apply -f` (backed up first) |
| `orphan` | applied once, no plan produces it now | `prune` |

Backups go to `~/.local/state/dot/backups/<timestamp>/`. State lives in
`~/.local/state/dot/state.json` and is a pure cache — losing it degrades
orphan detection, never correctness, and `apply` rebuilds it.

## Tests and checks

```sh
uv run pytest                                        # the suite
DOT_UPDATE_SNAPSHOTS=1 uv run pytest tests/test_plan.py   # after an intentional plan change

uv run ruff format .        # format
uv run ruff check . --fix   # lint
uv run basedpyright         # types
```

Ruff and basedpyright are configured in `pyproject.toml` — the same split nvim
runs over this repo, so the editor and the command line cannot disagree about
what counts as an error. `configs/*/module.py` is checked along with `src/`:
a module is code, and a typo in one is a broken machine.

Plans are data, so they snapshot per `(os, profile)`. The rest of the suite
applies the full plan into a throwaway `$HOME` and asserts idempotency plus
every drift state above. The CLI is driven through Typer's `CliRunner` and the
TUI headlessly through Textual's `run_test`, both against a fake home.

One rule the tests enforce directly: **no renderable may depend on the CLI's
Rich theme.** Textual draws Rich objects through its own console, which has no
theme, so `src/dot/theme.py` stores concrete style strings and
`tests/test_theme.py` renders every shared view through a themeless console.
