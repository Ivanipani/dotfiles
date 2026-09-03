from dot import Dir, Module, Render, Symlink


class Claude(Module):
    name = "claude"

    def plan(self, host):
        root = host.home / ".claude"
        yield Dir(root)
        yield Symlink(self.path("CLAUDE.md"), root / "CLAUDE.md")
        yield Symlink(self.path("skills"), root / "skills")
        yield Symlink(self.path("agents"), root / "agents")
        yield Symlink(self.path("hooks"), root / "hooks")
        # Claude Code rewrites this file itself — /config toggles, and the
        # auto-mode block it regenerates per repo — so it cannot be a symlink
        # into the repo. Copy mode hashes the content instead, which makes its
        # own writes show up as `edited` rather than being silently clobbered.
        # settings.local.json is deliberately left alone: it is the machine's
        # own layer, the same role ~/.config/dot/local.toml plays for vars.
        yield Render(self.path("templates/settings.json.j2"), root / "settings.json", mode="copy")
