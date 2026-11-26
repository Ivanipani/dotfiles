from dot import Dir, Module, Render


class ConveyorBelt(Module):
    """Client settings of conveyor-belt (`cb`): which repos it owns `main` of.

    The agents' cb-guard hook reads the list. The default context and namespace
    (`CB_CONTEXT`, `CB_NAMESPACE`) come from the same vars, exported by the shell modules.
    """

    name = "conveyor-belt"

    def enabled(self, host):
        return bool(host.vars["conveyor_belt"]["repos"])

    def plan(self, host):
        config = host.xdg_config / "conveyor-belt"
        yield Dir(config)
        yield Render(self.path("templates/repos.j2"), config / "repos")
