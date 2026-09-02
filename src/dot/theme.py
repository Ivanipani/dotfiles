"""One visual language, shared by the Rich CLI and the Textual TUI.

Styles are stored as concrete Rich style strings rather than theme names:
Textual renders Rich objects through its own console, which does not carry the
CLI's theme, so a named style would resolve in one front end and blow up in the
other. `THEME` still exists so console markup like `[ok]✔[/]` works in the CLI.
"""

from __future__ import annotations

from types import SimpleNamespace

from rich.theme import Theme

from .actions import Status

STYLES: dict[str, str] = {
    # drift states
    "ok": "green",
    "absent": "yellow",
    "stale": "yellow",
    "dangling": "red",
    "hijacked": "red",
    "clobbered": "bold red",
    "edited": "magenta",
    "conflict": "bold magenta",
    "orphan": "blue",
    "error": "bold red",
    # apply results
    "applied": "green",
    "unchanged": "dim",
    "blocked": "bold magenta",
    "failed": "bold red",
    "skipped": "dim",
    # chrome
    "module": "bold cyan",
    "dir": "dim",
    "leaf": "default",
    "detail": "dim italic",
    "head": "bold",
    "hint": "dim",
    "kind": "dim cyan",
    "rule": "cyan",
}

THEME = Theme(STYLES)

#: Attribute access for the chrome styles, e.g. ``S.module``.
S = SimpleNamespace(**STYLES)

# Single-cell glyphs so columns stay aligned in any monospace font.
GLYPH: dict[Status, str] = {
    Status.OK: "✔",
    Status.ABSENT: "○",
    Status.STALE: "≈",
    Status.DANGLING: "✗",
    Status.HIJACKED: "↔",
    Status.CLOBBERED: "▪",
    Status.EDITED: "✎",
    Status.CONFLICT: "⨯",
    Status.ORPHAN: "⊘",
    Status.ERROR: "!",
}

RESULT_GLYPH = {
    "applied": "✔",
    "unchanged": "·",
    "blocked": "▪",
    "failed": "!",
    "skipped": "·",
}

KIND_GLYPH = {
    "dir": "▸",
    "symlink": "→",
    "render": "❯",
    "fetch": "⇩",
    "run": "$",
    "action": "•",
}


def status_style(status: Status) -> str:
    return STYLES.get(status.value, "")


def status_glyph(status: Status) -> str:
    return GLYPH.get(status, "?")


def result_style(result: str) -> str:
    return STYLES.get(result, "")
