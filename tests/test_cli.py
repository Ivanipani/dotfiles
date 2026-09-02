"""End-to-end CLI behaviour: exit codes and the contract of each command."""

from __future__ import annotations

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
