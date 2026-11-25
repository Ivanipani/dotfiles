"""Command line surface, built on Typer."""

from __future__ import annotations

import os
import shutil
from collections import Counter
from pathlib import Path
from typing import Annotated

import typer
from rich.progress import BarColumn, MofNCompleteColumn, Progress, SpinnerColumn, TextColumn

from . import __version__, reconcile, session, ui
from .actions import Ctx, Status
from .modules import Module
from .packages import install as install_package
from .packages import probe, satisfies
from .session import UnknownModule
from .ui import console, err_console

REPO = session.default_repo()

INSPECT = "Inspect"
CHANGE = "Change"

app = typer.Typer(
    name="dot",
    help="Code-first dotfiles engine. Modules describe state; the engine reconciles it.",
    no_args_is_help=True,
    rich_markup_mode="rich",
    pretty_exceptions_show_locals=False,
    context_settings={"help_option_names": ["-h", "--help"]},
)

# --------------------------------------------------------------------------- #
# Shared option types

Modules = Annotated[
    list[str] | None,
    typer.Argument(metavar="[MODULE]...", help="Limit to these modules.", show_default=False),
]
DryRun = Annotated[bool, typer.Option("--dry-run", "-n", help="Change nothing; just report.")]
Force = Annotated[
    bool,
    typer.Option(
        "--force", "-f", help="Overwrite unmanaged or hand-edited files (backed up first)."
    ),
]
ShowAll = Annotated[
    bool, typer.Option("--all", "-a", help="Include entries that are already correct.")
]


# --------------------------------------------------------------------------- #
# Wiring


def _setup(*, dry_run: bool = False, force: bool = False) -> tuple[Ctx, list[Module]]:
    return session.setup(REPO, dry_run=dry_run, force=force)


def _select(mods: list[Module], names: list[str] | None) -> list[Module]:
    try:
        return session.choose(mods, names)
    except UnknownModule as exc:
        err_console.print(f"[error]✖[/] {exc}")
        raise typer.Exit(2) from None


def _plan(ctx: Ctx, mods: list[Module], names: list[str] | None):
    try:
        chosen, items, problems = session.plan(ctx, mods, names)
    except UnknownModule as exc:
        err_console.print(f"[error]✖[/] {exc}")
        raise typer.Exit(2) from None
    for problem in problems:
        err_console.print(f"[error]✖ conflict[/] {problem}")
    return chosen, items


def _version(value: bool) -> None:
    if value:
        console.print(f"dot {__version__}")
        raise typer.Exit()


@app.callback()
def root(
    version: Annotated[
        bool,
        typer.Option("--version", "-V", callback=_version, is_eager=True, help="Show the version."),
    ] = False,
) -> None:
    pass


# --------------------------------------------------------------------------- #
# Inspect


@app.command(rich_help_panel=INSPECT)
def facts() -> None:
    """Show the resolved host and the merged vars every template sees."""
    ctx, mods = _setup()
    console.print(ui.banner(ctx.host, "facts"))
    enabled = [m.name for m in mods if m.enabled(ctx.host)]
    disabled = [m.name for m in mods if not m.enabled(ctx.host)]
    console.print(ui.facts_view(ctx.host, enabled, disabled))


@app.command(rich_help_panel=INSPECT)
def doctor(modules: Modules = None) -> None:
    """Check the binaries each module declares, and hold pinned ones to their version."""
    ctx, mods = _setup()
    console.print(ui.banner(ctx.host, "doctor"))
    enabled = [mod for mod in _select(mods, modules) if mod.enabled(ctx.host)]
    # Only pinned binaries are probed: doctor stays a cheap `which` for the rest.
    pins = {
        pkg.provides: pkg.version
        for mod in enabled
        for pkg in mod.packages(ctx.host)
        if pkg.version
    }
    rows: list[tuple[str, str, str | None, str | None, str | None]] = []
    missing = outdated = 0
    for mod in enabled:
        for binary in mod.requires:
            found = shutil.which(binary)
            want = pins.get(binary)
            got = probe(found) if found and want else None
            missing += not found
            outdated += bool(want and got and not satisfies(got, want))
            rows.append((mod.name, binary, found, want, got))
    console.print(ui.doctor_table(rows))
    trouble = [
        label
        for count, label in ((missing, f"{missing} missing"), (outdated, f"{outdated} outdated"))
        if count
    ]
    if trouble:
        console.print(ui.hint(f"{', '.join(trouble)} — try [head]dot install[/head]"))
        raise typer.Exit(1)
    console.print(ui.hint("all required binaries present"))


@app.command(rich_help_panel=INSPECT)
def plan(modules: Modules = None) -> None:
    """List the actions each module produces, without touching the machine."""
    ctx, mods = _setup()
    _, items = _plan(ctx, mods, modules)
    console.print(ui.banner(ctx.host, "plan"))
    console.print(ui.plan_tree(items, ctx.host))
    console.print(ui.hint(f"{len(items)} actions"))


