import pytest

from dot.modules import Module, order, select


class Fake(Module):
    def __init__(self, name, after=()):
        self.name = name
        self.after = after
        self.dir = None


def test_order_respects_after():
    mods = [Fake("z", after=("a",)), Fake("a")]
    assert [m.name for m in order(mods)] == ["a", "z"]


def test_order_is_stable_for_independent_modules():
    mods = [Fake("c"), Fake("a"), Fake("b")]
    assert [m.name for m in order(mods)] == ["a", "b", "c"]


def test_order_rejects_a_cycle():
    with pytest.raises(ValueError, match="cycle"):
        order([Fake("a", after=("b",)), Fake("b", after=("a",))])


def test_order_ignores_dependencies_outside_the_selection():
    assert [m.name for m in order([Fake("a", after=("absent",))])] == ["a"]


def test_select_filters_and_validates():
    mods = [Fake("a"), Fake("b")]
    assert [m.name for m in select(mods, ["b"])] == ["b"]
    assert select(mods, None) == mods
    with pytest.raises(SystemExit, match="unknown module"):
        select(mods, ["nope"])


def test_every_repo_module_declares_a_name(modules):
    assert all(m.name for m in modules)
    assert len({m.name for m in modules}) == len(modules)


def test_every_repo_module_plans_something_or_installs_something(modules, host):
    for m in modules:
        if not m.enabled(host):
            continue
        assert list(m.plan(host)) or list(m.packages(host)), f"{m.name} does nothing"
