"""Plan -> drift -> apply. The only place side effects are allowed."""

from __future__ import annotations

from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path

from .actions import SEVERITY, Action, Ctx, Drift, Status
from .facts import Host
from .modules import Module
from .state import Entry


@dataclass
class Item:
    module: str
    action: Action

    @property
    def id(self) -> str:
        return f"{self.module}:{self.action.key()}"


@dataclass
class Checked:
    item: Item
    drift: Drift

    @property
    def module(self) -> str:
        return self.item.module

    @property
    def action(self) -> Action:
        return self.item.action


@dataclass
class Outcome:
    checked: Checked
    result: str  # applied | skipped | blocked | failed | unchanged
    detail: str = ""


def build(mods: Iterable[Module], host: Host) -> list[Item]:
    items: list[Item] = []
    for mod in mods:
        if not mod.enabled(host):
            continue
        for action in mod.plan(host):
            items.append(Item(mod.name, action))
    return items


def conflicts(items: Iterable[Item]) -> list[str]:
    """Two modules writing the same destination is always a bug."""
    owners: dict[str, list[str]] = {}
    for it in items:
        tgt = it.action.target()
        if tgt is None or it.action.kind == "dir":
            continue
        owners.setdefault(str(tgt), []).append(it.module)
    return [
        f"{path} is claimed by {', '.join(sorted(set(mods)))}"
        for path, mods in owners.items()
        if len(set(mods)) > 1
    ]


def check(items: Iterable[Item], ctx: Ctx) -> list[Checked]:
    out: list[Checked] = []
    for it in items:
        mctx = ctx.for_module(it.module)
        try:
            drift = it.action.check(mctx)
        except Exception as exc:
            drift = Drift(Status.ERROR, f"{type(exc).__name__}: {exc}")
        out.append(Checked(it, drift))
    return out


def orphans(items: Iterable[Item], ctx: Ctx, modules: set[str] | None = None) -> list[Entry]:
    live = {it.id for it in items}
    return ctx.state.orphans(live, modules)


def apply(
    checked: Iterable[Checked],
    ctx: Ctx,
    hook: Callable[[Checked], None] | None = None,
) -> list[Outcome]:
    """`hook` fires before each action so a caller can drive a progress bar."""
    results: list[Outcome] = []
    for c in checked:
        if hook is not None:
            hook(c)
        mctx = ctx.for_module(c.module)
        if c.drift.status is Status.ERROR:
            results.append(Outcome(c, "failed", c.drift.detail))
            continue
        if c.drift.ok:
            # Self-heal a lost or partial state file without touching disk.
            if not ctx.dry_run and ctx.state.get(c.module, c.action.key()) is None:
                ctx.state.record(c.action.entry(mctx))
            results.append(Outcome(c, "unchanged"))
            continue
        if c.drift.needs_force and not ctx.force:
            results.append(Outcome(c, "blocked", c.drift.detail))
            continue
        try:
            c.action.apply(mctx)
        except Exception as exc:
            results.append(Outcome(c, "failed", f"{type(exc).__name__}: {exc}"))
            continue
        if not ctx.dry_run:
            ctx.state.record(c.action.entry(mctx))
        results.append(Outcome(c, "applied", c.drift.detail))
    return results


def worst(checked: Iterable[Checked]) -> Status:
    statuses = [c.drift.status for c in checked]
    return max(statuses, key=lambda s: SEVERITY[s]) if statuses else Status.OK


def remove_orphans(entries: Iterable[Entry], ctx: Ctx) -> list[tuple[Entry, str]]:
    """Unwind by target path; the action that produced it may no longer exist."""
    out: list[tuple[Entry, str]] = []
    for e in entries:
        if not e.target:
            ctx.state.forget(e.module, e.key)
            out.append((e, "forgotten"))
            continue
        path = Path(e.target)
        try:
            if path.is_symlink():
                if not ctx.dry_run:
                    path.unlink()
                out.append((e, "unlinked"))
            elif e.kind in ("render", "fetch") and path.is_file():
                if not ctx.dry_run:
                    path.unlink()
                out.append((e, "deleted"))
            elif e.kind == "dir" and path.is_dir() and not any(path.iterdir()):
                if not ctx.dry_run:
                    path.rmdir()
                out.append((e, "rmdir"))
            else:
                out.append((e, "left in place"))
        except OSError as exc:
            out.append((e, f"failed: {exc}"))
            continue
        if not ctx.dry_run:
            ctx.state.forget(e.module, e.key)
    return out
