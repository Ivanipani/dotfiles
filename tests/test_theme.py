"""Renderables must not depend on the CLI's Rich theme.

Textual draws Rich objects through its own console, which has no theme, so a
named style resolves in the CLI and raises MissingStyle in the TUI. Everything
shared between the two front ends is rendered here through a themeless console.
"""

from __future__ import annotations

import io

from rich.console import Console

from dot import reconcile, ui
from dot.actions import Status
from dot.theme import GLYPH, RESULT_GLYPH, STYLES

BARE = Console(width=200, no_color=True, file=io.StringIO())


def render(renderable) -> None:
    BARE.print(renderable)


def test_every_style_in_the_table_is_a_real_rich_style():
    for style in STYLES.values():
        BARE.get_style(style)  # raises MissingStyle if it is not parseable


def test_every_status_has_a_glyph_and_a_style():
    for status in Status:
        assert status in GLYPH
        BARE.get_style(ui.status_text(status).style)


def test_every_apply_result_has_a_glyph():
    assert set(RESULT_GLYPH) >= {"applied", "unchanged", "blocked", "failed"}


def test_shared_renderables_survive_a_themeless_console(repo, modules, host, ctx):
    items = reconcile.build(modules, host)
    checked = reconcile.check(items, ctx)

    render(ui.plan_tree(items, host))
    for c in checked[:20]:
        render(ui.action_line(c.action, host))

    drift_rows = [
        (c.module, c.drift.status, str(c.action.target() or ""), c.drift.detail) for c in checked
    ]
    render(ui.drift_table(drift_rows, host))
    render(ui.summary_bar({s: 1 for s in Status}))

    apply_rows = [(c.module, "applied", str(c.action.target() or ""), "") for c in checked]
    render(ui.apply_table(apply_rows, host))
    render(ui.result_bar({"applied": 1, "blocked": 2, "failed": 3, "unchanged": 4}))

    render(ui.facts_view(host, [m.name for m in modules], []))
    render(
        ui.doctor_table(
            [
                ("vcs", "git", "/usr/bin/git", None, None),
                ("vcs", "jj", "/usr/bin/jj", "0.34.0", "0.33.0"),
                ("vcs", "difft", "/usr/bin/difft", "0.60", None),
                ("vcs", "lazygit", None, None, None),
            ]
        )
    )
    render(ui.banner(host, "test"))
    render(ui.hint("a hint"))

    for status in Status:
        render(ui.diff_panel("m", "~/x", status, "--- a\n+++ b\n-x\n+y\n"))
        render(ui.note_panel("m", "~/x", status, "some detail"))


def test_path_text_prefers_repo_relative_then_home(host):
    assert ui.shorten(host.repo / "configs" / "vcs" / "module.py", host) == "configs/vcs/module.py"
    assert ui.shorten(host.home / ".zshrc", host) == "~/.zshrc"
    assert ui.shorten("/etc/hosts", host) == "/etc/hosts"


def test_rel_source_shortens_urls(host):
    url = "https://github.com/o/r/releases/download/1.0/plugin.wasm"
    assert ui.rel_source(url, host.repo) == "github.com/…/plugin.wasm"
    assert (
        ui.rel_source(str(host.repo / "configs" / "zsh" / "atuin.toml"), host.repo)
        == "configs/zsh/atuin.toml"
    )
