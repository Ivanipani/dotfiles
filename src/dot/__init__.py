"""Public API for module.py files.

from dot import Module, Dir, Symlink, Render, Fetch, Run
from dot import brew, cask, apt, cargo, uv_tool, rustup, github
"""

__version__ = "0.1.0"

from .actions import Action, Dir, Drift, Fetch, Render, Run, Status, Symlink
from .facts import Host
from .modules import Module
from .packages import Package, Release, apt, brew, cargo, cask, github, rustup, uv_tool

__all__ = [
    "Action",
    "Dir",
    "Drift",
    "Fetch",
    "Host",
    "Module",
    "Package",
    "Release",
    "Render",
    "Run",
    "Status",
    "Symlink",
    "__version__",
    "apt",
    "brew",
    "cargo",
    "cask",
    "github",
    "rustup",
    "uv_tool",
]
