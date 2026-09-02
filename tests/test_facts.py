from conftest import make_host


def test_profile_selects_its_var_layer(repo, tmp_path):
    personal = make_host(repo, tmp_path / "a", profile="personal")
    work = make_host(repo, tmp_path / "b", profile="work")
    assert personal.profile == "personal"
    assert work.profile == "work"
    # Both currently resolve an email; the point is that the layer is consulted.
    assert personal.var("user.email")
    assert work.var("user.email")


def test_os_layer_overrides_default(repo, tmp_path):
    mac = make_host(repo, tmp_path / "m", os_name="macos")
    linux = make_host(repo, tmp_path / "l", os_name="linux")
    assert mac.var("shell.path_mode") == "replace"
    assert linux.var("shell.path_mode") == "prepend"
    assert mac.var("git.credential_helper") == "osxkeychain"
    assert mac.macos and linux.linux


def test_host_layer_wins_over_os(repo, tmp_path):
    # vars/host/ivans-macbook-pro.toml exists in the repo.
    h = make_host(repo, tmp_path / "h", hostname="ivans-macbook-pro")
    assert "laptop" in h.tags


def test_unknown_host_falls_back_cleanly(repo, tmp_path):
    h = make_host(repo, tmp_path / "u", hostname="does-not-exist")
    assert h.var("user.name")
    assert h.var("nothing.here.at.all", "fallback") == "fallback"


def test_xdg_defaults_follow_home(repo, tmp_path):
    h = make_host(repo, tmp_path / "x")
    assert h.xdg_config == h.home / ".config"
    assert h.xdg_state == h.home / ".local/state"
