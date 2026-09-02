"""Module base class and discovery."""

from __future__ import annotations

import importlib.util
import inspect
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path

from .actions import Action
from .facts import Host
from .packages import Package

MODULE_FILE = "module.py"
CONFIGS_DIR = "configs"  # every module lives in its own subdirectory here


class Module:
    """One configurable subsystem, living next to the config it owns."""

    name: str = ""
    requires: tuple[str, ...] = ()  # binaries `dot doctor` warns about
    after: tuple[str, ...] = ()  # modules that must be applied first

    def __init__(self, dir: Path) -> None:
        self.dir = dir
        if not self.name:
            self.name = dir.name

    def enabled(self, host: Host) -> bool:
        return True

    def packages(self, host: Host) -> Iterable[Package]:
        return ()

    def plan(self, host: Host) -> Iterable[Action]:
        return ()

    # Convenience for module authors.
    def path(self, *parts: str) -> Path:
        return self.dir.joinpath(*parts)

    def __repr__(self) -> str:
        return f"<Module {self.name}>"


def _load_file(path: Path) -> list[Module]:
    mod_name = f"dot_modules.{path.parent.name}"
    spec = importlib.util.spec_from_file_location(mod_name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = module
    spec.loader.exec_module(module)

    found = []
    for obj in vars(module).values():
        if not (inspect.isclass(obj) and issubclass(obj, Module) and obj is not Module):
            continue
        # __module__ ignores classes merely imported into this namespace.
        if obj.__module__ == mod_name:
            found.append(obj(path.parent))
    if not found:
        raise ImportError(f"{path} defines no Module subclass")
    return found


def discover(repo: Path) -> list[Module]:
    root = repo / CONFIGS_DIR
    mods: list[Module] = []
    for entry in sorted(root.iterdir()) if root.is_dir() else ():
        if not entry.is_dir() or entry.name.startswith("."):
            continue
        candidate = entry / MODULE_FILE
        if candidate.is_file():
            mods.extend(_load_file(candidate))
    return order(mods)


def order(mods: Iterable[Module]) -> list[Module]:
    """Topological sort on `after`; ties resolve alphabetically for stability."""
    pending = {m.name: m for m in mods}
    done: list[Module] = []
    placed: set[str] = set()
    while pending:
        ready = sorted(
            (m for m in pending.values() if all(d in placed or d not in pending for d in m.after)),
            key=lambda m: m.name,
        )
        if not ready:
            cycle = ", ".join(sorted(pending))
            raise ValueError(f"cycle in module ordering: {cycle}")
        for m in ready:
            done.append(m)
            placed.add(m.name)
            del pending[m.name]
    return done


def select(mods: Sequence[Module], names: Iterable[str] | None) -> list[Module]:
    if not names:
        return list(mods)
    wanted = set(names)
    known = {m.name for m in mods}
    missing = wanted - known
    if missing:
        raise SystemExit(f"unknown module(s): {', '.join(sorted(missing))}")
    return [m for m in mods if m.name in wanted]
