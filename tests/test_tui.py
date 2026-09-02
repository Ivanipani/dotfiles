"""The Textual drift browser, driven headlessly."""

from __future__ import annotations

from pathlib import Path

import pytest
from rich.console import Console
from textual.widgets import DataTable

from dot.actions import Status
from dot.tui import ALL, DotApp


@pytest.fixture
def app(fake_env, repo):
    return DotApp(repo)


def _flatten(renderable) -> str:
    """Render to plain text so assertions read the pane the way a user does."""
    console = Console(width=200, no_color=True)
    with console.capture() as cap:
        console.print(renderable)
    return cap.get()


def modules_table(app) -> DataTable:
    return app.query_one("#modules", DataTable)


def drift_table(app) -> DataTable:
    return app.query_one("#drift", DataTable)


async def test_mounts_and_lists_every_module(app):
    async with app.run_test():
        table = modules_table(app)
        assert table.row_count >= 2
        first = table.get_row_at(0)[0]
        assert str(first) == ALL


async def test_a_fresh_home_shows_drift(app):
    async with app.run_test():
        assert drift_table(app).row_count > 0
        assert all(not c.drift.ok for c in app.rows)


async def test_f_toggles_the_ok_rows(app):
    async with app.run_test() as pilot:
        before = drift_table(app).row_count
        await pilot.press("f")
        assert app.show_ok is True
        assert drift_table(app).row_count >= before
        await pilot.press("f")
        assert drift_table(app).row_count == before


async def test_selecting_a_module_filters_the_table(app):
    async with app.run_test() as pilot:
        table = modules_table(app)
        names = [str(table.get_row_at(i)[0]) for i in range(table.row_count)]
        target = names[1]
        table.move_cursor(row=1)
        await pilot.pause()
        assert app.filter_module == target
        assert {c.module for c in app.rows} == {target}


async def test_apply_from_the_tui_clears_the_drift(app):
    async with app.run_test() as pilot:
        assert drift_table(app).row_count > 0
        await pilot.press("A")
        await app.workers.wait_for_complete()
        await pilot.pause()
        assert app.busy is False
        assert drift_table(app).row_count == 0
        assert all(c.drift.ok for c in app.checked)


async def test_detail_pane_shows_a_diff_for_a_stale_render(app, fake_env):
    async with app.run_test() as pilot:
        await pilot.press("A")
        await app.workers.wait_for_complete()
        await pilot.pause()

        # Corrupt one rendered build file so its action becomes stale.
        build = Path(fake_env["DOT_BUILD"])
        rendered = next(p for p in build.rglob("*.zshrc"))
        rendered.write_text("# tampered\n")

        await pilot.press("r")
        await pilot.pause()
        stale = [c for c in app.checked if c.drift.status is Status.STALE]
        assert stale, "expected the tampered build file to read as stale"

        drift_table(app).move_cursor(row=0)
        await pilot.pause()
        assert app.detail_renderable is not None
        assert "tampered" in _flatten(app.detail_renderable)


async def test_summary_bar_tracks_the_filter(app):
    async with app.run_test() as pilot:
        assert str(app.summary_text).strip().endswith(f"[{ALL}]")
        table = modules_table(app)
        name = str(table.get_row_at(1)[0])
        table.move_cursor(row=1)
        await pilot.pause()
        assert str(app.summary_text).strip().endswith(f"[{name}]")
