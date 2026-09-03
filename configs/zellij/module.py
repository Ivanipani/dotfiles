from dot import Dir, Fetch, Module, Render, Symlink, cargo

PLUGINS = (
    (
        "vim-zellij-navigator",
        "https://github.com/hiasr/vim-zellij-navigator/releases/download/0.3.0/vim-zellij-navigator.wasm",
        "77e5a2f62f7c1a698caf2574493d5d75587e0fd877cfba34c48caf731c24c58d",
    ),
    (
        "zellij-attention",
        "https://github.com/KiryuuLight/zellij-attention/releases/download/v0.3.1/zellij-attention.wasm",
        "4209337ab61a731448ec733362f3ef699a905bbb5c0112a93a6d616360fd722f",
    ),
    (
        "zellij-sessionizer",
        "https://github.com/laperlej/zellij-sessionizer/releases/download/v0.4.3/zellij-sessionizer.wasm",
        "006b966ee457ed88bdb4f7594f30ca50b5e1dd72d4b389dabee89e0a29948334",
    ),
    (
        "zellij-switch",
        "https://github.com/mostafaqanbaryan/zellij-switch/releases/download/0.2.1/zellij-switch.wasm",
        "ef257e41ffeb73337ed1dead309942d14663d12d8f58170f0cdab5045b0a22ae",
    ),
    (
        "zjstatus",
        "https://github.com/dj95/zjstatus/releases/download/v0.25.0/zjstatus.wasm",
        "282ceab219e56e1908c9fac33907241fe6c3e1ef7d85faab3e5f438cef87b8fe",
    ),
)


class Zellij(Module):
    name = "zellij"
    # nu is not optional any more: the zjstatus widgets are nushell scripts.
    requires = ("zellij", "nu")

    def packages(self, host):
        yield cargo("zellij", locked=True, version="0.45.0")

    def plan(self, host):
        cfg = host.xdg_config / "zellij"
        yield Dir(cfg)
        yield Render(self.path("templates/config.kdl.j2"), cfg / "config.kdl")
        yield Symlink(self.path("layouts"), cfg / "layouts")
        # Checksums pin the exact plugin builds that are known to work here.
        for name, url, digest in PLUGINS:
            yield Fetch(url, self.path("plugins", f"{name}.wasm"), digest)
        yield Symlink(self.path("plugins"), cfg / "plugins")

        # On PATH rather than referenced by repo path: shells call these by
        # name, so a rename shows up as drift instead of failing at runtime.
        # Globbed so a new script lands in the plan without touching this file.
        yield Dir(host.home / "scripts")
        for script in sorted(p for p in self.path("scripts").iterdir() if p.is_file()):
            yield Symlink(script, host.home / "scripts" / script.name)
