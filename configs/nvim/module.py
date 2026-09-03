from dot import Module, Symlink, apt, brew, cargo, rustup, uv_tool


class Nvim(Module):
    name = "nvim"
    requires = ("nvim",)

    def packages(self, host):
        # Unpinned: homebrew-core ships no versioned neovim formula, so
        # `neovim@x.y.z` is not a thing brew can install.
        yield brew("neovim", bin="nvim") if host.macos else apt("neovim", bin="nvim")
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
