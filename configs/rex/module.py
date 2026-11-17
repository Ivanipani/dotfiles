from dot import Dir, Module, Symlink


class Rex(Module):
    """Rex terminal multiplexer: modes and keybindings live in init.lua."""

    name = "rex"

    def enabled(self, host):
        return host.macos

    def plan(self, host):
        rex = host.xdg_config / "rex"
        yield Dir(rex)
        yield Symlink(self.path("init.lua"), rex / "init.lua")
