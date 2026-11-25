"""Everything the CLI and the TUI both need to talk to a repo.

Kept free of typer and textual so neither front end depends on the other.
"""

from __future__ import annotations

import difflib
import os
from collections.abc import Iterable
from pathlib import Path

from . import reconcile
from .actions import Ctx, Render, Status
from .facts import detect_host
from .modules import Module, discover, select
from .render import make_env
from .state import State


def default_repo() -> Path:
    return Path(os.environ.get("DOT_REPO") or Path(__file__).resolve().parents[2])


def is_workspace(repo: Path) -> bool:
    """A secondary jj workspace or git worktree, not the canonical checkout."""
    return (repo / ".jj" / "repo").is_file() or (repo / ".git").is_file()


def setup(repo: Path, *, dry_run: bool = False, force: bool = False) -> tuple[Ctx, list[Module]]:
    host = detect_host(repo)
    mods = discover(repo)
    search = [p for p in [repo / "configs" / "templates", *(m.dir for m in mods)] if p.is_dir()]
    build_root = Path(os.environ.get("DOT_BUILD") or repo / ".build") / host.fingerprint
    state_dir = host.xdg_state / "dot"
    ctx = Ctx(
        host=host,
        state=State.load(state_dir / "state.json", host.fingerprint),
        env=make_env(host, search),
        build_root=build_root,
        backup_root=state_dir / "backups",
        dry_run=dry_run,
        force=force,
    )
    return ctx, mods


class UnknownModule(Exception):
    pass


def choose(mods: list[Module], names: Iterable[str] | None) -> list[Module]:
    try:
        return select(mods, names)
    except SystemExit as exc:
        raise UnknownModule(str(exc)) from None


def plan(ctx: Ctx, mods: list[Module], names: Iterable[str] | None = None):
    """Returns (chosen modules, items, conflict messages)."""
    chosen = choose(mods, names)
    items = reconcile.build(chosen, ctx.host)
    return chosen, items, reconcile.conflicts(items)


DIFFABLE = (Status.STALE, Status.EDITED, Status.CONFLICT)


def unified_diff(checked, ctx: Ctx, home: Path) -> str | None:
    """Unified diff for a pending rendered change; None when a diff makes no sense."""
    action = checked.action
    if not isinstance(action, Render) or checked.drift.status not in DIFFABLE:
        return None
    try:
        fresh = action.content(ctx.for_module(checked.module))
    except Exception as exc:
        return f"template error: {exc}"
    current = action.dst.read_text() if action.dst.is_file() else ""
    label = str(action.dst)
    if label.startswith(str(home) + "/"):
        label = "~" + label[len(str(home)) :]
    return "".join(
        difflib.unified_diff(
            current.splitlines(keepends=True),
            fresh.splitlines(keepends=True),
            fromfile=f"{label} (on disk)",
            tofile=f"{label} (rendered)",
        )
    )
