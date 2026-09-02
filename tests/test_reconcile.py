"""Drift classification and the apply loop, exercised against a fake $HOME."""

from __future__ import annotations

import pytest
from conftest import make_ctx, make_host

from dot import reconcile
from dot.actions import Dir, Render, Run, Status, Symlink
from dot.state import Entry, State


@pytest.fixture
def world(repo, modules, tmp_path):
    """A full plan pointed at an empty home, applied once."""
    host = make_host(repo, tmp_path / "home")
    ctx = make_ctx(repo, host, modules, tmp_path / "run")
    items = reconcile.build(modules, host)
    return host, ctx, items


def statuses(items, ctx):
    return {c.drift.status for c in reconcile.check(items, ctx)}


# --------------------------------------------------------------------------- #
# The happy path


def test_apply_makes_status_clean(world):
    _, ctx, items = world
    assert statuses(items, ctx) != {Status.OK}

    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert statuses(items, ctx) == {Status.OK}


def test_apply_is_idempotent(world):
    _, ctx, items = world
    reconcile.apply(reconcile.check(items, ctx), ctx)
    second = reconcile.apply(reconcile.check(items, ctx), ctx)
    assert {r.result for r in second} == {"unchanged"}


def test_dry_run_touches_nothing(world):
    host, ctx, items = world
    ctx.dry_run = True
    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert not (host.home / ".zshrc").exists()
    assert not (host.home / ".gitconfig").exists()


# --------------------------------------------------------------------------- #
# Symlink drift


def test_absent_then_repaired(world, tmp_path):
    host, ctx, _ = world
    link = Symlink(
        host.repo / "configs" / "vcs" / "git" / "gitignore_global", host.home / ".gitignore_global"
    )
    assert link.check(ctx).status is Status.ABSENT
    link.apply(ctx)
    assert link.check(ctx).status is Status.OK


def test_hijacked_link_is_detected_and_repaired(world):
    host, ctx, _ = world
    src = host.repo / "configs" / "vcs" / "git" / "gitignore_global"
    dst = host.home / ".gitignore_global"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.symlink_to(host.repo / "README.md")

    link = Symlink(src, dst)
    drift = link.check(ctx)
    assert drift.status is Status.HIJACKED
    assert "README.md" in drift.detail
    assert not drift.needs_force  # replacing a link loses nothing

    link.apply(ctx)
    assert link.check(ctx).status is Status.OK


def test_broken_link_to_a_moved_repo_is_flagged(world):
    """The exact failure this tool was written for."""
    host, ctx, _ = world
    dst = host.home / "starship.toml"
    dst.symlink_to("/nowhere/old-repo/configs/zsh/starship.toml")

    drift = Symlink(host.repo / "configs" / "zsh" / "starship.toml", dst).check(ctx)
    assert drift.status is Status.HIJACKED
    assert "(broken)" in drift.detail


def test_dangling_when_the_source_disappears(world, tmp_path):
    host, ctx, _ = world
    src = tmp_path / "gone.conf"
    src.write_text("x")
    dst = host.home / "gone.conf"
    link = Symlink(src, dst)
    link.apply(ctx)
    assert link.check(ctx).status is Status.OK

    src.unlink()
    drift = link.check(ctx)
    assert drift.status is Status.DANGLING


def test_real_file_in_the_way_needs_force(world):
    host, ctx, _ = world
    dst = host.home / ".gitignore_global"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text("hand written, must not vanish\n")

    link = Symlink(host.repo / "configs" / "vcs" / "git" / "gitignore_global", dst)
    drift = link.check(ctx)
    assert drift.status is Status.CLOBBERED
    assert drift.needs_force

    blocked = reconcile.apply([reconcile.Checked(reconcile.Item("vcs", link), drift)], ctx)
    assert blocked[0].result == "blocked"
    assert dst.read_text() == "hand written, must not vanish\n"

    ctx.force = True
    forced = reconcile.apply(reconcile.check([reconcile.Item("vcs", link)], ctx), ctx)
    assert forced[0].result == "applied"
    assert dst.is_symlink()
    backups = list(ctx.backup_root.rglob(".gitignore_global"))
    assert backups and backups[0].read_text() == "hand written, must not vanish\n"


