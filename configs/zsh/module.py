from dot import Dir, Module, Render, Symlink


class Zsh(Module):
    name = "zsh"
    requires = ("zsh", "starship", "atuin")
    # Nothing depends on zsh, but its fragments reference tools the other
    # modules put in place, so it reads better applied last.
    after = ("claude", "codex", "nushell", "nvim", "term", "tools", "vcs", "zellij")

    def plan(self, host):
        zdir = host.xdg_config / "zsh"
        yield Dir(zdir)

        # Rendered: these are the files that used to be duplicated per OS.
        yield Render(self.path("templates/main.zsh.j2"), host.home / ".zshrc")
        yield Render(self.path("templates/01-env.zsh.j2"), zdir / "01-env.zsh")
        yield Render(self.path("templates/07-platform.zsh.j2"), zdir / "07-platform.zsh")

        # Symlinked: edited often, and identical everywhere. Globbed so a new
        # fragment lands in the plan without touching this file.
        for fragment in sorted(self.path("common").glob("*.zsh")):
            yield Symlink(fragment, zdir / fragment.name)

        # Vendored plugins and theme, linked as one tree.
        yield Symlink(self.path("plugins"), zdir / "plugins")

        yield Dir(host.xdg_config / "atuin")
        yield Symlink(self.path("atuin.toml"), host.xdg_config / "atuin" / "config.toml")
        yield Symlink(self.path("starship.toml"), host.xdg_config / "starship.toml")

        yield Dir(host.home / "scripts")
        for script in sorted(p for p in self.path("scripts").iterdir() if p.is_file()):
            yield Symlink(script, host.home / "scripts" / script.name)