@app.command(rich_help_panel=INSPECT)
def status(modules: Modules = None, all: ShowAll = False) -> None:
    """Report how the machine differs from this repo."""
    ctx, mods = _setup()
    chosen, items = _plan(ctx, mods, modules)
    checked = reconcile.check(items, ctx)

    rows, counts = [], Counter()
    for c in checked:
        counts[c.drift.status] += 1
        if c.drift.ok and not all:
            continue
        target = str(c.action.target() or c.action.describe())
        rows.append((c.module, c.drift.status, target, c.drift.detail))

    scope = {m.name for m in chosen} if modules else None
    for entry in reconcile.orphans(items, ctx, scope):
        counts[Status.ORPHAN] += 1
        rows.append(
            (entry.module, Status.ORPHAN, entry.target or entry.key, "no longer in the plan")
        )

    console.print(ui.banner(ctx.host, "status"))
    if rows:
        console.print(ui.drift_table(sorted(rows, key=lambda r: r[0]), ctx.host))
    console.print(ui.summary_bar(counts))
    drifted = sum(n for s, n in counts.items() if s is not Status.OK)
    if drifted:
        console.print(ui.hint("dot diff to inspect · dot apply to fix · dot ui to browse"))
        raise typer.Exit(1)


@app.command(rich_help_panel=INSPECT)
def diff(modules: Modules = None) -> None:
    """Show, file by file, exactly what apply would change."""
    ctx, mods = _setup()
    _, items = _plan(ctx, mods, modules)
    console.print(ui.banner(ctx.host, "diff"))

    shown = 0
    for c in reconcile.check(items, ctx):
        if c.drift.ok:
            continue
        shown += 1
        action = c.action
        label = ui.shorten(str(action.target() or action.key()), ctx.host)
        body = session.unified_diff(c, ctx, ctx.host.home)
        if body is None:
            console.print(ui.note_panel(c.module, label, c.drift.status, c.drift.detail))
        else:
            console.print(ui.diff_panel(c.module, label, c.drift.status, body))
    if not shown:
        console.print(ui.hint("no differences"))
        return
    raise typer.Exit(1)


# --------------------------------------------------------------------------- #
# Change


@app.command(rich_help_panel=CHANGE)
def apply(
    modules: Modules = None,
    dry_run: DryRun = False,
    force: Force = False,
    all: ShowAll = False,
) -> None:
    """Reconcile the machine with this repo."""
    if session.is_workspace(REPO) and not (force or dry_run):
        err_console.print(
            f"[error]✖[/] {REPO} is a workspace, not the main checkout: applying would "
            "point your dotfiles here. Run from the main checkout, or pass --force."
        )
        raise typer.Exit(2)
    ctx, mods = _setup(dry_run=dry_run, force=force)
    chosen, items = _plan(ctx, mods, modules)
    checked = reconcile.check(items, ctx)
    todo = [c for c in checked if not c.drift.ok]

    console.print(ui.banner(ctx.host, "apply (dry run)" if dry_run else "apply"))

    if todo and console.is_terminal:
        with Progress(
            SpinnerColumn(style="rule"),
            TextColumn("[head]{task.description}"),
            BarColumn(complete_style="ok", finished_style="ok"),
            MofNCompleteColumn(),
            console=console,
            transient=True,
        ) as progress:
            task = progress.add_task("reconciling", total=len(checked))

            def tick(c) -> None:
                progress.update(task, advance=1, description=c.module)

            results = reconcile.apply(checked, ctx, hook=tick)
    else:
        results = reconcile.apply(checked, ctx)

    rows, counts = [], Counter()
    for r in results:
        counts[r.result] += 1
        if r.result == "unchanged" and not all:
            continue
        target = str(r.checked.action.target() or r.checked.action.describe())
        rows.append((r.checked.module, r.result, target, r.detail))
    if rows:
        console.print(ui.apply_table(rows, ctx.host))

    scope = {m.name for m in chosen} if modules else None
    stragglers = reconcile.orphans(items, ctx, scope)
    if stragglers:
        console.print(ui.hint(f"{len(stragglers)} orphan(s) — run [head]dot prune[/head]"))

    if ctx.state.dirty and not dry_run:
        ctx.state.save()

    console.print(ui.result_bar(counts))
    if counts["blocked"]:
        console.print(ui.hint("blocked entries need --force; the originals are backed up"))
    if counts["failed"] or counts["blocked"]:
        raise typer.Exit(1)


