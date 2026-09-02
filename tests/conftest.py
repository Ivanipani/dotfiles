from __future__ import annotations

import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "src"))

from dot.actions import Ctx  # noqa: E402
from dot.facts import detect_host  # noqa: E402
from dot.modules import discover  # noqa: E402
from dot.render import make_env  # noqa: E402
from dot.state import State  # noqa: E402


@pytest.fixture(scope="session")
def repo() -> Path:
    return REPO


@pytest.fixture(scope="session")
def modules(repo):
    return discover(repo)


def make_host(repo: Path, home: Path, *, os_name="macos", profile="personal", hostname="testbox"):
    """A Host pinned entirely by injected env, so tests never read the real machine."""
    home.mkdir(parents=True, exist_ok=True)
    env = {
        "HOME": str(home),
        "DOT_OS": os_name,
        "DOT_ARCH": "arm64",
        "DOT_HOSTNAME": hostname,
        "DOT_PROFILE": profile,
        "DOT_LOCAL_VARS": str(home / "no-such-local.toml"),
    }
    return detect_host(repo, env=env)


def make_ctx(repo: Path, host, mods, tmp: Path, **kw) -> Ctx:
    env = make_env(host, [m.dir for m in mods])
    return Ctx(
        host=host,
        state=State(tmp / "state.json", host.fingerprint),
        env=env,
        build_root=tmp / "build",
        backup_root=tmp / "backups",
        **kw,
    )


@pytest.fixture
def host(repo, tmp_path):
    return make_host(repo, tmp_path / "home")


@pytest.fixture
def ctx(repo, host, modules, tmp_path):
    return make_ctx(repo, host, modules, tmp_path / "run")


@pytest.fixture
def fake_env(repo, tmp_path, monkeypatch):
    """Point the whole app at a throwaway $HOME so nothing touches the machine."""
    home = tmp_path / "home"
    home.mkdir(parents=True, exist_ok=True)
    env = {
        "HOME": str(home),
        "DOT_REPO": str(repo),
        "DOT_OS": "macos",
        "DOT_ARCH": "arm64",
        "DOT_HOSTNAME": "testbox",
        "DOT_PROFILE": "personal",
        "DOT_LOCAL_VARS": str(tmp_path / "no-local.toml"),
        "DOT_BUILD": str(tmp_path / "build"),
        "XDG_STATE_HOME": str(tmp_path / "state"),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_CACHE_HOME": str(home / ".cache"),
    }
    for key, value in env.items():
        monkeypatch.setenv(key, value)
    return env
