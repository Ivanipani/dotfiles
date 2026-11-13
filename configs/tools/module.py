from dot import Dir, Module, Symlink, brew, cargo, github

TILT = "v0.37.7"
JJUI = "v0.10.9"


class Tools(Module):
    """Standalone binaries with no config of their own."""

    name = "tools"
    requires = (
        "starship",
        "fzf",
        "atuin",
        "rg",
        "fd",
        "just",
        "direnv",
        "tilt",
        "jjui",
    )

    def packages(self, host):
        if host.macos:
            yield brew("starship")
            yield brew("fzf")
            yield brew("atuin")
            yield brew("just")
            yield brew("direnv")
            yield brew("tilt")
            yield brew("jjui")
        else:
            yield cargo("starship", locked=True)
            yield cargo("just")
            yield cargo("direnv")
            # Neither Tilt nor jjui uses the triple arch token; jjui wants
            # the Go spelling, Tilt a mix of the two.
            goarch = "arm64" if host.arch in ("arm64", "aarch64") else "amd64"
            arch = "arm64" if goarch == "arm64" else "x86_64"
            yield github(
                "tilt-dev/tilt",
                TILT,
                f"tilt.{{version}}.linux.{arch}.tar.gz",
                host=host,
                bin="tilt",
            )
            # jjui ships a zip whose only member carries the version and arch.
            yield github(
                "idursun/jjui",
                JJUI,
                f"jjui-{{version}}-linux-{goarch}.zip",
                host=host,
                member=f"jjui-{JJUI.lstrip('v')}-linux-{goarch}",
            )
        yield cargo("ripgrep", bin="rg")
        yield cargo("fd-find", bin="fd")

    def plan(self, host):
        # The CLI installs itself, so a fresh machine only needs `uv` and
        # one `./bin/dot apply` to become self-hosting.
        bindir = host.home / ".local" / "bin"
        yield Dir(bindir)
        yield Symlink(host.repo / "bin" / "dot", bindir / "dot")
        # Built by `task build` (dot doesn't build Rust CLIs); link
        # it once it exists so a fresh `apply` doesn't create a dangling link.
        repo_sync = host.repo / "src" / "repo-sync" / "target" / "release" / "repo-sync"
        if repo_sync.exists():
            yield Symlink(repo_sync, bindir / "repo-sync")
