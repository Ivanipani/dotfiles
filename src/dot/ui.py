"""Rich rendering. Nothing in here decides anything."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path

from rich import box
from rich.console import Console, Group, RenderableType
from rich.panel import Panel
from rich.rule import Rule
from rich.syntax import Syntax
from rich.table import Table
from rich.text import Text
from rich.tree import Tree

from .actions import Status
from .facts import Host
from .theme import (
    KIND_GLYPH,
    RESULT_GLYPH,
    THEME,
    S,
    result_style,
    status_glyph,
    status_style,
)

console = Console(theme=THEME)
err_console = Console(theme=THEME, stderr=True)


# --------------------------------------------------------------------------- #
# Primitives


def shorten(path: str | Path, host: Host) -> str:
    """Repo paths show repo-relative, home paths as ~/…; the repo wins if nested."""
    s = str(path)
    for root, prefix in ((str(host.repo), ""), (str(host.home), "~")):
        if s == root:
            return prefix or root
        if s.startswith(root + "/"):
            return prefix + s[len(root) :] if prefix else s[len(root) + 1 :]
    return s


def path_text(path: str | Path, host: Host) -> Text:
    """Dim the directory so the eye lands on the file that actually changed."""
    short = shorten(path, host)
    head, sep, tail = short.rpartition("/")
    if not sep:
        return Text(short, style=S.leaf)
    return Text.assemble((head + sep, S.dir), (tail, S.leaf))


def status_text(status: Status, *, glyph: bool = True) -> Text:
    style = status_style(status)
    label = f"{status_glyph(status)} {status.value}" if glyph else status.value
    return Text(label, style=style)


def banner(host: Host, title: str) -> Rule:
    subtitle = f"{host.hostname} · {host.os}/{host.profile}"
    return Rule(Text.assemble((f" {title} ", S.head), (f"{subtitle} ", S.hint)), style=S.rule)


def _table(*columns: tuple[str, dict]) -> Table:
    table = Table(
        box=box.SIMPLE_HEAD,
        header_style=S.head,
        show_edge=False,
        pad_edge=False,
        expand=False,
    )
    for name, kw in columns:
        table.add_column(name, **kw)
    return table


# --------------------------------------------------------------------------- #
# status


def _grouped_table(rows, host: Host, state_header: str, cell) -> Table:
    """Shared shape for the status and apply tables.

    The module name prints once per group, and the detail column only exists
    when something actually has a detail to show.
    """
    has_detail = any(r[3] for r in rows)
    table = _table(
        ("", {"no_wrap": True, "width": 10}),
        (state_header, {"no_wrap": True}),
        ("target", {"overflow": "fold"}),
        *([("detail", {"style": S.detail, "overflow": "fold"})] if has_detail else []),
    )
    previous = None
    for module, key, target, detail in rows:
        if previous is not None and module != previous:
            table.add_section()
        label = Text(module, style=S.module) if module != previous else Text("")
        cells = [label, cell(key), path_text(target, host)]
        if has_detail:
            cells.append(detail)
        table.add_row(*cells)
        previous = module
    return table


def drift_table(rows: Sequence[tuple[str, Status, str, str]], host: Host) -> Table:
    """rows: (module, status, target, detail)."""
    return _grouped_table(rows, host, "state", status_text)


def summary_bar(counts: Mapping[Status, int]) -> Text:
    out = Text()
    for status in Status:
        n = counts.get(status, 0)
        if not n:
            continue
        if out:
            out.append("   ")
        out.append(f"{n} ", style=S.head)
        out.append(f"{status_glyph(status)} {status.value}", style=status_style(status))
    return out or Text("nothing planned", style=S.hint)


def hint(message: str) -> Text:
    return Text(message, style=S.hint)


# --------------------------------------------------------------------------- #
# plan


def rel_source(source: str, repo: Path) -> str:
    """Repo files show as repo-relative; URLs keep just host and last segment."""
    if source.startswith("https://") or source.startswith("http://"):
        rest = source.split("://", 1)[1]
        host, _, path = rest.partition("/")
        return f"{host}/…/{path.rsplit('/', 1)[-1]}" if path else host
    p = Path(source)
    try:
        return str(p.relative_to(repo))
    except ValueError:
        return source


def action_line(action, host: Host) -> Text:
    """`kind  target  ← source` — the one-line identity of an action."""
    line = Text.assemble(
        (f"{KIND_GLYPH.get(action.kind, '•')} ", S.kind),
        (f"{action.kind:<8}", S.kind),
    )
    target = action.target()
    line.append_text(path_text(target, host) if target else Text(action.describe(), S.leaf))
    origin = action.source()
    if origin:
        line.append("  ← ", style=S.hint)
        line.append(rel_source(origin, host.repo), style=S.detail)
    return line


def plan_tree(items, host: Host) -> Tree:
    root = Tree(Text("plan", style=S.head), guide_style=S.dir)
    branches: dict[str, Tree] = {}
    counts: Counter = Counter()
    for item in items:
        counts[item.module] += 1
    for item in items:
        if item.module not in branches:
            label = Text.assemble(
                (item.module, S.module), ("   ", ""), (f"{counts[item.module]} actions", S.hint)
            )
            branches[item.module] = root.add(label)
        branches[item.module].add(action_line(item.action, host))
    return root


# --------------------------------------------------------------------------- #
# facts


def _vars_tree(node: Mapping, tree: Tree) -> Tree:
    for key, value in sorted(node.items()):
        if isinstance(value, Mapping):
            _vars_tree(value, tree.add(Text(key, style=S.module)))
        elif isinstance(value, (list, tuple)):
            branch = tree.add(Text.assemble((key, S.module), (f"  [{len(value)}]", S.hint)))
            for entry in value:
                branch.add(Text(str(entry), style=S.leaf))
        else:
            tree.add(Text.assemble((key, S.leaf), ("  ", ""), (str(value), S.detail)))
    return tree


def facts_view(host: Host, enabled: Sequence[str], disabled: Sequence[str]) -> RenderableType:
    facts = _table(("", {"style": S.module, "no_wrap": True}), ("", {"overflow": "fold"}))
    facts.show_header = False
    for key, value in host.as_dict().items():
        if key in ("macos", "linux"):
            continue
        shown = ", ".join(value) if isinstance(value, list) else str(value)
        facts.add_row(key, shown)

    mods = Text()
    mods.append("enabled   ", style=S.head)
    mods.append(", ".join(enabled) or "none", style=S.ok)
    if disabled:
        mods.append("\ndisabled  ", style=S.head)
        mods.append(", ".join(disabled), style=S.hint)

    return Group(
        Panel(facts, title="host", title_align="left", border_style=S.rule, box=box.ROUNDED),
        Panel(
            _vars_tree(dict(host.vars), Tree(Text("vars", style=S.head), guide_style=S.dir)),
            title="vars",
            title_align="left",
            border_style=S.rule,
            box=box.ROUNDED,
        ),
        Panel(mods, title="modules", title_align="left", border_style=S.rule, box=box.ROUNDED),
    )


# --------------------------------------------------------------------------- #
# doctor / apply / diff


def doctor_table(rows: Iterable[tuple[str, str, str | None]]) -> Table:
    table = _table(
        ("", {"style": S.module, "no_wrap": True, "width": 10}),
        ("binary", {"no_wrap": True}),
        ("path", {"style": S.detail, "overflow": "fold"}),
    )
    previous = None
    for module, binary, found in rows:
        if previous is not None and module != previous:
            table.add_section()
        label = Text(module, style=S.module) if module != previous else Text("")
        mark = Text("✔ ", style=S.ok) if found else Text("✖ ", style=S.error)
        table.add_row(label, mark + Text(binary), found or "not found")
        previous = module
    return table


def apply_table(rows: Sequence[tuple[str, str, str, str]], host: Host) -> Table:
    """rows: (module, result, target, detail)."""

    def cell(result: str) -> Text:
        return Text(f"{RESULT_GLYPH.get(result, '·')} {result}", style=result_style(result))

    return _grouped_table(rows, host, "result", cell)


def result_bar(counts: Mapping[str, int]) -> Text:
    out = Text()
    for result in ("applied", "unchanged", "blocked", "failed"):
        n = counts.get(result, 0)
        if not n:
            continue
        if out:
            out.append("   ")
        out.append(f"{n} ", style=S.head)
        out.append(f"{RESULT_GLYPH[result]} {result}", style=result_style(result))
    return out or Text("nothing to do", style=S.hint)


def diff_panel(module: str, label: str, status: Status, body: str) -> Panel:
    title = Text.assemble(
        (module, S.module), ("  ", ""), (label, S.leaf), ("  ", ""), status_text(status)
    )
    inner: RenderableType
    if body.strip():
        inner = Syntax(body, "diff", background_color="default", word_wrap=True)
    else:
        inner = Text("no content difference", style=S.hint)
    return Panel(
        inner, title=title, title_align="left", border_style=status_style(status), box=box.ROUNDED
    )


def note_panel(module: str, label: str, status: Status, detail: str) -> Panel:
    title = Text.assemble(
        (module, S.module), ("  ", ""), (label, S.leaf), ("  ", ""), status_text(status)
    )
    return Panel(
        Text(detail or "—", style=S.detail),
        title=title,
        title_align="left",
        border_style=status_style(status),
        box=box.ROUNDED,
    )
