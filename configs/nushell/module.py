from dot import Dir, Module, Render, Symlink, brew, cargo

SUBDIRS = ("scripts", "lib", "plugins", "autoload")


class Nushell(Module):
    name = "nushell"
    requires = ("nu",)

    def packages(self, host):
        yield brew("nushell", bin="nu") if host.macos else cargo("nu", locked=True)

    def config_homes(self, host):
        """Every directory nu might pick as its config home.

        macOS login shells get ~/Library/Application Support/nushell (no
        XDG_CONFIG_HOME), while a nu launched from zsh gets ~/.config/nushell.
        Both need the same files.
        """
        dirs = [host.xdg_config / "nushell"]
        if host.macos:
            dirs.insert(0, host.home / "Library" / "Application Support" / "nushell")
        return dirs

    def plan(self, host):
        for home in self.config_homes(host):
            yield Dir(home)
            for sub in SUBDIRS:
                yield Dir(home / sub)
            yield Render(self.path("templates/config.nu.j2"), home / "config.nu")
            yield Render(self.path("templates/login.nu.j2"), home / "login.nu")
            yield Symlink(self.path("env.nu"), home / "env.nu")
