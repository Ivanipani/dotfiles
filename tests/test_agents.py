"""Shared agent configuration and hook behavior."""

import json
import os
import subprocess
from pathlib import Path

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
        normalized = (
            str(claude["hooks"][event])
            .replace(".claude", ".codex")
            .replace("claude-code", "codex")
            .replace("--agent claude", "--agent codex")
        )
        assert normalized == str(codex["hooks"][event])
    assert "Notification" not in codex["hooks"]
    assert {"PermissionRequest", "SubagentStop", "Stop"} <= codex["hooks"].keys()
    for config in (claude, codex):
        for groups in config["hooks"].values():
            for group in groups:
                for hook in group["hooks"]:
                    command = hook["command"].replace("$HOME", str(home))
                    if command.startswith('"'):  # skip inline shell commands
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


# --- quota guard / handoff -------------------------------------------------

FUTURE = 4_000_000_000
PAST = 1_000_000_000


def _hook(repo, tmp_path, script, payload, *args):
    return subprocess.run(
        [str(repo / "configs/agents/hooks" / script), *args],
        input=json.dumps({"cwd": str(tmp_path), "session_id": "s1", **payload}),
        text=True,
        capture_output=True,
        check=True,
        env={**os.environ, "AGENT_STATE_DIR": str(tmp_path / "state")},
    )


def _claude_quota(tmp_path, used, resets_at=FUTURE):
    cache = tmp_path / "state/quota/claude.json"
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(
        json.dumps({"five_hour": {"used_percentage": used, "resets_at": resets_at}})
    )


def _codex_transcript(tmp_path, used, resets_at=FUTURE, with_limits=True):
    event = {"type": "event_msg", "payload": {"type": "token_count", "info": {}}}
    if with_limits:
        event["payload"]["rate_limits"] = {
            "primary": {"used_percent": used, "resets_at": resets_at},
            "secondary": {"used_percent": 1.0, "resets_at": FUTURE},
        }
    transcript = tmp_path / "rollout.jsonl"
    transcript.write_text(json.dumps(event) + "\n")
    return str(transcript)


def _write_handoff(repo, tmp_path):
    script = f'. "{repo}/configs/agents/hooks/agent-state.sh"; handoff_path "{tmp_path}"'
    path = subprocess.run(
        ["sh", "-c", script],
        text=True,
        capture_output=True,
        check=True,
        env={**os.environ, "AGENT_STATE_DIR": str(tmp_path / "state")},
    ).stdout.strip()
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write("work")
    return Path(path)


def _claude_guard(repo, tmp_path, event="PostToolUse", **payload):
    return _hook(repo, tmp_path, "quota-guard.sh", {"hook_event_name": event, **payload}, "claude")


def test_quota_guard_nudges_when_claude_usage_is_over_threshold(repo, tmp_path):
    _claude_quota(tmp_path, 94.0)
    output = json.loads(_claude_guard(repo, tmp_path).stdout)["hookSpecificOutput"]
    assert output["hookEventName"] == "PostToolUse"
    assert "94%" in output["additionalContext"]
    assert "handoff" in output["additionalContext"]


def test_quota_guard_is_silent_when_claude_usage_is_under_threshold(repo, tmp_path):
    _claude_quota(tmp_path, 40.0)
    assert _claude_guard(repo, tmp_path).stdout == ""


def test_quota_guard_ignores_a_claude_window_that_already_reset(repo, tmp_path):
    _claude_quota(tmp_path, 99.0, resets_at=PAST)
    assert _claude_guard(repo, tmp_path).stdout == ""


def test_quota_guard_is_silent_when_claude_quota_was_never_cached(repo, tmp_path):
    assert _claude_guard(repo, tmp_path).stdout == ""


def test_quota_guard_nudges_only_once_per_session(repo, tmp_path):
    _claude_quota(tmp_path, 94.0)
    _claude_guard(repo, tmp_path)
    assert _claude_guard(repo, tmp_path).stdout == ""


def test_quota_guard_respects_threshold_override(repo, tmp_path, monkeypatch):
    _claude_quota(tmp_path, 40.0)
    monkeypatch.setenv("AGENT_QUOTA_THRESHOLD", "30")
    assert _claude_guard(repo, tmp_path).stdout != ""


def test_quota_guard_blocks_stop_when_no_handoff_was_written(repo, tmp_path):
    _claude_quota(tmp_path, 94.0)
    result = json.loads(_claude_guard(repo, tmp_path, event="Stop", stop_hook_active=False).stdout)
    assert result["decision"] == "block"
    assert "handoff" in result["reason"]


def test_quota_guard_allows_stop_once_handoff_is_newer_than_the_nudge(repo, tmp_path):
    _claude_quota(tmp_path, 94.0)
    _claude_guard(repo, tmp_path)
    handoff = _write_handoff(repo, tmp_path)
    os.utime(handoff, (FUTURE, FUTURE))
    assert _claude_guard(repo, tmp_path, event="Stop", stop_hook_active=False).stdout == ""


def test_quota_guard_does_not_loop_on_stop_retry(repo, tmp_path):
    _claude_quota(tmp_path, 94.0)
    assert _claude_guard(repo, tmp_path, event="Stop", stop_hook_active=True).stdout == ""


def test_quota_guard_is_silent_on_stop_when_usage_is_under_threshold(repo, tmp_path):
    _claude_quota(tmp_path, 10.0)
    assert _claude_guard(repo, tmp_path, event="Stop", stop_hook_active=False).stdout == ""


def _codex_guard(repo, tmp_path, transcript, event="PostToolUse"):
    return _hook(
        repo,
        tmp_path,
        "quota-guard.sh",
        {"hook_event_name": event, "transcript_path": transcript},
        "codex",
    )


