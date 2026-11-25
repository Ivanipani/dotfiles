"""Repo checkout detection."""

from __future__ import annotations

from dot.session import is_workspace


def test_jj_secondary_workspace_is_a_workspace(tmp_path):
    # GIVEN a checkout whose .jj/repo is a pointer file
    (tmp_path / ".jj").mkdir()
    (tmp_path / ".jj" / "repo").write_text("../main/.jj/repo")
    # WHEN / THEN it is detected as a workspace
    assert is_workspace(tmp_path)


def test_git_worktree_is_a_workspace(tmp_path):
    # GIVEN a checkout whose .git is a pointer file
    (tmp_path / ".git").write_text("gitdir: ../main/.git/worktrees/x")
    # WHEN / THEN it is detected as a workspace
    assert is_workspace(tmp_path)


def test_main_jj_checkout_is_not_a_workspace(tmp_path):
    # GIVEN a checkout that owns its .jj/repo directory
    (tmp_path / ".jj" / "repo").mkdir(parents=True)
    # WHEN / THEN it is not a workspace
    assert not is_workspace(tmp_path)