# --------------------------------------------------------------------------- #
# Rendered content


@pytest.fixture
def tpl(tmp_path):
    p = tmp_path / "greeting.j2"
    p.write_text("hello {{ v.user.name }}\n")
    return p


def test_render_symlink_mode_tracks_the_template(world, tpl):
    host, ctx, _ = world
    ctx.module = "demo"
    action = Render(tpl, host.home / "greeting")
    assert action.check(ctx).status is Status.ABSENT

    action.apply(ctx)
    assert action.check(ctx).status is Status.OK
    assert (host.home / "greeting").read_text() == "hello Ivan Perdomo\n"

    tpl.write_text("hello again {{ v.user.name }}\n")
    assert action.check(ctx).status is Status.STALE
    action.apply(ctx)
    assert action.check(ctx).status is Status.OK


def test_render_copy_mode_distinguishes_edit_from_update(world, tpl):
    host, ctx, _ = world
    ctx.module = "demo"
    dst = host.home / "greeting"
    action = Render(tpl, dst, mode="copy")

    action.apply(ctx)
    ctx.state.record(action.entry(ctx))
    assert action.check(ctx).status is Status.OK

    # Template moved on, file untouched -> safe update.
    tpl.write_text("v2 {{ v.user.name }}\n")
    assert action.check(ctx).status is Status.STALE

    # File edited by hand, template back where it was -> refuse.
    tpl.write_text("hello {{ v.user.name }}\n")
    dst.write_text("I edited this myself\n")
    drift = action.check(ctx)
    assert drift.status is Status.EDITED
    assert drift.needs_force

    # Both moved -> conflict.
    tpl.write_text("v3 {{ v.user.name }}\n")
    drift = action.check(ctx)
    assert drift.status is Status.CONFLICT
    assert drift.needs_force


def test_untracked_destination_is_not_silently_overwritten(world, tpl):
    host, ctx, _ = world
    ctx.module = "demo"
    dst = host.home / "greeting"
    dst.write_text("pre-existing\n")
    drift = Render(tpl, dst, mode="copy").check(ctx)
    assert drift.status is Status.EDITED
    assert drift.needs_force


def test_template_error_is_reported_not_raised(world, tmp_path):
    host, ctx, _ = world
    ctx.module = "demo"
    bad = tmp_path / "bad.j2"
    bad.write_text("{{ v.nope.missing }}\n")
    drift = Render(bad, host.home / "out").check(ctx)
    assert drift.status is Status.ERROR


def test_a_template_error_does_not_stop_the_run(world, tmp_path):
    host, ctx, _ = world
    bad = tmp_path / "bad.j2"
    bad.write_text("{{ v.nope.missing }}\n")
    good = Symlink(host.repo / "README.md", host.home / "README.md")
    items = [
        reconcile.Item("demo", Render(bad, host.home / "out")),
        reconcile.Item("demo", good),
    ]
    results = reconcile.apply(reconcile.check(items, ctx), ctx)
    assert [r.result for r in results] == ["failed", "applied"]


# --------------------------------------------------------------------------- #
# Directories, Run, orphans, removal


def test_dir_refuses_to_eat_a_file(world):
    host, ctx, _ = world
    p = host.home / "notadir"
    p.write_text("x")
    assert Dir(p).check(ctx).status is Status.CLOBBERED


def test_run_without_a_predicate_is_an_error(world):
    _, ctx, _ = world
    assert Run(("true",)).check(ctx).status is Status.ERROR


