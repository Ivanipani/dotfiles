from dot import Dir, Module, Render, Symlink


def shared_links(module, root, instructions):
    yield Dir(root)
    yield Symlink(module.path("AGENTS.md"), root / instructions)
    yield Symlink(module.path("hooks"), root / "hooks")


class Claude(Module):
    name = "claude"
    requires = ("jj", "jq")

    def plan(self, host):
        root = host.home / ".claude"
        yield from shared_links(self, root, "CLAUDE.md")
        yield Symlink(self.path("skills"), root / "skills")
        yield Symlink(self.path("agents"), root / "agents")
        # Symlinked so edits from the ccstatusline TUI land in the repo.
        yield Dir(host.home / ".config/ccstatusline")
        yield Symlink(
            self.path("ccstatusline/settings.json"),
            host.home / ".config/ccstatusline/settings.json",
        )
        # Claude rewrites settings; copy mode protects local edits.
        yield Render(
            self.path("templates/settings.json.j2"),
            root / "settings.json",
            mode="copy",
            extra={"client": "claude"},
        )


class Codex(Module):
    name = "codex"
    requires = ("jj", "jq")

    def plan(self, host):
        root = host.home / ".codex"
        yield from shared_links(self, root, "AGENTS.md")
        yield Dir(host.home / ".agents")
        yield Symlink(self.path("skills"), host.home / ".agents" / "skills")
        yield Render(
            self.path("templates/codex-hooks.json.j2"),
            root / "hooks.json",
            extra={"client": "codex"},
        )
        # Separate file: Codex appends its own approvals to default.rules.
        yield Render(self.path("templates/codex.rules.j2"), root / "rules" / "dot.rules")
