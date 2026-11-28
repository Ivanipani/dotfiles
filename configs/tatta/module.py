from dot import Dir, Module, Symlink


class Tatta(Module):
    """tatta user config: OpenBao address and where credentials come from."""

    name = "tatta"

    def plan(self, host):
        config = host.xdg_config / "tatta"
        yield Dir(config)
        yield Symlink(self.path("config.toml"), config / "config.toml")
