"""Host facts: the single, frozen input every module plans against."""

from __future__ import annotations

import os
import platform
import socket
import tomllib
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

DEFAULT_PROFILE = "personal"


def deep_merge(base: Mapping[str, Any], over: Mapping[str, Any]) -> dict[str, Any]:
    out = dict(base)
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(out.get(key), dict):
            out[key] = deep_merge(out[key], value)
        else:
            out[key] = value
    return out


def _read_toml(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    with path.open("rb") as fh:
        return tomllib.load(fh)


def detect_os() -> str:
    system = platform.system()
    if system == "Darwin":
        return "macos"
    if system == "Linux":
        return "linux"
    raise SystemExit(f"unsupported platform: {system}")


def detect_hostname() -> str:
    name = socket.gethostname().split(".")[0]
    return name.lower().replace("_", "-")


@dataclass(frozen=True)
class Host:
    """Everything a module is allowed to branch on."""

    os: str
    arch: str
    hostname: str
    profile: str
    tags: frozenset[str]
    home: Path
    xdg_config: Path
    xdg_data: Path
    xdg_state: Path
    xdg_cache: Path
    repo: Path
    vars: Mapping[str, Any]

    @property
    def macos(self) -> bool:
        return self.os == "macos"

    @property
    def linux(self) -> bool:
        return self.os == "linux"

    @property
    def fingerprint(self) -> str:
        return f"{self.os}-{self.arch}-{self.hostname}-{self.profile}"

    def has(self, tag: str) -> bool:
        return tag in self.tags

    def var(self, dotted: str, default: Any = None) -> Any:
        node: Any = self.vars
        for part in dotted.split("."):
            if not isinstance(node, Mapping) or part not in node:
                return default
            node = node[part]
        return node

    def as_dict(self) -> dict[str, Any]:
        return {
            "os": self.os,
            "arch": self.arch,
            "hostname": self.hostname,
            "profile": self.profile,
            "tags": sorted(self.tags),
            "macos": self.macos,
            "linux": self.linux,
            "home": str(self.home),
            "xdg_config": str(self.xdg_config),
            "xdg_data": str(self.xdg_data),
            "xdg_state": str(self.xdg_state),
            "xdg_cache": str(self.xdg_cache),
            "repo": str(self.repo),
            "fingerprint": self.fingerprint,
        }


def _xdg(env: Mapping[str, str], key: str, home: Path, fallback: str) -> Path:
    raw = env.get(key)
    return Path(raw).expanduser() if raw else home / fallback


def load_vars(
    repo: Path,
    os_name: str,
    hostname: str,
    local_file: Path,
    env: Mapping[str, str] | None = None,
) -> tuple[dict[str, Any], str, frozenset[str]]:
    """Layer vars/, then resolve profile from the result and re-layer.

    Profile lives in the vars it also selects, so it takes two passes: discover
    it from the layers that do not depend on it, then fold in vars/profile/.
    """
    vroot = repo / "vars"
    default = _read_toml(vroot / "default.toml")
    os_layer = _read_toml(vroot / "os" / f"{os_name}.toml")
    host_layer = _read_toml(vroot / "host" / f"{hostname}.toml")
    local_layer = _read_toml(local_file)

    discovery = deep_merge(deep_merge(deep_merge(default, os_layer), host_layer), local_layer)
    env = os.environ if env is None else env
    profile = env.get("DOT_PROFILE") or discovery.get("profile") or DEFAULT_PROFILE
    profile_layer = _read_toml(vroot / "profile" / f"{profile}.toml")

    merged = default
    for layer in (os_layer, profile_layer, host_layer, local_layer):
        merged = deep_merge(merged, layer)

    merged["profile"] = profile
    tags = frozenset(merged.get("tags", []))
    return merged, profile, tags


def detect_host(repo: Path, *, env: Mapping[str, str] | None = None) -> Host:
    env = os.environ if env is None else env
    home = Path(env.get("HOME") or Path.home()).expanduser()
    os_name = env.get("DOT_OS") or detect_os()
    hostname = env.get("DOT_HOSTNAME") or detect_hostname()

    xdg_config = _xdg(env, "XDG_CONFIG_HOME", home, ".config")
    local_file = Path(env.get("DOT_LOCAL_VARS") or xdg_config / "dot" / "local.toml")

    merged, profile, tags = load_vars(repo, os_name, hostname, local_file, env)

    return Host(
        os=os_name,
        arch=env.get("DOT_ARCH") or platform.machine(),
        hostname=hostname,
        profile=profile,
        tags=tags,
        home=home,
        xdg_config=xdg_config,
        xdg_data=_xdg(env, "XDG_DATA_HOME", home, ".local/share"),
        xdg_state=_xdg(env, "XDG_STATE_HOME", home, ".local/state"),
        xdg_cache=_xdg(env, "XDG_CACHE_HOME", home, ".cache"),
        repo=repo,
        vars=merged,
    )
