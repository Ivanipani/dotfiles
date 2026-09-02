"""Package declarations.

Kept out of the plan/apply loop on purpose: installs are slow, networked and
not reconcilable, so `dot install` is a separate verb from `dot apply`.
"""

from __future__ import annotations

import gzip
import hashlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

from .facts import Host


@dataclass(frozen=True)
class Package:
    manager: str
    name: str
    args: tuple[str, ...] = ()
    bin: str | None = None

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
        return [*base, *self.args, self.name]

    def describe(self) -> str:
        return " ".join(self.command())

    def installed(self) -> bool:
        return shutil.which(self.provides) is not None

    def run(self) -> int:
        return subprocess.run(self.command()).returncode


def brew(name: str, bin: str | None = None) -> Package:
    return Package("brew", name, bin=bin)


def cask(name: str, bin: str | None = None) -> Package:
    return Package("cask", name, bin=bin)


def apt(name: str, bin: str | None = None) -> Package:
    return Package("apt", name, bin=bin)


def cargo(
    name: str, *, locked: bool = False, git: str | None = None, bin: str | None = None
) -> Package:
    args: list[str] = []
    if locked:
        args.append("--locked")
    if git:
        args += ["--git", git]
    return Package("cargo", name, tuple(args), bin=bin)


def uv_tool(name: str, bin: str | None = None) -> Package:
    return Package("uv", name, bin=bin)


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
    """

    url: str = ""
    tag: str = ""
    dest: Path = field(default_factory=Path)
    member: str | None = None
    sha256: str | None = None

    def command(self) -> list[str]:
        raise ValueError(f"{self.name} is a release asset, not a package manager install")

    def describe(self) -> str:
        return f"fetch {self.url} -> {self.dest / self.provides}"

    def installed(self) -> bool:
        return (self.dest / self.provides).is_file() or super().installed()

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
            shutil.move(str(binary), str(landed))
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
) -> Release:
    """A release asset from github.com/<repo>, installed into ~/.local/bin.

        yield github("zellij-org/zellij", "v0.43.1", "zellij-{target}.tar.gz", host=host)
        yield github("jj-vcs/jj", "v0.34.0", "jj-{tag}-{target}.tar.gz", host=host)

    `asset` is a format string over {name} {repo} {tag} {version} {arch} {os}
    {target}; `member` names the binary inside the archive when it is not just
    {name}. Anything tar, zip, gzip or a bare binary unpacks.
    """
    name = bin or repo.split("/")[-1]
    arch = ARCHES.get(host.arch, host.arch)
    filename = asset.format(
        name=name,
        repo=repo,
        tag=tag,
        version=tag.lstrip("v"),
        arch=arch,
        os="darwin" if host.macos else host.os,
        target=f"{arch}-{TRIPLES[host.os]}",
    )
    return Release(
        manager="github",
        name=repo,
        bin=name,
        url=f"https://github.com/{repo}/releases/download/{tag}/{filename}",
        tag=tag,
        dest=dest or host.home / ".local" / "bin",
        member=member,
        sha256=sha256,
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
