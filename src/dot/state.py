"""Applied-action ledger.

Purely a cache: every entry can be rederived by re-checking the plan against
disk, so a lost state file degrades orphan detection, never correctness.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path

VERSION = 1


@dataclass
class Entry:
    module: str
    kind: str
    key: str
    target: str | None = None
    hash: str | None = None
    meta: dict = field(default_factory=dict)

    @property
    def id(self) -> str:
        return f"{self.module}:{self.key}"


class State:
    def __init__(self, path: Path, fingerprint: str = "") -> None:
        self.path = path
        self.fingerprint = fingerprint
        self.entries: dict[str, Entry] = {}
        self.dirty = False

    @classmethod
    def load(cls, path: Path, fingerprint: str = "") -> State:
        state = cls(path, fingerprint)
        if not path.is_file():
            return state
        try:
            raw = json.loads(path.read_text())
        except (json.JSONDecodeError, OSError):
            return state
        if raw.get("version") != VERSION:
            return state
        state.fingerprint = raw.get("fingerprint", fingerprint)
        for eid, item in raw.get("entries", {}).items():
            state.entries[eid] = Entry(**item)
        return state

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "version": VERSION,
            "fingerprint": self.fingerprint,
            "entries": {eid: asdict(e) for eid, e in sorted(self.entries.items())},
        }
        tmp = self.path.with_suffix(".json.tmp")
        tmp.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
        tmp.replace(self.path)
        self.dirty = False

    def get(self, module: str, key: str) -> Entry | None:
        return self.entries.get(f"{module}:{key}")

    def record(self, entry: Entry) -> None:
        self.entries[entry.id] = entry
        self.dirty = True

    def forget(self, module: str, key: str) -> None:
        if self.entries.pop(f"{module}:{key}", None) is not None:
            self.dirty = True

    def orphans(self, live_ids: set[str], modules: set[str] | None = None) -> list[Entry]:
        """Entries we applied once that no current plan produces."""
        out = [e for eid, e in self.entries.items() if eid not in live_ids]
        if modules is not None:
            out = [e for e in out if e.module in modules]
        return sorted(out, key=lambda e: e.id)