def test_run_is_satisfied_by_its_marker(world):
    host, ctx, _ = world
    marker = host.home / "cloned"
    action = Run(("false",), unless=marker, desc="clone something")
    assert action.check(ctx).status is Status.ABSENT
    marker.mkdir()
    assert action.check(ctx).status is Status.OK


def test_orphans_are_found_and_pruned(world):
    host, ctx, items = world
    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert reconcile.orphans(items, ctx) == []

    stray = host.home / ".config" / "leftover.conf"
    stray.parent.mkdir(parents=True, exist_ok=True)
    stray.symlink_to(host.repo / "README.md")
    ctx.state.record(Entry(module="vcs", kind="symlink", key=f"symlink:{stray}", target=str(stray)))

    found = reconcile.orphans(items, ctx)
    assert [e.target for e in found] == [str(stray)]

    reconcile.remove_orphans(found, ctx)
    assert not stray.is_symlink()
    assert reconcile.orphans(items, ctx) == []


def test_prune_leaves_files_it_did_not_create(world):
    host, ctx, _items = world
    real = host.home / "precious.txt"
    real.write_text("keep me")
    entry = Entry(module="vcs", kind="symlink", key="symlink:precious", target=str(real))
    ctx.state.record(entry)
    results = reconcile.remove_orphans([entry], ctx)
    assert results[0][1] == "left in place"
    assert real.read_text() == "keep me"


def test_remove_unwinds_a_module(world):
    host, ctx, items = world
    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert (host.home / ".gitconfig").is_symlink()

    vcs_items = [i for i in items if i.module == "vcs"]
    for item in reversed(vcs_items):
        item.action.remove(ctx.for_module("vcs"))
    assert not (host.home / ".gitconfig").is_symlink()
    assert (host.home / ".zshrc").is_symlink()  # other modules untouched


def test_state_self_heals_when_the_file_is_lost(world, tmp_path):
    host, ctx, items = world
    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert ctx.state.entries

    ctx.state = State(tmp_path / "fresh.json", host.fingerprint)
    reconcile.apply(reconcile.check(items, ctx), ctx)
    assert len(ctx.state.entries) == len(items)
    assert reconcile.orphans(items, ctx) == []


def test_state_round_trips(tmp_path):
    s = State(tmp_path / "s.json", "fp")
    s.record(Entry(module="m", kind="symlink", key="symlink:/x", target="/x", hash="abc"))
    s.save()
    again = State.load(tmp_path / "s.json")
    assert again.fingerprint == "fp"
    entry = again.get("m", "symlink:/x")
    assert entry is not None
    assert entry.hash == "abc"


def test_corrupt_state_degrades_quietly(tmp_path):
    p = tmp_path / "s.json"
    p.write_text("{ this is not json")
    assert State.load(p).entries == {}


def test_remove_reports_what_it_actually_did(world):
    _host, ctx, items = world
    reconcile.apply(reconcile.check(items, ctx), ctx)

    claude = [i for i in items if i.module == "claude"]
    mctx = ctx.for_module("claude")
    outcomes = [i.action.remove(mctx) for i in reversed(claude)]
    # Reverse order: every file the module owns goes first, so the directory
    # it lived in is empty by the time we reach it.
    assert outcomes[-1] == "rmdir"
    assert set(outcomes[:-1]) <= {"unlinked", "deleted"}
    assert "left in place" not in outcomes


def test_remove_will_not_claim_a_path_it_left_alone(world):
    host, ctx, _ = world
    foreign = host.home / "not-ours"
    foreign.symlink_to(host.repo / "README.md")
    assert Symlink(host.repo / "justfile", foreign).remove(ctx) == "left in place"
    assert foreign.is_symlink()


def test_remove_leaves_a_non_empty_directory(world):
    host, ctx, _ = world
    d = host.home / "keep"
    d.mkdir()
    (d / "child").write_text("x")
    assert Dir(d).remove(ctx) == "left in place"
    assert d.is_dir()
