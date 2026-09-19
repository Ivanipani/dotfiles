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

## Shared coding-agent configuration

`configs/agents/` owns the shared `AGENTS.md`, skills, and hook scripts.
The `claude` and `codex` modules use the existing `Symlink` and `Render`
actions, so they can be planned and applied together or independently:

```sh
dot plan claude codex
dot apply claude codex
```

- `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md` link to the same `AGENTS.md`.
- `~/.claude/skills` and `~/.agents/skills` link to the same skills directory.
- Both clients' `hooks/` directories link to the same scripts.
- Claude's `settings.json` and Codex's `hooks.json` render hook registrations
  from one template. Claude settings retain copy-mode protection for local edits;
  Codex's existing `config.toml` stays locally managed.
- Claude's subagent definitions remain installed only for Claude.

The jj context, command guard, and commit reminder run in both clients.
Codex maps permission requests, subagent completion, and turn completion to
shared desktop notifications and Zellij attention updates. Claude retains its
additional notification events (including idle and quota notifications).
See the [Codex hook reference](https://learn.chatgpt.com/docs/hooks) for event
coverage. After applying, review and trust the Codex hooks with `/hooks`.

Existing Claude links are repointed by `dot apply claude`; locally edited
settings are reported for review using the usual `dot diff` / `--force` workflow.
The `claude.auto_mode` local variable remains unchanged.

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
| `dot doctor [module]` | check the binaries each module declares in `requires`, and hold pinned ones to their version |
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

## Pinning a version

Every package helper takes an optional `version`, and each manager spells the
pin its own way:

```python
yield brew("node", version="20")  # brew install node@20
yield apt("jq", version="1.7")  # sudo apt install -y jq=1.7
yield uv_tool("ruff", version="0.16.5")  # uv tool install ruff==0.16.5
yield cargo("zellij", locked=True, version="0.43.1")
yield github("tilt-dev/tilt", "v0.37.7", ...)  # the tag is the pin
```

`rustup` has no `version`: `rustup component add` installs whatever the
toolchain carries, so a pin there would be a lie.

A pin is enforced, not just recorded. `dot install` asks a pinned binary for
its `--version` and reinstalls when the answer disagrees, so moving `TILT` in
`configs/tools/module.py` is enough to upgrade the machine. A partial pin is
satisfied by anything under it (`20` accepts `20.11.1`), and a binary that
prints no version it can parse is left alone — churning on every run is worse
than trusting a tool that will not answer.

`dot doctor` reports the same comparison instead of acting on it: a `version`
column showing what is installed, `0.44.0 ≠ 0.45.0` and a failed row when a pin
has drifted, `0.13.2 ?` when the binary will not say. Only pinned binaries are
probed, so doctor stays a cheap `which` for everything else.

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
pointed at, and `configs/agents/templates/settings.json.j2` renders it back out
of local vars, so the committed template stays free of it.

## Symlinked vs rendered

Both end up as symlinks in your home directory, but they behave differently
when you edit them:

- **Symlinked** (`configs/zsh/common/*.zsh`, `configs/nvim/config`,
  `configs/zellij/layouts`) —
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

## The zellij status bar

[zjstatus](https://github.com/dj95/zjstatus) owns both bottom rows: the existing
session/repository/system widgets and a custom keybind menu. The builtin
`tab-bar` stays above the panes; the official `status-bar` is replaced.
The custom `default` layout also applies to plain `zellij` launches. Existing
or resurrected sessions retain their saved layouts; start a fresh session
to pick up a layout change.

The `zjkeys` alias uses zjstatus's mode widget to show the actions for the
current mode. Session mode starts with `f Sessionizer`; rename and search
input modes show their own finish/cancel keys. Locked mode, the resting state, shows only `Ctrl a Unlock`; focus, tab and
quick-action hints appear in normal mode. Keys are blue ribbons and labels use
the shared Catppuccin Frappé palette. Very long mode menus can still clip
on narrow terminals; the primary actions come first.

The menu is authored alongside `keybinds` in
`configs/zellij/templates/config.kdl.j2`. It does not discover bindings:
update the hints when changing a shortcut. `zjstatus` owns the widget row;
`zjkeys` is a separate instance of the same binary with no command widgets.
Both aliases share the palette through a template macro.

```kdl
default_tab_template {
    pane size=1 borderless=true { plugin location="zellij:tab-bar" }
    children
    pane size=1 borderless=true { plugin location="zjstatus" }
    pane size=1 borderless=true { plugin location="zjkeys" }
}
```

Tabs then declare only their own panes, and a tab opened later gets the same
chrome: with no `new_tab_template`, zellij falls back to this one.

Every tab does get its own zjstatus instance, and there is no way around that:
zellij keys a plugin instance by pane (`plugin_map`, `(PluginId, ClientId)`)
and a pane belongs to exactly one tab, so nothing in a layout can point several
tabs at one instance. Two consequences are worth knowing about, and both are
handled below — the permission prompts are seeded away, and the widgets share
their work through a cache rather than each repeating it.

**Why that file is a template.** zjstatus runs widget commands directly, with
no shell — so `~` never expands and `PATH` is whatever the zellij server
happened to inherit. Rendering lets the plan write an absolute interpreter, an
absolute script path, and an explicit `PATH` built from the same vars the
shells use.

**Colours.** The Catppuccin Frappé palette is declared once in the template macro as `color_*` entries. zjstatus resolves `$name` against them anywhere a
format string is parsed — including the output of a widget script — so
`configs/zellij/scripts/zjstatus-*.nu` emit `#[fg=$red]` and contain no hex
codes. Retheming the bar is one block.

**The widgets** are nushell, symlinked onto `PATH` like every other script
here, and both print a line of zjstatus markup (`rendermode "dynamic"`):

| script | shows | notes |
| --- | --- | --- |
| `zjstatus-jj.nu` | change id, description, bookmark, distance above it, and conflict / divergent / empty / immutable flags | one `jj log` for all of it |
| `zjstatus-sys.nu` | cpu, memory and battery as a percentage of the whole machine | `top` on macOS, `/proc/stat` deltas on Linux |
| `zjstatus-cache.nu` | not a widget: the shared cache the other two go through | keeps the cost flat as tabs are added |

**One sample per interval, not one per tab.** Since every tab runs its own
instance, five tabs would otherwise mean five `top` samples every five seconds
— about a third of a core, permanently, and growing with each tab. So both
widgets go through `zjstatus-cache.nu`: the first instance to find the value
expired recomputes it, and every instance that ticks inside that window is
served what it wrote. The TTL sits just under each widget's interval, so
exactly one refresh happens per interval no matter how many tabs are open.

The claim on that refresh happens *before* the work rather than after: the
recomputing instance touches the file first, so instances waking at the same
moment see something fresh and serve the previous value instead of all
repeating the same sample. Verified with six concurrent instances against an
expired cache — one recomputed. The cache key carries whatever the value
depends on, which for the jj widget is the directory, so two tabs in different
repos never share an answer.

The jj widget runs `--ignore-working-copy`: a bar ticking every few seconds
must never take the working-copy lock or race `jjui`. The cost is that edits
appear only once something else snapshots. It follows `{focused_pane_cwd}`, so
it describes the repo you are looking at, and prints nothing outside a repo
(`hideonemptystdout`) rather than leaving a gap.

**Permissions** are granted ahead of time rather than prompted for. zjstatus
asks for `ReadApplicationState`, `ChangeApplicationState` and `RunCommands`
when it loads, and zellij only consults its cache *after* a plugin has asked —
so opening a layout on a machine with nothing cached puts a permission screen
in every tab at once, one per status bar, before the first answer is written
back. The plan writes those grants into the cache itself, from the table in
`configs/zellij/module.py`, and nothing ever asks.

That file belongs to zellij, which rewrites the whole of it from a hash map
every time a grant goes through: key order moves between runs and comments do
not survive. So the plan's `Grant` action checks what the file *says* rather
than what it looks like, and an apply merges into it instead of replacing it.

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
