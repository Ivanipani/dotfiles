from pathlib import Path

from dot import Action, Dir, Drift, Fetch, Module, Render, Run, Status, Symlink, cargo

# name, url, sha256, permissions the plugin asks for on load.
#
# The permissions matter because zellij only consults its cache *after* a plugin
# has asked: every zjstatus pane asks at once when a layout opens, so a machine
# with nothing cached prompts once per tab before the first answer is written
# back. Seeding the cache from here means it never asks at all.
PLUGINS = (
    (
        "vim-zellij-navigator",
        "https://github.com/hiasr/vim-zellij-navigator/releases/download/0.3.0/vim-zellij-navigator.wasm",
        "77e5a2f62f7c1a698caf2574493d5d75587e0fd877cfba34c48caf731c24c58d",
        ("ReadApplicationState", "ChangeApplicationState", "WriteToStdin"),
    ),
    (
        "zellij-attention",
        "https://github.com/KiryuuLight/zellij-attention/releases/download/v0.3.1/zellij-attention.wasm",
        "4209337ab61a731448ec733362f3ef699a905bbb5c0112a93a6d616360fd722f",
        (
            "ReadApplicationState",
            "ChangeApplicationState",
            "ReadCliPipes",
            "MessageAndLaunchOtherPlugins",
        ),
    ),
    (
        "zjstatus",
        "https://github.com/dj95/zjstatus/releases/download/v0.25.0/zjstatus.wasm",
        "282ceab219e56e1908c9fac33907241fe6c3e1ef7d85faab3e5f438cef87b8fe",
        ("ReadApplicationState", "ChangeApplicationState", "RunCommands"),
    ),
)

# Our own plugins (src/<name>): built from source rather than fetched, so they
# carry no url/checksum — just the permissions they ask for.
# zellij-sessionizer replaces laperlej/zellij-sessionizer and
# mostafaqanbaryan/zellij-switch, doing both jobs (and the fuzzy-finding fzf/fd
# used to do) natively. zellij-workspace manages sessions of jj workspaces.
LOCAL_PLUGINS = (
    (
        "zellij-sessionizer",
        ("ReadApplicationState", "ChangeApplicationState", "FullHdAccess"),
    ),
    (
        "zellij-workspace",
        ("ReadApplicationState", "ChangeApplicationState", "FullHdAccess", "RunCommands"),
    ),
)


def cache_dir(host):
    """Where zellij keeps `permissions.kdl`.

    Zellij locates it with the `directories` crate rather than XDG, so on macOS
    it lands under ~/Library/Caches instead of anywhere `host.xdg_cache` points.
    """
    if host.macos:
        return host.home / "Library" / "Caches" / "org.Zellij-Contributors.Zellij"
    return host.xdg_cache / "zellij"


def read_grants(path):
    """Parse zellij's permission cache: `"plugin" { Permission; ... }`, one per line."""
    grants = {}
    current = None
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("//"):
            continue
        if line.endswith("{"):
            current = grants.setdefault(line[:-1].strip().strip('"'), set())
        elif line == "}":
            current = None
        elif current is not None:
            current.add(line)
    return grants


def write_grants(path, grants):
    body = ""
    for plugin in sorted(grants):
        listed = "".join(f"    {p}\n" for p in sorted(grants[plugin]))
        body += f'"{plugin}" {{\n{listed}}}\n'
    path.write_text(body)


