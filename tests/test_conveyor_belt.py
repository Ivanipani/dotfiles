"""The conveyor-belt module: the repos list of the cb-guard hook, and cb's default context."""

from __future__ import annotations

from conftest import make_ctx, make_host

from dot import reconcile
from dot.modules import select


def _host(repo, tmp_path, profile):
    return make_host(repo, tmp_path / "home", profile=profile)


def _render(repo, modules, tmp_path, module, dst, profile="personal"):
    """The content `module` renders to `dst` (relative to the config directory)."""
    host = _host(repo, tmp_path, profile)
    mods = select(modules, [module])
    ctx = make_ctx(repo, host, mods, tmp_path)
    target = host.xdg_config / dst
    action = next(i.action for i in reconcile.build(mods, host) if i.action.target() == target)
    return action.content(ctx)


def test_personal_profile_lists_the_repos_the_belt_owns(repo, modules, tmp_path):
    # GIVEN the personal profile WHEN the repos file renders
    content = _render(repo, modules, tmp_path, "conveyor-belt", "conveyor-belt/repos")
    # THEN each repo is on a line of its own
    assert "Ivanipani/edifice" in content.splitlines()


def test_personal_profile_enrols_the_deploy_repo_in_the_belt(repo, modules, tmp_path):
    # GIVEN the personal profile WHEN the repos file renders
    content = _render(repo, modules, tmp_path, "conveyor-belt", "conveyor-belt/repos")
    # THEN the deploy repo is on a line of its own
    assert "Ivanipani/edifice-deploy-configs" in content.splitlines()


def test_profile_without_repos_plans_nothing_for_conveyor_belt(repo, modules, tmp_path):
    # GIVEN the work profile, which enrols no repo WHEN the plan is built
    host = _host(repo, tmp_path, "work")
    plan = reconcile.build(modules, host)
    # THEN the module yields no action
    assert [i for i in plan if i.module == "conveyor-belt"] == []


def test_zsh_exports_the_default_cb_context(repo, modules, tmp_path):
    # GIVEN the personal profile WHEN the zsh env renders
    content = _render(repo, modules, tmp_path, "zsh", "zsh/01-env.zsh")
    # THEN cb defaults to the homelab context
    assert 'export CB_CONTEXT="doghouse"' in content


def test_zsh_leaves_cb_context_unset_without_one(repo, modules, tmp_path):
    # GIVEN the work profile, which names no context WHEN the zsh env renders
    content = _render(repo, modules, tmp_path, "zsh", "zsh/01-env.zsh", profile="work")
    # THEN nothing is exported
    assert "CB_CONTEXT" not in content
