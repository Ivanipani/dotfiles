import importlib.util

from conftest import REPO

spec = importlib.util.spec_from_file_location("rex_procs", REPO / "configs" / "rex" / "bin" / "rex-procs.py")
rp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rp)

PS = """\
  100     1  2048  0.5    01:00 -zsh
  101   100 20480  1.5 01:02:03 nvim --embed .
  102   101  1024  0.0    00:10 rust-analyzer
  200     1  4096  0.0    05:00 unrelated daemon
"""


def procs():
    return rp.parse_ps(PS)


def test_parse_ps_keeps_the_full_command_with_spaces():
    # GIVEN ps output / WHEN parsed / THEN args keep their spaces
    assert procs()[1].args == "nvim --embed ."


def test_parse_ps_reads_resident_kb_as_int():
    assert procs()[1].rss_kb == 20480


def test_parse_ps_skips_short_lines():
    assert rp.parse_ps("garbage\n") == []


def test_descendants_includes_grandchildren_of_a_root():
    assert 102 in rp.descendants(procs(), {100: "s"})


def test_descendants_excludes_processes_outside_the_root():
    assert 200 not in rp.descendants(procs(), {100: "s"})


def test_ancestors_includes_every_parent_up_to_init():
    assert rp.ancestors(procs(), 102) == {102, 101, 100, 1} - {1}


def test_build_rows_sorts_biggest_memory_first():
    rows = rp.build_rows(procs(), {100: "s"}, set())
    assert [r.proc.pid for r in rows] == [101, 100, 102]


def test_build_rows_drops_excluded_pids():
    rows = rp.build_rows(procs(), {100: "s"}, {101})
    assert 101 not in [r.proc.pid for r in rows]


def test_build_rows_labels_rows_with_their_root_session():
    rows = rp.build_rows(procs(), {100: "work", 200: "other"}, set())
    assert {r.proc.pid: r.session for r in rows}[200] == "other"


def test_render_leads_each_process_line_with_its_pid_field():
    lines = rp.render(rp.build_rows(procs(), {100: "s"}, set())).splitlines()
    assert lines[2].split("\t")[0] == "101"


def test_render_summarises_total_resident_memory_in_the_first_line():
    first = rp.render(rp.build_rows(procs(), {100: "s"}, set())).splitlines()[0]
    assert "3 processes · 23M resident" in first
