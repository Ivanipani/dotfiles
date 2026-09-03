from dot import Module, Symlink, brew, cargo, github, rustup, uv_tool

NVIM = "v0.12.4"


class Nvim(Module):
    name = "nvim"
    requires = ("nvim",)

    def packages(self, host):
        # Straight from the release: brew has no versioned neovim formula and
        # apt's is always old. The tarball is a whole prefix, hence `tree`.
        arch = "arm64" if host.arch in ("arm64", "aarch64") else "x86_64"
        slug = f"{'macos' if host.macos else 'linux'}-{arch}"
        yield github(
            "neovim/neovim",
            NVIM,
            f"nvim-{slug}.tar.gz",
            host=host,
            bin="nvim",
            member=f"nvim-{slug}/bin/nvim",
            tree=True,
        )
        yield rustup("rust-analyzer")
        yield uv_tool("basedpyright")
        yield uv_tool("ruff")
        yield uv_tool("ty")
        yield cargo("stylua")
        if host.macos:
            yield brew("lua-language-server")
        yield cargo("nufmt", git="https://github.com/nushell/nufmt")

    def plan(self, host):
        # One symlink for the whole config tree: lazy.nvim owns what is inside.
        yield Symlink(self.path("config"), host.xdg_config / "nvim")