@app.command(rich_help_panel=CHANGE)
def install(
    modules: Modules = None,
    dry_run: DryRun = False,
    force: Force = False,
    keep_going: Annotated[
        bool, typer.Option("--keep-going", "-k", help="Continue after a failed install.")
    ] = False,
) -> None:
    """Install the packages modules declare. Slow and networked, so kept out of apply."""
    ctx, mods = _setup(dry_run=dry_run, force=force)
    pending = [
        (mod.name, pkg)
        for mod in _select(mods, modules)
        if mod.enabled(ctx.host)
        for pkg in mod.packages(ctx.host)
        if force or not pkg.installed()
    ]
    console.print(ui.banner(ctx.host, "install"))
    if not pending:
        console.print(ui.hint("all declared packages are present"))
        return
    for name, pkg in pending:
        console.print(f"[module]{name}[/module] [hint]$[/hint] {pkg.describe()}")
        if dry_run:
            continue
        code = install_package(pkg)
        if code != 0:
            err_console.print(f"[failed]✖ failed ({code})[/] {pkg.describe()}")
            if not keep_going:
                raise typer.Exit(code)


@app.command(rich_help_panel=CHANGE)
def adopt(
    path: Annotated[Path, typer.Argument(help="The file or directory to bring under management.")],
    module: Annotated[str, typer.Argument(help="Module that should own it.")],
    name: Annotated[str | None, typer.Option(help="Name inside the module directory.")] = None,
    dry_run: DryRun = False,
) -> None:
    """Move an existing file into a module and link it back where it was."""
    ctx, mods = _setup(dry_run=dry_run)
    # Deliberately not .resolve(): resolving would follow a symlink and adopt
    # whatever it points at, which is never what the caller meant.
    target = path.expanduser()
    if not target.is_absolute():
        target = Path.cwd() / target
    target = Path(os.path.normpath(target))
    mod = next((m for m in mods if m.name == module), None)
    if mod is None:
        err_console.print(f"[error]✖[/] unknown module: {module}")
        raise typer.Exit(2)
    if not target.exists():
        err_console.print(f"[error]✖[/] no such path: {target}")
        raise typer.Exit(2)
    if target.is_symlink():
        err_console.print(f"[error]✖[/] {target} is already a symlink")
        raise typer.Exit(2)

    dest = mod.dir / (name or target.name)
    if dest.exists():
        err_console.print(f"[error]✖[/] {dest} already exists")
        raise typer.Exit(2)
    if not dry_run:
        shutil.move(str(target), str(dest))
        target.symlink_to(dest)

    verb = "would adopt" if dry_run else "adopted"
    console.print(f"[ok]✔[/] {verb} {ui.shorten(target, ctx.host)} [hint]→[/hint] {dest}")
    console.print(ui.hint(f"add this to {mod.dir / 'module.py'}:"))
    console.print(
        f'    yield Symlink(self.path("{dest.relative_to(mod.dir)}"), '
        f"{_home_expr(target, ctx.host.home)})"
    )


def _home_expr(path: Path, home: Path) -> str:
    try:
        return f'host.home / "{path.relative_to(home)}"'
    except ValueError:
        return f'Path("{path}")'


@app.command(rich_help_panel=CHANGE)
def prune(modules: Modules = None, dry_run: DryRun = False) -> None:
    """Remove entries that were applied once but no plan produces any more."""
    ctx, mods = _setup(dry_run=dry_run)
    chosen, items = _plan(ctx, mods, modules)
    scope = {m.name for m in chosen} if modules else None
    stragglers = reconcile.orphans(items, ctx, scope)
    console.print(ui.banner(ctx.host, "prune (dry run)" if dry_run else "prune"))
    if not stragglers:
        console.print(ui.hint("no orphans"))
        return
    rows = [
        (e.module, what, e.target or e.key, "")
        for e, what in reconcile.remove_orphans(stragglers, ctx)
    ]
    console.print(ui.apply_table(rows, ctx.host))
    if ctx.state.dirty and not dry_run:
        ctx.state.save()


@app.command(rich_help_panel=CHANGE)
def remove(
    modules: Annotated[list[str], typer.Argument(help="Modules to unwind.")],
    dry_run: DryRun = False,
) -> None:
    """Unwind the named modules, undoing what apply put in place."""
    ctx, mods = _setup(dry_run=dry_run)
    chosen = _select(mods, modules)
    console.print(ui.banner(ctx.host, "remove (dry run)" if dry_run else "remove"))
    rows = []
    for item in reversed(reconcile.build(chosen, ctx.host)):
        mctx = ctx.for_module(item.module)
        try:
            what = item.action.remove(mctx)
        except OSError as exc:
            rows.append((item.module, "failed", str(item.action.target() or ""), str(exc)))
            continue
        rows.append((item.module, what, str(item.action.target() or item.action.describe()), ""))
        if not dry_run and what != "left in place":
            ctx.state.forget(item.module, item.action.key())
    console.print(ui.apply_table(rows, ctx.host))
    if ctx.state.dirty and not dry_run:
        ctx.state.save()


# --------------------------------------------------------------------------- #
# Interactive


@app.command(name="ui", rich_help_panel=INSPECT)
def tui() -> None:
    """Browse drift, read diffs and apply modules in a full-screen TUI."""
    from .tui import run  # lazy: importing textual costs ~200ms

    run(REPO)


def main() -> None:
    app(prog_name="dot")


if __name__ == "__main__":
    main()
