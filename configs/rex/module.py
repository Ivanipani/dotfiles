import shutil
import subprocess

from dot import Action, Dir, Drift, Module, Status, Symlink
from dot.actions import sha256_file

APP_REX = "/Applications/Rex Beta.app/Contents/Helpers/Rex Server.rex/Contents/MacOS/rex"


def rex_bin():
    return shutil.which("rex") or APP_REX


class Reload(Action):
    """Make a running Rex server re-read init.lua.

    init.lua is symlinked, so the link never drifts when its content changes.
    What holds still is the file's hash: reload whenever it differs from the
    one recorded at the last successful reload.
    """

    kind = "reload"

    def __init__(self, config):
        self.config = config

    def key(self):
        return f"reload:{self.config}"

    def target(self):
        return self.config

    def describe(self):
        return f"reload Rex config {self.config}"

    def check(self, ctx):
        if not self.config.is_file():
            return Drift(Status.OK)
        entry = ctx.state.get(ctx.module, self.key())
        if entry and entry.hash == sha256_file(self.config):
            return Drift(Status.OK)
        return Drift(Status.STALE, "init.lua changed since last reload")

    def apply(self, ctx):
        if ctx.dry_run:
            return
        # --autostart=false: a stopped server reads init.lua when it starts anyway.
        subprocess.run([rex_bin(), "--autostart=false", "config", "reload"], check=True)

    def entry(self, ctx):
        e = super().entry(ctx)
        if self.config.is_file():
            e.hash = sha256_file(self.config)
        return e


class Rex(Module):
    """Rex terminal multiplexer: modes and keybindings live in init.lua."""

    name = "rex"
    requires = ("jj",)

    def enabled(self, host):
        return host.macos

    def plan(self, host):
        rex = host.xdg_config / "rex"
        bindir = host.home / ".local" / "bin"
        yield Dir(rex)
        yield Symlink(self.path("init.lua"), rex / "init.lua")
        yield Reload(rex / "init.lua")
        yield Dir(bindir)
        # Built by `task build` from src/rex-workspace.
        yield Symlink(host.repo / "src" / "rex-workspace" / "target" / "release" / "rex-workspace", bindir / "rex-workspace")
