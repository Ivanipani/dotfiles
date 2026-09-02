"""Actions are declarative state, not side effects.

Every action answers two questions independently: what does the machine look
like now (`check`), and how do I make it match (`apply`). Modules only ever
build these; the reconciler decides whether anything runs.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import urllib.request
from collections.abc import Mapping
from dataclasses import dataclass, replace
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any

from jinja2 import Environment

from .facts import Host
from .state import Entry, State


class Status(str, Enum):
    OK = "ok"
    ABSENT = "absent"
    DANGLING = "dangling"
    HIJACKED = "hijacked"
    STALE = "stale"
    CLOBBERED = "clobbered"
    EDITED = "edited"
    CONFLICT = "conflict"
    ORPHAN = "orphan"
    ERROR = "error"


# Higher wins when one action reports several problems at once.
SEVERITY = {
    Status.OK: 0,
    Status.ABSENT: 1,
    Status.STALE: 2,
    Status.DANGLING: 3,
    Status.HIJACKED: 4,
    Status.ORPHAN: 5,
    Status.CLOBBERED: 6,
    Status.EDITED: 7,
    Status.CONFLICT: 8,
    Status.ERROR: 9,
}

# Destructive to fix: something we did not write would be lost.
NEEDS_FORCE = {Status.CLOBBERED, Status.EDITED, Status.CONFLICT}


@dataclass(frozen=True)
class Drift:
    status: Status
    detail: str = ""

    @property
    def ok(self) -> bool:
        return self.status is Status.OK

    @property
    def needs_force(self) -> bool:
        return self.status in NEEDS_FORCE

    @property
    def fixable(self) -> bool:
        return self.status not in (Status.OK, Status.ERROR)


OK = Drift(Status.OK)


@dataclass
class Ctx:
    host: Host
    state: State
    env: Environment
    build_root: Path
    backup_root: Path
    module: str = ""
    dry_run: bool = False
    force: bool = False

    def for_module(self, name: str) -> Ctx:
        return replace(self, module=name)

    @property
    def build_dir(self) -> Path:
        return self.build_root / self.module

    def backup(self, path: Path) -> Path:
        """Move an unmanaged file aside before overwriting it."""
        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        rel = path.relative_to("/") if path.is_absolute() else Path(path.name)
        dest = self.backup_root / stamp / rel
        if not self.dry_run:
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(path), str(dest))
        return dest


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def _link_target(link: Path) -> Path:
    raw = Path(os.readlink(link))
    return raw if raw.is_absolute() else (link.parent / raw)


def _same_path(a: Path, b: Path) -> bool:
    return os.path.normpath(str(a)) == os.path.normpath(str(b))


class Action:
    kind = "action"

    def key(self) -> str:
        raise NotImplementedError

    def target(self) -> Path | None:
        return None

    def source(self) -> str | None:
        """Where the content comes from: a repo file, a template, a URL."""
        return None

    def check(self, ctx: Ctx) -> Drift:
        raise NotImplementedError

    def apply(self, ctx: Ctx) -> None:
        raise NotImplementedError

    def remove(self, ctx: Ctx) -> str:
        """Undo this action. Returns what actually happened, for reporting."""
        return "left in place"

    def describe(self) -> str:
        return self.key()

    def entry(self, ctx: Ctx) -> Entry:
        tgt = self.target()
        return Entry(
            module=ctx.module,
            kind=self.kind,
            key=self.key(),
            target=str(tgt) if tgt else None,
        )


@dataclass(frozen=True)
class Dir(Action):
    path: Path
    kind = "dir"

    def key(self) -> str:
        return f"dir:{self.path}"

    def target(self) -> Path:
        return self.path

    def describe(self) -> str:
        return f"mkdir -p {self.path}"

    def check(self, ctx: Ctx) -> Drift:
        if self.path.is_dir():
            return OK
        if self.path.exists() or self.path.is_symlink():
            return Drift(Status.CLOBBERED, "exists and is not a directory")
        return Drift(Status.ABSENT)

    def apply(self, ctx: Ctx) -> None:
        if self.path.exists() and not self.path.is_dir():
            ctx.backup(self.path)
        if not ctx.dry_run:
            self.path.mkdir(parents=True, exist_ok=True)

    def remove(self, ctx: Ctx) -> str:
        # Only reclaim directories we left empty; never recurse.
        if not (self.path.is_dir() and not any(self.path.iterdir())):
            return "left in place"
        if not ctx.dry_run:
            self.path.rmdir()
        return "rmdir"


def _symlink_drift(src: Path, dst: Path, *, require_src: bool = True) -> Drift:
    if not dst.is_symlink() and not dst.exists():
        return Drift(Status.ABSENT)
    if not dst.is_symlink():
        what = "directory" if dst.is_dir() else "regular file"
        return Drift(Status.CLOBBERED, f"{what} in the way")
    actual = _link_target(dst)
    if not _same_path(actual, src):
        broken = "" if actual.exists() else " (broken)"
        return Drift(Status.HIJACKED, f"-> {actual}{broken}")
    if require_src and not src.exists():
        return Drift(Status.DANGLING, f"-> {src}")
    return OK


def _place_symlink(src: Path, dst: Path, ctx: Ctx) -> None:
    if ctx.dry_run:
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.is_symlink() or dst.exists():
        if dst.is_symlink():
            dst.unlink()
        elif dst.is_dir():
            ctx.backup(dst)
        else:
            ctx.backup(dst)
    dst.symlink_to(src)


@dataclass(frozen=True)
class Symlink(Action):
    src: Path
    dst: Path
    kind = "symlink"

    def key(self) -> str:
        return f"symlink:{self.dst}"

    def target(self) -> Path:
        return self.dst

    def source(self) -> str:
        return str(self.src)

    def describe(self) -> str:
        return f"ln -s {self.src} {self.dst}"

    def check(self, ctx: Ctx) -> Drift:
        return _symlink_drift(self.src, self.dst)

    def apply(self, ctx: Ctx) -> None:
        _place_symlink(self.src, self.dst, ctx)

    def remove(self, ctx: Ctx) -> str:
        if not (self.dst.is_symlink() and _same_path(_link_target(self.dst), self.src)):
            return "left in place"
        if not ctx.dry_run:
            self.dst.unlink()
        return "unlinked"

    def entry(self, ctx: Ctx) -> Entry:
        e = super().entry(ctx)
        e.meta = {"src": str(self.src)}
        return e


def _build_path(ctx: Ctx, dst: Path) -> Path:
    """Mirror the destination under the build dir, keeping paths collision-free."""
    try:
        rel = dst.relative_to(ctx.host.home)
        return ctx.build_dir / "home" / rel
    except ValueError:
        return ctx.build_dir / "root" / Path(str(dst)).relative_to("/")


@dataclass(frozen=True)
class Render(Action):
    template: Path
    dst: Path
    mode: str = "symlink"  # or "copy"
    extra: tuple = ()
    kind = "render"

    def __init__(
        self,
        template: Path | str,
        dst: Path,
        mode: str = "symlink",
        extra: Mapping[str, Any] | None = None,
    ) -> None:
        object.__setattr__(self, "template", Path(template))
        object.__setattr__(self, "dst", dst)
        object.__setattr__(self, "mode", mode)
        object.__setattr__(self, "extra", tuple(sorted((extra or {}).items())))

    def key(self) -> str:
        return f"render:{self.dst}"

    def target(self) -> Path:
        return self.dst

    def source(self) -> str:
        return str(self.template)

    def describe(self) -> str:
        return f"render {self.template.name} -> {self.dst} ({self.mode})"

    def content(self, ctx: Ctx) -> str:
        from .render import render_template

        return render_template(ctx.env, self.template, dict(self.extra))

    def check(self, ctx: Ctx) -> Drift:
        try:
            content = self.content(ctx)
        except Exception as exc:  # template bugs must surface, not crash the run
            return Drift(Status.ERROR, f"{type(exc).__name__}: {exc}")
        if self.mode == "symlink":
            return self._check_symlink(ctx, content)
        return self._check_copy(ctx, content)

    def _check_symlink(self, ctx: Ctx, content: str) -> Drift:
        build = _build_path(ctx, self.dst)
        # require_src=False: a missing build file means "not rendered yet",
        # which is staleness, not a broken link.
        link = _symlink_drift(build, self.dst, require_src=False)
        if not link.ok:
            return link
        if not (build.is_file() and build.read_text() == content):
            return Drift(Status.STALE, "template output changed")
        return OK

    def _check_copy(self, ctx: Ctx, content: str) -> Drift:
        if not self.dst.exists():
            return Drift(Status.ABSENT)
        if self.dst.is_dir():
            return Drift(Status.CLOBBERED, "directory in the way")
        on_disk = sha256_file(self.dst)
        fresh = sha256_bytes(content.encode())
        if on_disk == fresh:
            return OK
        entry = ctx.state.get(ctx.module, self.key())
        last = entry.hash if entry else None
        if last is None:
            return Drift(Status.EDITED, "untracked file already at destination")
        if on_disk == last:
            return Drift(Status.STALE, "template output changed")
        if fresh == last:
            return Drift(Status.EDITED, "edited by hand since last apply")
        return Drift(Status.CONFLICT, "both the template and the file changed")

    def apply(self, ctx: Ctx) -> None:
        content = self.content(ctx)
        if self.mode == "symlink":
            build = _build_path(ctx, self.dst)
            if not ctx.dry_run:
                build.parent.mkdir(parents=True, exist_ok=True)
                build.write_text(content)
            _place_symlink(build, self.dst, ctx)
            return
        if self.dst.exists() and not self.dst.is_symlink():
            drift = self.check(ctx)
            if drift.needs_force:
                ctx.backup(self.dst)
        if not ctx.dry_run:
            self.dst.parent.mkdir(parents=True, exist_ok=True)
            self.dst.write_text(content)

    def remove(self, ctx: Ctx) -> str:
        build = _build_path(ctx, self.dst)
        if self.mode == "symlink":
            ours = self.dst.is_symlink() and _same_path(_link_target(self.dst), build)
            if not ours:
                return "left in place"
            if not ctx.dry_run:
                self.dst.unlink()
                build.unlink(missing_ok=True)
            return "unlinked"
        if not self.dst.is_file():
            return "left in place"
        if not ctx.dry_run:
            self.dst.unlink()
        return "deleted"

    def entry(self, ctx: Ctx) -> Entry:
        e = super().entry(ctx)
        e.meta = {"template": str(self.template), "mode": self.mode}
        try:
            e.hash = sha256_bytes(self.content(ctx).encode())
        except Exception:
            e.hash = None
        return e


@dataclass(frozen=True)
class Fetch(Action):
    url: str
    dst: Path
    sha256: str | None = None
    kind = "fetch"

    def key(self) -> str:
        return f"fetch:{self.dst}"

    def target(self) -> Path:
        return self.dst

    def source(self) -> str:
        return self.url

    def describe(self) -> str:
        return f"fetch {self.url} -> {self.dst}"

    def check(self, ctx: Ctx) -> Drift:
        if not self.dst.is_file():
            return Drift(Status.ABSENT)
        if self.sha256 and sha256_file(self.dst) != self.sha256:
            return Drift(Status.STALE, "checksum mismatch")
        return OK

    def apply(self, ctx: Ctx) -> None:
        if ctx.dry_run:
            return
        self.dst.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.dst.with_suffix(self.dst.suffix + ".part")
        with urllib.request.urlopen(self.url) as resp, tmp.open("wb") as out:
            shutil.copyfileobj(resp, out)
        if self.sha256:
            got = sha256_file(tmp)
            if got != self.sha256:
                tmp.unlink(missing_ok=True)
                raise RuntimeError(f"checksum mismatch for {self.url}: {got}")
        tmp.replace(self.dst)

    def remove(self, ctx: Ctx) -> str:
        if not self.dst.exists():
            return "left in place"
        if not ctx.dry_run:
            self.dst.unlink()
        return "deleted"

    def entry(self, ctx: Ctx) -> Entry:
        e = super().entry(ctx)
        e.meta = {"url": self.url}
        if self.dst.is_file():
            e.hash = sha256_file(self.dst)
        return e


@dataclass(frozen=True)
class Run(Action):
    cmd: tuple[str, ...]
    unless: Path | None = None
    desc: str = ""
    cwd: Path | None = None
    kind = "run"

    def key(self) -> str:
        return f"run:{self.unless or ' '.join(self.cmd)}"

    def target(self) -> Path | None:
        return self.unless

    def source(self) -> str:
        return " ".join(self.cmd)

    def describe(self) -> str:
        return self.desc or " ".join(self.cmd)

    def check(self, ctx: Ctx) -> Drift:
        if self.unless is None:
            return Drift(Status.ERROR, "Run needs an `unless` predicate to be idempotent")
        return OK if self.unless.exists() else Drift(Status.ABSENT)

    def apply(self, ctx: Ctx) -> None:
        if ctx.dry_run:
            return
        subprocess.run(list(self.cmd), cwd=self.cwd, check=True)
