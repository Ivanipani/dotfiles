"""Plans are pure data, so they snapshot.

A diff here is the whole point: any change to a module, a template path or a
vars layer shows up as an explicit, reviewable change to the plan.
"""

from __future__ import annotations

import os
from pathlib import Path

import pytest
from conftest import make_host

from dot import reconcile

MATRIX = [
    ("macos", "personal"),
    ("macos", "work"),
    ("linux", "personal"),
]

SNAPSHOTS = Path(__file__).parent / "snapshots"


def normalize(text: str, home: Path, repo: Path) -> str:
    return text.replace(str(home), "$HOME").replace(str(repo), "$REPO")


def plan_text(mods, host) -> str:
    lines = []
    for item in reconcile.build(mods, host):
        lines.append(f"{item.module}\t{item.action.kind}\t{item.action.describe()}")
    return "\n".join(lines) + "\n"


@pytest.mark.parametrize("os_name,profile", MATRIX)
def test_plan_snapshot(repo, modules, tmp_path, os_name, profile):
    host = make_host(repo, tmp_path / "home", os_name=os_name, profile=profile)
    actual = normalize(plan_text(modules, host), host.home, repo)
    snap = SNAPSHOTS / f"plan-{os_name}-{profile}.txt"

    if os.environ.get("DOT_UPDATE_SNAPSHOTS"):
        snap.parent.mkdir(parents=True, exist_ok=True)
        snap.write_text(actual)
        pytest.skip(f"updated {snap.name}")

    assert snap.is_file(), f"missing snapshot {snap}; run DOT_UPDATE_SNAPSHOTS=1 pytest"
    assert actual == snap.read_text()


@pytest.mark.parametrize("os_name,profile", MATRIX)
def test_no_two_modules_claim_the_same_path(repo, modules, tmp_path, os_name, profile):
    host = make_host(repo, tmp_path / "home", os_name=os_name, profile=profile)
    assert reconcile.conflicts(reconcile.build(modules, host)) == []


@pytest.mark.parametrize("os_name,profile", MATRIX)
def test_plan_is_deterministic(repo, modules, tmp_path, os_name, profile):
    host = make_host(repo, tmp_path / "home", os_name=os_name, profile=profile)
    assert plan_text(modules, host) == plan_text(modules, host)


def test_linux_drops_the_gui_only_module(repo, modules, tmp_path):
    linux = make_host(repo, tmp_path / "l", os_name="linux")
    names = {i.module for i in reconcile.build(modules, linux)}
    assert "term" not in names
    assert {"zsh", "nvim", "vcs", "zellij", "nushell", "claude"} <= names


def test_module_ordering_puts_zsh_last(modules):
    assert modules[-1].name == "zsh"
