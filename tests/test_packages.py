from __future__ import annotations

import gzip
import hashlib
import tarfile
import zipfile
from pathlib import Path

import pytest
from conftest import make_host

from dot.packages import (
    Package,
    Release,
    apt,
    brew,
    cargo,
    github,
    install,
    probe,
    satisfies,
    uv_tool,
)


@pytest.fixture
def linux_host(repo, tmp_path):
    return make_host(repo, tmp_path / "home", os_name="linux")


def test_asset_template_resolves_host_facts(host, linux_host):
    mac = github("zellij-org/zellij", "v0.43.1", "zellij-{target}.tar.gz", host=host)
    assert mac.url == (
        "https://github.com/zellij-org/zellij/releases/download/v0.43.1/"
        "zellij-aarch64-apple-darwin.tar.gz"
    )
    assert mac.dest == host.home / ".local" / "bin"
    assert mac.provides == "zellij"

    lin = github("jj-vcs/jj", "v0.34.0", "{name}-{version}-{arch}-{os}.tgz", host=linux_host)
    assert lin.url.endswith("/download/v0.34.0/jj-0.34.0-aarch64-linux.tgz")


def test_bin_overrides_the_repo_name(host):
    pkg = github("BurntSushi/ripgrep", "14.1.1", "{name}.tar.gz", host=host, bin="rg")
    assert pkg.provides == "rg"
    assert pkg.url.endswith("/rg.tar.gz")


def _release(tmp_path: Path, blob: Path, **kw) -> Release:
    return Release(
        manager="github",
        name="acme/tool",
        bin="tool",
        url=blob.as_uri(),
        tag="v1",
        dest=tmp_path / "bin",
        **kw,
    )


def _payload(tmp_path: Path) -> Path:
    src = tmp_path / "src" / "tool"
    src.parent.mkdir(parents=True, exist_ok=True)
    src.write_text("#!/bin/sh\necho hi\n")
    return src


def test_fetch_unpacks_a_tarball(host, tmp_path):
    src = _payload(tmp_path)
    blob = tmp_path / "tool.tar.gz"
    with tarfile.open(blob, "w:gz") as tf:
        tf.add(src, arcname="tool-v1/tool")

    landed = _release(tmp_path, blob).fetch()
    assert landed == tmp_path / "bin" / "tool"
    assert landed.read_text() == src.read_text()
    assert landed.stat().st_mode & 0o111


def test_fetch_unpacks_a_zip_by_member(host, tmp_path):
    src = _payload(tmp_path)
    blob = tmp_path / "tool.zip"
    with zipfile.ZipFile(blob, "w") as zf:
        zf.write(src, "dist/tool")

    landed = _release(tmp_path, blob, member="dist/tool").fetch()
    assert landed.read_text() == src.read_text()


def test_fetch_gunzips_a_bare_binary(host, tmp_path):
    src = _payload(tmp_path)
    blob = tmp_path / "tool.gz"
    blob.write_bytes(gzip.compress(src.read_bytes()))

    assert _release(tmp_path, blob).fetch().read_text() == src.read_text()


def test_fetch_keeps_the_whole_tree_and_symlinks_the_binary(host, tmp_path):
    src = _payload(tmp_path)
    runtime = tmp_path / "src" / "runtime.vim"
    runtime.write_text("set nocompatible\n")
    blob = tmp_path / "tool.tar.gz"
    with tarfile.open(blob, "w:gz") as tf:
        tf.add(src, arcname="tool-v1/bin/tool")
        tf.add(runtime, arcname="tool-v1/share/runtime.vim")

    tree = tmp_path / "opt" / "tool"
    landed = _release(tmp_path, blob, member="tool-v1/bin/tool", tree=tree).fetch()
    assert landed == tmp_path / "bin" / "tool"
    assert landed.is_symlink() and landed.resolve() == tree / "bin" / "tool"
    assert (tree / "share" / "runtime.vim").read_text() == runtime.read_text()


def test_a_tree_install_replaces_the_old_tree(host, tmp_path):
    src = _payload(tmp_path)
    blob = tmp_path / "tool.tar.gz"
    with tarfile.open(blob, "w:gz") as tf:
        tf.add(src, arcname="tool-v1/bin/tool")

    tree = tmp_path / "opt" / "tool"
    (tree / "bin").mkdir(parents=True)
    (tree / "stale").write_text("from the last version")
    _release(tmp_path, blob, member="tool-v1/bin/tool", tree=tree).fetch()
    assert not (tree / "stale").exists()


