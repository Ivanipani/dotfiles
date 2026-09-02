from __future__ import annotations

import gzip
import hashlib
import tarfile
import zipfile
from pathlib import Path

import pytest
from conftest import make_host

from dot.packages import Release, github, install


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
