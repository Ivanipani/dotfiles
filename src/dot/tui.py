"""Full-screen drift browser.

`dot status` tells you what drifted; this lets you walk it, read the diff and
apply a module without leaving the report.
"""

from __future__ import annotations

from collections import Counter
from pathlib import Path

from rich.console import Group
from rich.syntax import Syntax
from rich.text import Text
from textual import work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.widgets import DataTable, Footer, Header, Static

from . import reconcile, session
from .actions import Ctx, Status
from .theme import GLYPH, RESULT_GLYPH, S, status_style
from .ui import action_line, shorten

ALL = "all"


def status_cell(status: Status) -> Text:
    return Text(f"{GLYPH.get(status, '?')} {status.value}", style=status_style(status))


class DotApp(App):
    TITLE = "dot"
    CSS = """
    Screen { layers: base; }

    #main { height: 1fr; }

    #body { height: 1fr; }

    #modules {
        width: 28;
        border-right: solid $panel;
        background: $surface;
    }

    #right { width: 1fr; }

    #drift { height: 1fr; }

    #detail {
        height: 45%;
        border-top: solid $panel;
        background: $surface;
        padding: 0 1;
    }

    #summary {
        height: 1;
        padding: 0 1;
        background: $panel;
        color: $text;
    }

    DataTable > .datatable--cursor { background: $accent; }
    """

    BINDINGS = [
        Binding("r", "rescan", "Rescan"),
        Binding("a", "apply_one", "Apply module"),
        Binding("A", "apply_all", "Apply all"),
        Binding("f", "toggle_ok", "Show/hide ok"),
        Binding("q", "quit", "Quit"),
    ]

    def __init__(self, repo: Path) -> None:
        super().__init__()
        self.repo = repo
        self._ctx: Ctx | None = None
        self.checked: list = []
        self.rows: list = []
        self.filter_module: str | None = None
        self.show_ok = False
        self.busy = False
        # What the two read-only panes currently show. Kept as plain attributes
        # so behaviour is assertable without reaching into Textual internals.
        self.summary_text = Text()
        self.detail_renderable = None

    # ----------------------------------------------------------------- layout

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False)
        with Vertical(id="main"):
            with Horizontal(id="body"):
                yield DataTable(id="modules", cursor_type="row", show_header=True)
                with Vertical(id="right"):
                    yield DataTable(id="drift", cursor_type="row", show_header=True)
                    with VerticalScroll(id="detail"):
                        yield Static(id="detail-body")
            yield Static("", id="summary")
        yield Footer()

    def on_mount(self) -> None:
        modules = self.query_one("#modules", DataTable)
        modules.add_column("module", width=14)
        modules.add_column("drift", width=9)
        drift = self.query_one("#drift", DataTable)
        drift.add_column("state", width=12)
        drift.add_column("target", width=48)
        drift.add_column("detail")
        self.action_rescan()

    # ------------------------------------------------------------------ scan

    def action_rescan(self) -> None:
        ctx, mods = session.setup(self.repo)
        _, items, _ = session.plan(ctx, mods)
        self._ctx = ctx
        self.checked = reconcile.check(items, ctx)
        self.sub_title = f"{ctx.host.hostname} · {ctx.host.os}/{ctx.host.profile}"
        self._fill_modules()
        self._fill_drift()

    @property
    def ctx(self) -> Ctx:
        """The context of the last scan.

        on_mount calls action_rescan before anything can render, so the only
        way to reach this with nothing set is a half-constructed app.
        """
        if self._ctx is None:
            raise RuntimeError("no scan yet: action_rescan has not run")
        return self._ctx

    def _counts(self, module: str | None) -> Counter:
        rows = [c for c in self.checked if module is None or c.module == module]
        return Counter(c.drift.status for c in rows)

    def _badge(self, counts: Counter) -> Text:
        drifted = {s: n for s, n in counts.items() if s is not Status.OK}
        if not drifted:
            return Text(f"{GLYPH[Status.OK]} clean", style=S.ok)
        worst = max(drifted, key=lambda s: reconcile.SEVERITY[s])
        return Text(f"{GLYPH[worst]} {sum(drifted.values())}", style=status_style(worst))

    def _fill_modules(self) -> None:
        table = self.query_one("#modules", DataTable)
        table.clear()
        table.add_row(Text(ALL, style="bold"), self._badge(self._counts(None)), key=ALL)
        for name in sorted({c.module for c in self.checked}):
            table.add_row(Text(name), self._badge(self._counts(name)), key=name)

    def _fill_drift(self) -> None:
        table = self.query_one("#drift", DataTable)
        table.clear()
        self.rows = [
            c
            for c in self.checked
            if (self.filter_module in (None, ALL) or c.module == self.filter_module)
            and (self.show_ok or not c.drift.ok)
        ]
        for i, c in enumerate(self.rows):
            label = shorten(str(c.action.target() or c.action.describe()), self.ctx.host)
            table.add_row(
                status_cell(c.drift.status), Text(label), Text(c.drift.detail), key=str(i)
            )
        self._update_summary()
        if not self.rows:
            self._set_detail(
                Text("nothing to show — press f to include entries that are already ok", S.hint)
            )

    def _update_summary(self) -> None:
        counts = self._counts(None if self.filter_module in (None, ALL) else self.filter_module)
        bar = Text()
        for status in Status:
            n = counts.get(status, 0)
            if not n:
                continue
            if bar:
                bar.append("   ")
            bar.append(f"{n} {GLYPH[status]} {status.value}", style=status_style(status))
        scope = self.filter_module or ALL
        bar.append(f"    [{scope}]", style=S.hint)
        self.summary_text = bar
        self.query_one("#summary", Static).update(bar)

    # -------------------------------------------------------------- reactions

    def on_data_table_row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        key = event.row_key.value
        if event.data_table.id == "modules":
            self.filter_module = key
            self._fill_drift()
        elif event.data_table.id == "drift" and key is not None:
            self._show_detail(self.rows[int(key)])

    def _set_detail(self, renderable) -> None:
        self.detail_renderable = renderable
        self.query_one("#detail-body", Static).update(renderable)

    def _show_detail(self, checked) -> None:
        action = checked.action
        header = Text()
        header.append(f"{checked.module}  ", style=S.module)
        header.append_text(status_cell(checked.drift.status))
        header.append("\n")
        header.append_text(action_line(action, self.ctx.host))
        header.append("\n")
        diff = session.unified_diff(checked, self.ctx, self.ctx.host.home)
        if diff:
            syntax = Syntax(diff, "diff", background_color="default", word_wrap=True)
            self._set_detail(Group(header, syntax))
            return
        if checked.drift.detail:
            header.append(f"\n{checked.drift.detail}", style=S.detail)
        self._set_detail(header)

    def action_toggle_ok(self) -> None:
        self.show_ok = not self.show_ok
        self._fill_drift()

    # ----------------------------------------------------------------- apply

    def action_apply_one(self) -> None:
        target = self.filter_module
        self._apply(None if target in (None, ALL) else target)

    def action_apply_all(self) -> None:
        self._apply(None)

    def _apply(self, module: str | None) -> None:
        if self.busy:
            self.notify("already applying", severity="warning")
            return
        self.busy = True
        self.notify(f"applying {module or ALL}…")
        self._apply_worker(module)

    @work(thread=True, exclusive=True, group="apply")
    def _apply_worker(self, module: str | None) -> None:
        ctx, mods = session.setup(self.repo)
        _, items, _ = session.plan(ctx, mods, [module] if module else None)
        results = reconcile.apply(reconcile.check(items, ctx), ctx)
        if ctx.state.dirty:
            ctx.state.save()
        self.call_from_thread(self._applied, results)

    def _applied(self, results) -> None:
        self.busy = False
        counts = Counter(r.result for r in results)
        parts = [f"{n} {RESULT_GLYPH.get(k, '')} {k}" for k, n in sorted(counts.items()) if n]
        severity = "error" if counts["failed"] or counts["blocked"] else "information"
        self.notify("  ".join(parts) or "nothing to do", severity=severity)
        self.action_rescan()


def run(repo: Path) -> None:
    DotApp(repo).run()