class Grant(Action):
    """Pre-grant the permissions zellij would otherwise prompt for.

    Not a Render, because zellij owns this file: it rewrites the whole thing
    from a hash map whenever any plugin is granted anything, so key order moves
    between runs and comments do not survive. Byte-exact is therefore the wrong
    question. What holds still is the content, so that is what this checks, and
    an apply merges into whatever is already there rather than replacing it.
    """

    kind = "grant"

    def __init__(self, path, wanted):
        self.path = path
        self.wanted = wanted

    def key(self):
        return f"grant:{self.path}"

    def target(self):
        return self.path

    def describe(self):
        return f"grant {len(self.wanted)} plugins their permissions in {self.path}"

    def missing(self):
        have = read_grants(self.path) if self.path.is_file() else {}
        return {
            plugin: sorted(set(perms) - have.get(plugin, set()))
            for plugin, perms in self.wanted.items()
            if set(perms) - have.get(plugin, set())
        }

    def check(self, ctx):
        if not self.path.is_file():
            return Drift(Status.ABSENT)
        missing = self.missing()
        if not missing:
            return Drift(Status.OK)
        detail = ", ".join(
            f"{Path(plugin).stem}: {' '.join(perms)}" for plugin, perms in sorted(missing.items())
        )
        return Drift(Status.STALE, f"would prompt for {detail}")

    def apply(self, ctx):
        if ctx.dry_run:
            return
        grants = read_grants(self.path) if self.path.is_file() else {}
        for plugin, perms in self.wanted.items():
            grants.setdefault(plugin, set()).update(perms)
        write_grants(self.path, grants)

    # No remove(): the base default leaves the file alone, which is right for a
    # cache we only ever add to — dropping the grants would just re-prompt.


class Zellij(Module):
    name = "zellij"
    # nu is not optional any more: the zjstatus widgets are nushell scripts.
    requires = ("zellij", "nu")

    def packages(self, host):
        yield cargo("zellij", locked=True, version="0.45.1")

    def plan(self, host):
        cfg = host.xdg_config / "zellij"
        yield Dir(cfg)
        yield Render(self.path("templates/config.kdl.j2"), cfg / "config.kdl")
        # One shared tab template feeds every layout, the workspace plugin's included.
        yield Dir(cfg / "layouts")
        for layout in sorted(self.path("templates", "layouts").glob("*.kdl.j2")):
            yield Render(layout, cfg / "layouts" / layout.name.removesuffix(".j2"))
        yield Render(self.path("templates/workspace-tab.kdl.j2"), cfg / "workspace-tab.kdl")
        # Checksums pin the exact plugin builds that are known to work here.
        for name, url, digest, _ in PLUGINS:
            yield Fetch(url, self.path("plugins", f"{name}.wasm"), digest)

        # Local plugins are built rather than fetched. `unless` makes this a
        # one-time build rather than a rebuild-on-every-apply: cargo's own
        # incremental cache does the rest during development. Requires the
        # wasm32-wasip1 rustup target (`rustup target add wasm32-wasip1`).
        for name, _ in LOCAL_PLUGINS:
            crate_dir = host.repo / "src" / name
            built_wasm = crate_dir / "target" / "wasm32-wasip1" / "release" / f"{name.replace('-', '_')}.wasm"
            yield Run(
                ("cargo", "build", "--release", "--target", "wasm32-wasip1"),
                unless=built_wasm,
                cwd=crate_dir,
                desc=f"cargo build {name} (wasm32-wasip1)",
            )
            yield Symlink(built_wasm, self.path("plugins", f"{name}.wasm"))

        yield Symlink(self.path("plugins"), cfg / "plugins")

        cache = cache_dir(host)
        yield Dir(cache)
        yield Grant(
            cache / "permissions.kdl",
            # Keyed by the path zellij resolves the plugin to, which is the
            # `location` in config.kdl with `file:` stripped.
            {str(cfg / "plugins" / f"{name}.wasm"): perms for name, _, _, perms in PLUGINS}
            | {str(cfg / "plugins" / f"{name}.wasm"): perms for name, perms in LOCAL_PLUGINS},
        )

        # On PATH rather than referenced by repo path: shells call these by
        # name, so a rename shows up as drift instead of failing at runtime.
        # Globbed so a new script lands in the plan without touching this file.
        yield Dir(host.home / "scripts")
        for script in sorted(p for p in self.path("scripts").iterdir() if p.is_file()):
            yield Symlink(script, host.home / "scripts" / script.name)
