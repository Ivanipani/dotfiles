"""Package declarations.

Kept out of the plan/apply loop on purpose: installs are slow, networked and
not reconcilable, so `dot install` is a separate verb from `dot apply`.
"""

from __future__ import annotations

import gzip
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from .facts import Host

# How each manager spells "install exactly this version". `rustup` is absent on
# purpose: a component has no version of its own, it has the toolchain's.
PINS: dict[str, Callable[[str, str], list[str]]] = {
    "brew": lambda name, version: [f"{name}@{version}"],
    "cask": lambda name, version: [f"{name}@{version}"],
    "apt": lambda name, version: [f"{name}={version}"],
    "cargo": lambda name, version: [name, "--version", version],
    "uv": lambda name, version: [f"{name}=={version}"],
}

# The first dotted number a `--version` banner prints, tag noise and all.
VERSION = re.compile(r"\d+(?:\.\d+)*(?:[-+][0-9A-Za-z.]+)?")


def probe(binary: str) -> str | None:
    """Best effort: the version `binary --version` reports, or None.

    stdin is closed on purpose. `pants --version` outside a pants repo asks a
    yes/no question, and a probe that can steal the terminal is worse than a
    probe that gives up.
    """
    try:
        out = subprocess.run(
            [binary, "--version"],
            capture_output=True,
            stdin=subprocess.DEVNULL,
            text=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    found = VERSION.search(out.stdout or out.stderr)
    return found.group(0) if found else None


def satisfies(got: str, want: str) -> bool:
    """Does `got` answer the pin `want`? A partial pin matches its own prefix.

    `brew install node@20` gives you 20.11.1, and that is the pin honoured, so
    a pin is satisfied by itself or by anything under it: 20 accepts 20.11.1,
    20.11 does not accept 20.2.
    """
    want = want.lstrip("v")
    return got == want or got.startswith(f"{want}.")


@dataclass(frozen=True)
class Package:
    manager: str
    name: str
    args: tuple[str, ...] = ()
    bin: str | None = None
    version: str | None = None

    @property
    def provides(self) -> str:
        return self.bin or self.name

    def command(self) -> list[str]:
        table = {
            "brew": ["brew", "install"],
            "cask": ["brew", "install", "--cask"],
            "apt": ["sudo", "apt", "install", "-y"],
            "cargo": ["cargo", "install"],
            "uv": ["uv", "tool", "install"],
            "rustup": ["rustup", "component", "add"],
        }
        base = table.get(self.manager)
        if base is None:
            raise ValueError(f"unknown package manager: {self.manager}")
        if self.version is None:
            return [*base, *self.args, self.name]
        pin = PINS.get(self.manager)
        if pin is None:
            raise ValueError(f"{self.manager} cannot pin a version: {self.name} {self.version}")
        return [*base, *self.args, *pin(self.name, self.version)]

    def describe(self) -> str:
        return " ".join(self.command())

    def current(self, binary: str) -> bool:
        """Is the binary at `binary` the version this package pins?

        An unpinned package only has to exist. A pinned one that will not say
        what it is counts as current: reinstalling on every run is worse than
        trusting a binary we cannot interrogate.
        """
        if self.version is None:
            return True
        got = probe(binary)
        return got is None or satisfies(got, self.version)

    def installed(self) -> bool:
        found = shutil.which(self.provides)
        return found is not None and self.current(found)

    def run(self) -> int:
        return subprocess.run(self.command()).returncode


def brew(name: str, bin: str | None = None, *, version: str | None = None) -> Package:
    return Package("brew", name, bin=bin, version=version)


def cask(name: str, bin: str | None = None, *, version: str | None = None) -> Package:
    return Package("cask", name, bin=bin, version=version)


def apt(name: str, bin: str | None = None, *, version: str | None = None) -> Package:
    return Package("apt", name, bin=bin, version=version)


def cargo(
    name: str,
    *,
    locked: bool = False,
    git: str | None = None,
    bin: str | None = None,
    version: str | None = None,
) -> Package:
    args: list[str] = []
    if locked:
        args.append("--locked")
    if git:
        args += ["--git", git]
    return Package("cargo", name, tuple(args), bin=bin, version=version)


def uv_tool(name: str, bin: str | None = None, *, version: str | None = None) -> Package:
    return Package("uv", name, bin=bin, version=version)


# No `version`: `rustup component add` installs whatever the toolchain carries.
def rustup(name: str, bin: str | None = None) -> Package:
    return Package("rustup", name, bin=bin)


# --------------------------------------------------------------------------- #
# GitHub releases

# platform.machine() spellings -> the token release assets are usually named with.
ARCHES = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}

# Rust target triples, the naming most release tarballs follow. musl over gnu:
# static builds run on whatever libc the machine happens to have.
TRIPLES = {"macos": "apple-darwin", "linux": "unknown-linux-musl"}


