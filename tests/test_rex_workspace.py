import os
import re
import subprocess
from pathlib import Path

import pytest
from conftest import REPO

SCRIPT = REPO / "configs" / "rex" / "scripts" / "rex-workspace"


@pytest.fixture
def env(tmp_path):
    roots = tmp_path / "roots"
    for name in ("alpha", "beta"):
        (roots / name).mkdir(parents=True)
        subprocess.run(["jj", "git", "init", str(roots / name)], check=True, capture_output=True)
    return {
        **os.environ,
        "REX_BIN": "/nonexistent",  # no Rex server: sessions are skipped
        "REX_WORKSPACES_DIR": str(tmp_path / "ws"),
        "REX_WORKSPACE_ROOTS": str(roots),
    }


def run(env, *args):
    return subprocess.run(["nu", str(SCRIPT), *args], env=env, capture_output=True, text=True)


def projects(env, set_name="demo"):
    text = (Path(env["REX_WORKSPACES_DIR"]) / set_name / "workspace.yaml").read_text()
    return re.findall(r"^- name: (.+)$", text, re.M)


def test_add_records_each_named_repo_in_the_manifest(env):
    # GIVEN two repos, WHEN both are added to a new set
    assert run(env, "add", "demo", "alpha", "beta").returncode == 0
    # THEN the manifest lists both
    assert projects(env) == ["alpha", "beta"]


def test_add_creates_a_jj_workspace_directory_per_repo(env):
    # GIVEN/WHEN
    run(env, "add", "demo", "alpha")
    # THEN the project directory is a jj workspace
    assert (Path(env["REX_WORKSPACES_DIR"]) / "demo" / "alpha" / ".jj").is_dir()


def test_add_links_claude_md_to_agents_md(env):
    # GIVEN/WHEN
    run(env, "add", "demo", "alpha")
    # THEN
    claude = Path(env["REX_WORKSPACES_DIR"]) / "demo" / "CLAUDE.md"
    assert os.readlink(claude) == "AGENTS.md"


def test_add_rejects_an_unknown_repo(env):
    # GIVEN/WHEN a repo that does not exist
    result = run(env, "add", "demo", "nope")
    # THEN the command fails naming it
    assert result.returncode != 0 and "nope" in result.stderr


def test_remove_drops_the_project_from_the_manifest(env):
    # GIVEN a set with two projects
    run(env, "add", "demo", "alpha", "beta")
    # WHEN one is removed
    run(env, "remove", "demo", "alpha")
    # THEN only the other remains
    assert projects(env) == ["beta"]


def test_remove_keeps_uncommitted_work_under_a_saved_bookmark(env, tmp_path):
    # GIVEN a project with an uncommitted file
    run(env, "add", "demo", "alpha")
    (Path(env["REX_WORKSPACES_DIR"]) / "demo" / "alpha" / "f.txt").write_text("x")
    # WHEN it is removed
    run(env, "remove", "demo", "alpha")
    # THEN the source repo has a saved-demo bookmark
    out = subprocess.run(["jj", "-R", str(tmp_path / "roots" / "alpha"), "bookmark", "list"], capture_output=True, text=True).stdout
    assert "saved-demo" in out


def test_delete_removes_the_set_directory(env):
    # GIVEN a set
    run(env, "add", "demo", "alpha")
    # WHEN it is deleted
    run(env, "delete", "demo", "--yes")
    # THEN nothing is left
    assert not (Path(env["REX_WORKSPACES_DIR"]) / "demo").exists()


def test_delete_without_confirmation_keeps_the_set(env):
    # GIVEN a set
    run(env, "add", "demo", "alpha")
    # WHEN delete is declined (no input)
    subprocess.run(["nu", str(SCRIPT), "delete", "demo"], env=env, input="n\n", capture_output=True, text=True)
    # THEN the set is untouched
    assert (Path(env["REX_WORKSPACES_DIR"]) / "demo").exists()
