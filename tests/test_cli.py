"""End-to-end CLI behaviour: exit codes and the contract of each command."""

from __future__ import annotations

from pathlib import Path

import pytest
from typer.testing import CliRunner

from dot.cli import app

runner = CliRunner()


@pytest.fixture
def run(fake_env):
    def invoke(*args: str):
        return runner.invoke(app, list(args), env=fake_env)

    return invoke


def test_version(run):
    result = run("--version")
    assert result.exit_code == 0
    assert "dot" in result.output


def test_help_groups_the_commands(run):
    out = run("--help").output
    assert "Inspect" in out and "Change" in out
    for command in ("status", "diff", "apply", "adopt", "prune", "remove", "ui"):
        assert command in out


def test_facts_reports_the_injected_host(run):
    result = run("facts")
    assert result.exit_code == 0
    assert "testbox" in result.output
    assert "macos" in result.output


def test_plan_lists_actions_without_touching_disk(run, fake_env):
    from pathlib import Path

    result = run("plan")
    assert result.exit_code == 0
    assert "actions" in result.output
    assert not (Path(fake_env["HOME"]) / ".zshrc").exists()


def test_status_exits_nonzero_when_drifted(run):
    result = run("status")
    assert result.exit_code == 1


def test_apply_then_status_is_clean(run):
    assert run("apply").exit_code == 0
    assert run("status").exit_code == 0


def test_apply_dry_run_changes_nothing(run, fake_env):
    from pathlib import Path

    assert run("apply", "--dry-run").exit_code == 0
    assert not (Path(fake_env["HOME"]) / ".zshrc").exists()
    assert run("status").exit_code == 1


def test_diff_exits_nonzero_while_differences_remain(run):
    assert run("diff").exit_code == 1
    run("apply")
    result = run("diff")
    assert result.exit_code == 0
    assert "no differences" in result.output


def test_unknown_module_is_a_usage_error(run):
    result = run("status", "nope")
    assert result.exit_code == 2
    assert "unknown module" in result.output


def test_module_scoping(run, fake_env):
    from pathlib import Path

    assert run("apply", "vcs").exit_code == 0
    home = Path(fake_env["HOME"])
    assert (home / ".gitconfig").is_symlink()
    assert not (home / ".zshrc").exists()


def test_remove_requires_explicit_modules(run):
    assert run("remove").exit_code == 2


def test_prune_reports_nothing_on_a_fresh_apply(run):
    run("apply")
    result = run("prune")
    assert result.exit_code == 0
    assert "no orphans" in result.output


def test_adopt_moves_the_file_and_links_it_back(run, fake_env, tmp_path):
    from pathlib import Path

    stray = Path(fake_env["HOME"]) / "stray.conf"
    stray.write_text("mine\n")
    result = run("adopt", str(stray), "term", "--name", "adopted-test.conf")
    assert result.exit_code == 0
    assert stray.is_symlink()
    adopted = Path(fake_env["DOT_REPO"]) / "configs" / "term" / "adopted-test.conf"
    try:
        assert adopted.read_text() == "mine\n"
    finally:
        stray.unlink()
        adopted.unlink()


def test_adopt_refuses_an_existing_symlink(run, fake_env):
    from pathlib import Path

    link = Path(fake_env["HOME"]) / "already"
    link.symlink_to(Path(fake_env["DOT_REPO"]) / "README.md")
    result = run("adopt", str(link), "term")
    assert result.exit_code == 2
    assert "already a symlink" in result.output


def test_install_dry_run_prints_commands_without_running_them(run):
    result = run("install", "--dry-run", "--force", "vcs")
    assert result.exit_code == 0
    assert "brew install" in result.output or "cargo install" in result.output


PINNED = {"tilt": "0.37.7", "jjui": "0.10.9"}


def _probe_pinned(versions):
    return lambda binary: versions.get(Path(binary).name)


def test_doctor_passes_when_pinned_binaries_match(run, fake_env, monkeypatch):
    # GIVEN a linux host, where tilt and jjui are pinned, and matching installed versions
    fake_env["DOT_OS"] = "linux"
    monkeypatch.setattr("dot.cli.shutil.which", lambda binary: f"/fake/bin/{binary}")
    monkeypatch.setattr("dot.cli.probe", _probe_pinned(PINNED))

    # WHEN doctor checks the tools module
    result = run("doctor", "tools")

    # THEN it succeeds and shows the pinned version
    assert result.exit_code == 0
    assert "0.37.7" in result.output


def test_doctor_fails_when_pinned_binary_has_drifted(run, fake_env, monkeypatch):
    # GIVEN a linux host where tilt is installed at the wrong version
    fake_env["DOT_OS"] = "linux"
    monkeypatch.setattr("dot.cli.shutil.which", lambda binary: f"/fake/bin/{binary}")
    monkeypatch.setattr("dot.cli.probe", _probe_pinned({**PINNED, "tilt": "0.0.1"}))

    # WHEN doctor checks the tools module
    result = run("doctor", "tools")

    # THEN it reports exactly one outdated binary
    assert result.exit_code == 1
    assert "1 outdated" in result.output


def test_doctor_leaves_unpinned_binaries_unprobed(run, fake_env, monkeypatch):
    # GIVEN a linux host where only tilt and jjui are pinned
    fake_env["DOT_OS"] = "linux"
    monkeypatch.setattr("dot.cli.shutil.which", lambda binary: f"/fake/bin/{binary}")
    probed: list[str] = []
    monkeypatch.setattr(
        "dot.cli.probe", lambda binary: probed.append(Path(binary).name) or PINNED[Path(binary).name]
    )

    # WHEN doctor checks the tools module
    result = run("doctor", "tools")

    # THEN only the pinned binaries are probed
    assert result.exit_code == 0
    assert sorted(probed) == ["jjui", "tilt"]


@pytest.fixture
def workspace_repo(tmp_path, monkeypatch):
    repo = tmp_path / "workspace"
    (repo / ".jj").mkdir(parents=True)
    (repo / ".jj" / "repo").write_text("../../main/.jj/repo")
    monkeypatch.setattr("dot.cli.REPO", repo)
    return repo


def test_apply_from_workspace_without_force_is_refused(run, workspace_repo):
    # GIVEN dot runs from a secondary jj workspace
    # WHEN apply runs without --force
    result = run("apply")
    # THEN it refuses with a usage error
    assert result.exit_code == 2
    assert "workspace" in result.output
