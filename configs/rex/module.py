from dot import Dir, Module, Symlink


class Rex(Module):
    """Rex terminal multiplexer: modes and keybindings live in init.lua."""

    name = "rex"
    requires = ("nu", "jj", "fzf")

    def enabled(self, host):
        return host.macos

    def plan(self, host):
        rex = host.xdg_config / "rex"
        bindir = host.home / ".local" / "bin"
        yield Dir(rex)
        yield Symlink(self.path("init.lua"), rex / "init.lua")
        yield Dir(bindir)
        yield Symlink(self.path("scripts", "rex-workspace"), bindir / "rex-workspace")