def test_quota_guard_nudges_when_codex_rollout_reports_usage_over_threshold(repo, tmp_path):
    output = json.loads(_codex_guard(repo, tmp_path, _codex_transcript(tmp_path, 95.0)).stdout)
    assert "95%" in output["hookSpecificOutput"]["additionalContext"]


def test_quota_guard_is_silent_when_codex_usage_is_under_threshold(repo, tmp_path):
    assert _codex_guard(repo, tmp_path, _codex_transcript(tmp_path, 12.0)).stdout == ""


def test_quota_guard_is_silent_when_codex_rollout_has_no_rate_limits(repo, tmp_path):
    transcript = _codex_transcript(tmp_path, 0, with_limits=False)
    assert _codex_guard(repo, tmp_path, transcript).stdout == ""


def test_quota_statusline_caches_rate_limits_for_the_guard(repo, tmp_path):
    limits = {"five_hour": {"used_percentage": 25, "resets_at": FUTURE}}
    _hook(repo, tmp_path, "quota-statusline.sh", {"rate_limits": limits})
    assert json.loads((tmp_path / "state/quota/claude.json").read_text()) == limits


def test_quota_statusline_prints_a_usage_summary(repo, tmp_path):
    limits = {
        "five_hour": {"used_percentage": 25, "resets_at": FUTURE},
        "seven_day": {"used_percentage": 47.6, "resets_at": FUTURE},
    }
    result = _hook(repo, tmp_path, "quota-statusline.sh", {"rate_limits": limits})
    assert result.stdout.strip() == "5h:25% 7d:47%"


def test_quota_statusline_leaves_cache_alone_without_rate_limits(repo, tmp_path):
    _hook(repo, tmp_path, "quota-statusline.sh", {})
    assert not (tmp_path / "state/quota/claude.json").exists()


def test_handoff_context_points_new_session_at_a_recent_handoff(repo, tmp_path):
    handoff = _write_handoff(repo, tmp_path)
    assert str(handoff) in _hook(repo, tmp_path, "handoff-context.sh", {}).stdout


def test_handoff_context_is_silent_without_a_handoff(repo, tmp_path):
    assert _hook(repo, tmp_path, "handoff-context.sh", {}).stdout == ""


def test_quota_guard_borrows_codex_quota_from_a_recent_rollout_in_a_fresh_session(repo, tmp_path):
    sessions = tmp_path / "codex/sessions/2026/11/16"
    sessions.mkdir(parents=True)
    (sessions / "rollout-old.jsonl").write_text(Path(_codex_transcript(tmp_path, 95.0)).read_text())
    fresh = sessions / "rollout-fresh.jsonl"
    fresh.write_text(Path(_codex_transcript(tmp_path, 0, with_limits=False)).read_text())
    output = json.loads(_codex_guard(repo, tmp_path, str(fresh), event="UserPromptSubmit").stdout)
    assert "95%" in output["hookSpecificOutput"]["additionalContext"]


def _run_rex_status(repo, tmp_path, payload, *, in_rex=True, app=None):
    tty = tmp_path / "tty"
    tty.touch()
    env = {**os.environ, "REX_STATUS_TTY": str(tty)}
    env.pop("REX_BLOCK", None)
    if in_rex:
        env["REX_BLOCK"] = "block:test"
    subprocess.run(
        [str(repo / "configs/agents/hooks/rex-status.sh"), *([app] if app else [])],
        input=json.dumps(payload),
        text=True,
        capture_output=True,
        check=True,
        env=env,
    )
    return tty.read_text()


@pytest.mark.parametrize(
    "payload, expected",
    [
        ({"hook_event_name": "UserPromptSubmit"}, "state=working:app=claude-code"),
        ({"hook_event_name": "PostToolUse"}, "state=working:app=claude-code"),
        ({"hook_event_name": "Stop"}, "state=done:app=claude-code"),
        ({"hook_event_name": "SessionStart"}, "state=idle:app=claude-code"),
        (
            {"hook_event_name": "Notification", "notification_type": "permission_prompt"},
            "state=blocked:app=claude-code:kind=permission",
        ),
        (
            {"hook_event_name": "Notification", "notification_type": "elicitation_dialog"},
            "state=blocked:app=claude-code:kind=question",
        ),
        ({"hook_event_name": "PermissionRequest"}, "state=blocked:app=claude-code:kind=permission"),
    ],
)
def test_rex_status_reports_the_state_for_each_hook_event(repo, tmp_path, payload, expected):
    # GIVEN a hook event, WHEN rex-status runs inside Rex, THEN it writes the matching OSC 7501 state
    assert expected in _run_rex_status(repo, tmp_path, payload)


def test_rex_status_names_the_app_from_its_argument(repo, tmp_path):
    out = _run_rex_status(repo, tmp_path, {"hook_event_name": "Stop"}, app="codex")
    assert "state=done:app=codex" in out


def test_rex_status_base64_encodes_the_notification_message(repo, tmp_path):
    payload = {"hook_event_name": "Notification", "notification_type": "permission_prompt", "message": "Run ls?"}
    assert ":msg=UnVuIGxzPw==" in _run_rex_status(repo, tmp_path, payload)


def test_rex_status_ignores_events_it_has_no_state_for(repo, tmp_path):
    assert _run_rex_status(repo, tmp_path, {"hook_event_name": "PreToolUse"}) == ""


def test_rex_status_is_silent_outside_rex(repo, tmp_path):
    assert _run_rex_status(repo, tmp_path, {"hook_event_name": "Stop"}, in_rex=False) == ""
