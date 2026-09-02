from dot import Dir, Module, Render, Symlink, brew, cask


class Term(Module):
    name = "term"

    def enabled(self, host):
        # No terminal emulator config on a headless box.
        return host.macos and host.has("gui")

    def packages(self, host):
        yield cask("wezterm")
        yield cask("ghostty")
        yield brew("font-inconsolata-nerd-font")

    def plan(self, host):
        wez = host.xdg_config / "wezterm"
        ghostty = host.xdg_config / "ghostty"
        yield Dir(wez)
        yield Dir(ghostty)
        yield Render(self.path("templates/wezterm.lua.j2"), wez / "wezterm.lua")
        yield Symlink(self.path("ghostty/config"), ghostty / "config")
