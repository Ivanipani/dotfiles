from dot import Dir, Module, Symlink, brew, cargo, github

TILT = "v0.37.7"
PANTS = "v0.13.2"  # scie-pants launcher; each repo pins its own pants version
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
        "pants",
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
        # No package manager ships the pants launcher; it is a bare release binary.
        yield github(
            "pantsbuild/scie-pants",
            PANTS,
            f"scie-pants-{'macos' if host.macos else 'linux'}-{{arch}}",
            host=host,
            bin="pants",
        )

    def plan(self, host):
        # The CLI installs itself, so a fresh machine only needs `uv` and
        # one `./bin/dot apply` to become self-hosting.
        bindir = host.home / ".local" / "bin"
        yield Dir(bindir)
        yield Symlink(host.repo / "bin" / "dot", bindir / "dot")
