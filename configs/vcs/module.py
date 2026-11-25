from dot import Dir, Module, Render, Symlink, apt, brew, cargo


class Vcs(Module):
    name = "vcs"
    requires = ("git", "jj", "difft")

    def packages(self, host):
        if host.macos:
            yield brew("difftastic", bin="difft")
            yield brew("jj")
            yield brew("lazygit")
        else:
            yield cargo("difftastic", locked=True, bin="difft")
            yield cargo("jj-cli", locked=True, bin="jj")
            yield apt("lazygit")
        yield cargo("lazyjj", locked=True)

    def plan(self, host):
        jj = host.xdg_config / "jj"
        jjui = host.xdg_config / "jjui"
        yield Dir(jj)
        yield Dir(jjui)
        yield Dir(jj / "conf.d")
        # Identity is the whole reason these are rendered: one repo, one
        # template, a different `user.email` per profile.
        yield Render(self.path("templates/jj-config.toml.j2"), jj / "config.toml")
        yield Symlink(self.path("jj/flight.toml"), jj / "conf.d" / "flight.toml")
        yield Symlink(self.path("jj/jjui-config.toml"), jjui / "config.toml")
        yield Render(self.path("templates/gitconfig.j2"), host.home / ".gitconfig")
        yield Symlink(self.path("git/gitignore_global"), host.home / ".gitignore_global")