def test_member_resolves_host_facts(host):
    pkg = github(
        "neovim/neovim", "v0.12.4", "{name}-{os}.tar.gz", host=host, member="{name}-{os}/bin/{name}"
    )
    assert pkg.member == "neovim-darwin/bin/neovim"


def test_checksum_mismatch_fails_the_install(host, tmp_path, capsys):
    src = _payload(tmp_path)
    blob = tmp_path / "tool.tar.gz"
    with tarfile.open(blob, "w:gz") as tf:
        tf.add(src, arcname="tool")
    digest = hashlib.sha256(blob.read_bytes()).hexdigest()

    assert install(_release(tmp_path, blob, sha256=digest)) == 0
    assert install(_release(tmp_path, blob, sha256="0" * 64)) == 1
    assert "checksum mismatch" in capsys.readouterr().err


def test_installed_looks_in_the_destination(host, tmp_path):
    pkg = github("acme/tool", "v1", "{name}.tar.gz", host=host, dest=tmp_path / "bin")
    assert not pkg.installed()
    pkg.dest.mkdir(parents=True)
    (pkg.dest / "tool").write_text("")
    assert pkg.installed()


# --------------------------------------------------------------------------- #
# Version pins


def test_each_manager_spells_the_pin_its_own_way():
    assert brew("node", version="20").describe() == "brew install node@20"
    assert apt("jq", version="1.7").describe() == "sudo apt install -y jq=1.7"
    assert uv_tool("ruff", version="0.16.5").describe() == "uv tool install ruff==0.16.5"
    assert (
        cargo("zellij", locked=True, version="0.43.1").describe()
        == "cargo install --locked zellij --version 0.43.1"
    )


def test_an_unpinned_package_installs_by_bare_name():
    assert brew("node").describe() == "brew install node"


def test_a_manager_that_cannot_pin_says_so():
    with pytest.raises(ValueError, match="cannot pin"):
        Package("rustup", "rust-analyzer", version="1.0").command()


@pytest.mark.parametrize(
    ("got", "want", "ok"),
    [
        ("0.43.1", "0.43.1", True),
        ("0.43.1", "v0.43.1", True),  # tags keep their leading v
        ("20.11.1", "20", True),  # a partial pin matches under itself
        ("20.11.1", "20.11", True),
        ("20.2.0", "20.11", False),
        ("2.11", "2.1", False),  # prefix, but not on a dot boundary
        ("0.43.0", "0.43.1", False),
    ],
)
def test_satisfies_compares_on_dot_boundaries(got, want, ok):
    assert satisfies(got, want) is ok


def _stub(path: Path, banner: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"#!/bin/sh\necho '{banner}'\n")
    path.chmod(0o755)
    return path


def test_probe_reads_the_version_banner(tmp_path):
    assert probe(str(_stub(tmp_path / "tool", "tool 1.2.3 (abc123)"))) == "1.2.3"
    assert probe(str(_stub(tmp_path / "quiet", "no numbers here"))) is None
    assert probe(str(tmp_path / "missing")) is None


def test_installed_holds_a_pinned_package_to_its_version(tmp_path, monkeypatch):
    _stub(tmp_path / "tool", "tool 1.2.3")
    monkeypatch.setenv("PATH", str(tmp_path))

    assert Package("brew", "tool").installed()
    assert Package("brew", "tool", version="1.2").installed()
    assert not Package("brew", "tool", version="1.3").installed()


def test_a_binary_that_will_not_say_its_version_is_left_alone(tmp_path, monkeypatch):
    _stub(tmp_path / "tool", "mystery build")
    monkeypatch.setenv("PATH", str(tmp_path))
    assert Package("brew", "tool", version="1.3").installed()


def test_a_release_reinstalls_when_the_tag_moves(host, tmp_path):
    pkg = github("acme/tool", "v1.2.3", "{name}.tar.gz", host=host, dest=tmp_path / "bin")
    assert pkg.version == "1.2.3"
    assert not pkg.installed()

    _stub(pkg.dest / "tool", "tool 1.2.3")
    assert pkg.installed()

    _stub(pkg.dest / "tool", "tool 1.2.2")
    assert not pkg.installed()
