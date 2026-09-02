"""Jinja environment shared by every module."""

from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

from jinja2 import ChoiceLoader, Environment, FileSystemLoader, StrictUndefined

from .facts import Host


def _which(name: str) -> str | None:
    return shutil.which(name)


def _json(value: Any, indent: int = 2) -> str:
    """Vars as JSON, for config files that are JSON rather than a DSL.

    Not Jinja's `tojson`: that one HTML-escapes, so an em-dash or an
    apostrophe from a vars file lands in the output as \\u2014 / \\u0027.
    Sorted keys keep the render stable across dict orderings.
    """
    return json.dumps(value, indent=indent, ensure_ascii=False, sort_keys=True)


def make_env(host: Host, search: list[Path]) -> Environment:
    env = Environment(
        loader=ChoiceLoader([FileSystemLoader(str(p)) for p in search]),
        undefined=StrictUndefined,
        keep_trailing_newline=True,
        trim_blocks=True,
        lstrip_blocks=True,
    )
    env.globals.update(
        host=host,
        v=host.vars,
        home=str(host.home),
        repo=str(host.repo),
        which=_which,
    )
    env.filters["path"] = lambda p: str(Path(str(p)).expanduser())
    env.filters["json"] = _json
    return env


def render_template(env: Environment, template: Path, extra: dict[str, Any]) -> str:
    """Render by absolute path; the loader search list is only for {% include %}."""
    source = template.read_text()
    return env.from_string(source).render(**extra)