@dataclass(frozen=True)
class Release(Package):
    """A binary published as a GitHub release asset.

    No package manager owns it, so `dot install` downloads the asset itself,
    unpacks whatever archive it is, and drops the binary in ~/.local/bin.
    With `tree`, the unpacked prefix is kept and only symlinked onto PATH.
    """

    url: str = ""
    tag: str = ""
    dest: Path = field(default_factory=Path)
    member: str | None = None
    sha256: str | None = None
    tree: Path | None = None

    def command(self) -> list[str]:
        raise ValueError(f"{self.name} is a release asset, not a package manager install")

    def describe(self) -> str:
        return f"fetch {self.url} -> {self.tree or self.dest / self.provides}"

    def installed(self) -> bool:
        landed = self.dest / self.provides
        if landed.is_file():
            return self.current(str(landed))
        return super().installed()

    def run(self) -> int:
        try:
            self.fetch()
        except Exception as exc:
            print(f"{self.url}: {exc}", file=sys.stderr)
            return 1
        return 0

    def fetch(self) -> Path:
        """Download, unpack and install the binary. Returns where it landed."""
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            blob = work / self.url.rsplit("/", 1)[-1]
            with urllib.request.urlopen(self.url) as resp, blob.open("wb") as out:
                shutil.copyfileobj(resp, out)
            if self.sha256:
                got = hashlib.sha256(blob.read_bytes()).hexdigest()
                if got != self.sha256:
                    raise RuntimeError(f"checksum mismatch: {got}")

            binary = _unpack(blob, work / "unpacked", self.member or self.provides)
            self.dest.mkdir(parents=True, exist_ok=True)
            landed = self.dest / self.provides
            binary.chmod(0o755)
            if self.tree is None:
                shutil.move(str(binary), str(landed))
                return landed
            return self._graft(binary, landed, self.tree)

    def _graft(self, binary: Path, landed: Path, tree: Path) -> Path:
        """Keep the whole unpacked prefix; put a symlink to it on PATH.

        Some releases are a prefix, not a binary: nvim reads its runtime from
        ../share/nvim next to the executable, so the tree has to survive.
        """
        root = binary.parents[1] if binary.parent.name == "bin" else binary.parent
        inside = binary.relative_to(root)
        if tree.is_dir():
            shutil.rmtree(tree)
        tree.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(root), str(tree))
        landed.unlink(missing_ok=True)
        landed.symlink_to(tree / inside)
        return landed


def github(
    repo: str,
    tag: str,
    asset: str,
    *,
    host: Host,
    bin: str | None = None,
    member: str | None = None,
    sha256: str | None = None,
    dest: Path | None = None,
    tree: bool = False,
) -> Release:
    """A release asset from github.com/<repo>, installed into ~/.local/bin.

        yield github("zellij-org/zellij", "v0.43.1", "zellij-{target}.tar.gz", host=host)
        yield github("jj-vcs/jj", "v0.34.0", "jj-{tag}-{target}.tar.gz", host=host)

    `asset` and `member` are format strings over {name} {repo} {tag} {version}
    {arch} {os} {target}; `member` names the binary inside the archive when it
    is not just {name}. Anything tar, zip, gzip or a bare binary unpacks.

    `tree=True` is for releases that ship a prefix rather than a lone binary:
    the unpacked tree lives in ~/.local/share/dot/{name} and ~/.local/bin/{name}
    is a symlink into it.
    """
    name = bin or repo.split("/")[-1]
    arch = ARCHES.get(host.arch, host.arch)
    tokens = dict(
        name=name,
        repo=repo,
        tag=tag,
        version=tag.lstrip("v"),
        arch=arch,
        os="darwin" if host.macos else host.os,
        target=f"{arch}-{TRIPLES[host.os]}",
    )
    filename = asset.format(**tokens)
    return Release(
        manager="github",
        name=repo,
        bin=name,
        version=tag.lstrip("v"),
        url=f"https://github.com/{repo}/releases/download/{tag}/{filename}",
        tag=tag,
        dest=dest or host.home / ".local" / "bin",
        member=member.format(**tokens) if member else None,
        sha256=sha256,
        tree=host.xdg_data / "dot" / name if tree else None,
    )


def _unpack(blob: Path, into: Path, want: str) -> Path:
    """Extract `blob` and return the path of the binary `want` inside it."""
    into.mkdir(parents=True, exist_ok=True)
    if zipfile.is_zipfile(blob):
        with zipfile.ZipFile(blob) as zf:
            zf.extractall(into)
    elif tarfile.is_tarfile(blob):
        with tarfile.open(blob) as tf:
            # data_filter refuses members that would escape the directory.
            if hasattr(tarfile, "data_filter"):
                tf.extractall(into, filter="data")
            else:
                tf.extractall(into)
    elif blob.suffix == ".gz":
        plain = into / want
        plain.parent.mkdir(parents=True, exist_ok=True)
        with gzip.open(blob, "rb") as src, plain.open("wb") as out:
            shutil.copyfileobj(src, out)
        return plain
    else:
        return blob  # published as a bare binary

    direct = into / want
    if direct.is_file():
        return direct
    for found in sorted(into.rglob(want)):
        if found.is_file():
            return found
    raise FileNotFoundError(f"no {want} in {blob.name}")


def install(pkg: Package, *, dry_run: bool = False) -> int:
    if dry_run:
        return 0
    return pkg.run()
