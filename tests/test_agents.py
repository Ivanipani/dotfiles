"""Shared agent configuration and hook behavior."""

import json
import os
import subprocess

import pytest

from dot import reconcile
from dot.actions import Status


def test_shared_agent_install(host, ctx, modules):
    selected = [m for m in modules if m.name in {"claude", "codex"}]
    items = reconcile.build(selected, host)
    reconcile.apply(reconcile.check(items, ctx), ctx)
    home = host.home
    shared = host.repo / "configs/agents"
    for path in (".claude/CLAUDE.md", ".codex/AGENTS.md"):
        assert (home / path).resolve() == shared / "AGENTS.md"
    for path in (".claude/skills", ".agents/skills"):
        assert (home / path).resolve() == shared / "skills"
    for client in ("claude", "codex"):
        assert (home / f".{client}/hooks").resolve() == shared / "hooks"
    claude = json.loads((home / ".claude/settings.json").read_text())
    codex = json.loads((home / ".codex/hooks.json").read_text())
    assert not (home / ".claude/settings.json").is_symlink()
    assert (home / ".codex/hooks.json").is_symlink()
    assert not (home / ".codex/config.toml").exists()
    for event in ("SessionStart", "PreToolUse"):
        assert str(claude["hooks"][event]).replace(".claude", ".codex") == str(
            codex["hooks"][event]
        )
    assert "Notification" not in codex["hooks"]
    assert {"PermissionRequest", "SubagentStop", "Stop"} <= codex["hooks"].keys()
    for config in (claude, codex):
        for groups in config["hooks"].values():
            for group in groups:
                for hook in group["hooks"]:
                    command = hook["command"].replace("$HOME", str(home))
                    assert os.path.isfile(command.split('"')[1])
    assert all(c.drift.status == Status.OK for c in reconcile.check(items, ctx))


@pytest.mark.parametrize(
    "command,blocked", [("git status", True), ("jj git fetch", False), ("jj status", False)]
)
def test_guard(repo, tmp_path, command, blocked):
    fake_jj = tmp_path / "jj"
    fake_jj.write_text("#!/bin/sh\nexit 0\n")
    fake_jj.chmod(0o755)
    result = subprocess.run(
        [str(repo / "configs/agents/hooks/jj-guard.sh")],
        input=json.dumps({"cwd": str(tmp_path), "tool_input": {"command": command}}),
        text=True,
        capture_output=True,
        check=True,
        env={**os.environ, "PATH": f"{tmp_path}:{os.environ['PATH']}"},
    )
    if blocked:
        assert json.loads(result.stdout)["hookSpecificOutput"]["permissionDecision"] == "deny"
    else:
        assert result.stdout == ""


def test_stop_does_not_loop(repo):
    result = subprocess.run(
        [str(repo / "configs/agents/hooks/jj-commit.sh")],
        input='{"stop_hook_active": true}',
        text=True,
        capture_output=True,
        check=True,
    )
    assert result.stdout == ""


@pytest.mark.parametrize(
    "event,state,message",
    [
        ("PermissionRequest", "waiting", "Needs permission to continue"),
        ("SubagentStop", "completed", "An agent finished"),
        ("Stop", "completed", "Finished responding"),
    ],
)
def test_codex_notifications(repo, tmp_path, event, state, message):
    log = tmp_path / "calls"
    for name in ("zellij", "osascript"):
        executable = tmp_path / name
        executable.write_text('#!/bin/sh\nprintf "%s\\n" "$@" >> "$HOOK_TEST_LOG"\n')
        executable.chmod(0o755)
    subprocess.run(
        [str(repo / "configs/agents/hooks/notify.sh"), "Codex"],
        input=json.dumps({"hook_event_name": event, "cwd": str(tmp_path)}),
        text=True,
        capture_output=True,
        check=True,
        env={
            **os.environ,
            "PATH": f"{tmp_path}:{os.environ['PATH']}",
            "HOOK_TEST_LOG": str(log),
            "ZELLIJ": "1",
            "ZELLIJ_PANE_ID": "42",
        },
    )
    calls = log.read_text()
    assert f"zellij-attention::{state}::42" in calls
    assert message in calls
    assert "Codex" in calls


def _run_reek_check(repo, tmp_path, lint_exit, config="reek.toml"):
    for name, body in {
        "jj": f'case "$1" in root) echo {tmp_path};; diff) echo a.py;; esac',
        "reek": f'[ "$1" = lint ] && {{ echo "bad finding"; exit {lint_exit}; }}; exit 0',
    }.items():
        executable = tmp_path / name
        executable.write_text(f"#!/bin/sh\n{body}\n")
        executable.chmod(0o755)
    if config:
        (tmp_path / config).parent.mkdir(exist_ok=True)
        (tmp_path / config).write_text("")
    (tmp_path / "a.py").write_text("")
    return subprocess.run(
        [str(repo / "configs/agents/hooks/reek-check.sh")],
        input=json.dumps({"cwd": str(tmp_path)}),
        text=True,
        capture_output=True,
        check=True,
        env={**os.environ, "PATH": f"{tmp_path}:{os.environ['PATH']}"},
    )


def test_reek_check_blocks_when_lint_fails(repo, tmp_path):
    result = _run_reek_check(repo, tmp_path, lint_exit=1)
    assert json.loads(result.stdout)["decision"] == "block"
    assert "bad finding" in json.loads(result.stdout)["reason"]


def test_reek_check_allows_stop_when_lint_passes(repo, tmp_path):
    assert _run_reek_check(repo, tmp_path, lint_exit=0).stdout == ""


def test_reek_check_skips_projects_without_reek_toml(repo, tmp_path):
    assert _run_reek_check(repo, tmp_path, lint_exit=1, config=None).stdout == ""


def test_reek_check_finds_config_in_reekcfg_dir(repo, tmp_path):
    result = _run_reek_check(repo, tmp_path, lint_exit=1, config="reekcfg/reek.toml")
    assert json.loads(result.stdout)["decision"] == "block"
