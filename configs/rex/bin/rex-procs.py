#!/usr/bin/env python3
"""rex-procs: see and stop the processes running under your Rex panes."""

import argparse
import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
from dataclasses import dataclass

EPILOG = """\
examples:
  rex-procs                 browse this session's processes (fzf)
  rex-procs --all           browse every session's processes
  rex-procs list --all      print the table and exit
  rex-procs kill TERM 4242  send SIGTERM to a pid

keys (browser):
  tab        mark / unmark         ctrl-k  stop (SIGTERM)    ctrl-x  force kill (SIGKILL)
  ctrl-r     refresh               ctrl-a  all sessions      ctrl-s  this session
  esc        quit
"""

APP_REX = "/Applications/Rex Beta.app/Contents/Helpers/Rex Server.rex/Contents/MacOS/rex"


@dataclass(frozen=True)
class Proc:
    pid: int
    ppid: int
    rss_kb: int
    cpu: float
    etime: str
    args: str


@dataclass(frozen=True)
class Row:
    proc: Proc
    session: str


def parse_ps(text):
    """Parse `ps -axo pid=,ppid=,rss=,pcpu=,etime=,args=` output."""
    procs = []
    for line in text.splitlines():
        parts = line.split(None, 5)
        if len(parts) < 6:
            continue
        pid, ppid, rss, cpu, etime, args = parts
        procs.append(Proc(int(pid), int(ppid), int(rss), float(cpu), etime, args))
    return procs


def descendants(procs, roots):
    """Map every process at or below a root pid to that root."""
    children = {}
    for p in procs:
        children.setdefault(p.ppid, []).append(p.pid)
    owner = {}
    for root in roots:
        stack = [root]
        while stack:
            pid = stack.pop()
            if pid in owner:
                continue
            owner[pid] = root
            stack.extend(children.get(pid, []))
    return owner


def ancestors(procs, pid):
    """The pid and every parent above it."""
    parent = {p.pid: p.ppid for p in procs}
    chain = set()
    while pid in parent and pid not in chain:
        chain.add(pid)
        pid = parent[pid]
    return chain


def build_rows(procs, root_sessions, exclude):
    """Rows for processes under the roots, minus `exclude`, biggest memory first."""
    owner = descendants(procs, root_sessions)
    rows = [Row(p, root_sessions[owner[p.pid]]) for p in procs if p.pid in owner and p.pid not in exclude]
    return sorted(rows, key=lambda r: r.proc.rss_kb, reverse=True)


def mb(kb):
    return f"{kb / 1024:,.0f}M"


def render(rows):
    """Tab-separated lines for fzf: pid, then the display text. Two header lines first."""
    total = sum(r.proc.rss_kb for r in rows)
    out = [f"-\t{len(rows)} processes · {mb(total)} resident", "-\t" + _line("SESSION", "PID", "MEM", "CPU%", "TIME", "COMMAND")]
    for r in rows:
        p = r.proc
        out.append(f"{p.pid}\t" + _line(r.session, p.pid, mb(p.rss_kb), f"{p.cpu:.1f}", p.etime, p.args))
    return "\n".join(out)


def _line(session, pid, mem, cpu, etime, cmd):
    return f"{str(session)[:22]:<22} {pid:>7} {mem:>8} {cpu:>6} {etime:>11}  {cmd}"


def rex_bin():
    return shutil.which("rex") or APP_REX


def rex_json(*args):
    out = subprocess.run([rex_bin(), "--autostart=false", *args], capture_output=True, text=True)
    return json.loads(out.stdout) if out.returncode == 0 and out.stdout.strip() else None


def root_pids(all_sessions):
    """{shell pid: session label} for the terminal panes in scope."""
    sessions = (rex_json("session", "list", "--json") or {}).get("sessions", [])
    if not all_sessions:
        sessions = [s for s in sessions if s["session_id"] == os.environ.get("REX_SESSION")]
    roots = {}
    for s in sessions:
        blocks = (rex_json("block", "ls", "-s", s["session_id"], "--json") or {}).get("blocks", [])
        for b in blocks:
            info = rex_json("block", "call", "-s", s["session_id"], "-b", b["block_id"], "process", "{}")
            pid = ((info or {}).get("child") or {}).get("pid")
            if pid:
                roots[pid] = s["label"]
    return roots


def read_ps():
    cmd = ["ps", "-axo", "pid=,ppid=,rss=,pcpu=,etime=,args="]
    return parse_ps(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)


def list_rows(all_sessions):
    procs = read_ps()
    exclude = ancestors(procs, os.getpid()) | set(descendants(procs, {os.getpid(): ""}))
    return build_rows(procs, root_pids(all_sessions), exclude)


def cmd_list(args):
    print(render(list_rows(args.all)))


def cmd_kill(args):
    sig = getattr(signal, "SIG" + args.signal.upper().removeprefix("SIG"), None)
    if sig is None:
        sys.exit(f"rex-procs: unknown signal {args.signal}")
    for pid in args.pids:
        if pid.isdigit():
            try:
                os.kill(int(pid), sig)
            except ProcessLookupError:
                pass


def cmd_show(args):
    subprocess.run(["ps", "-o", "pid,ppid,%cpu,%mem,rss,etime,lstart,command", "-p", args.pid])
    cwd = subprocess.run(["lsof", "-a", "-p", args.pid, "-d", "cwd", "-Fn"], capture_output=True, text=True).stdout
    print("\ncwd:", next((l[1:] for l in cwd.splitlines() if l.startswith("n")), "?"))
    tree = subprocess.run(["ps", "-o", "pid,rss,command", "-g", args.pid], capture_output=True, text=True).stdout
    print("\n" + tree)


def cmd_ui(args):
    me = shlex.quote(os.path.abspath(__file__))
    scope = "--all" if args.all else ""
    reload_all, reload_here = f"{me} list --all", f"{me} list"
    reload_cur = f"{me} list {scope}".strip()
    refresh = f"reload({reload_cur})"
    kill = lambda sig: f"execute-silent({me} kill {sig} {{+1}})+{refresh}"
    fzf = [
        "fzf", "--multi", "--ansi", "--delimiter=\t", "--with-nth=2..", "--header-lines=2",
        "--layout=reverse", "--no-sort", "--prompt=filter> ", "--info=inline-right",
        "--preview", f"{me} show {{1}}", "--preview-window=down,40%,wrap",
        "--bind", f"ctrl-k:{kill('TERM')}", "--bind", f"ctrl-x:{kill('KILL')}",
        "--bind", f"ctrl-r:{refresh}", "--bind", "enter:ignore",
        "--bind", f"ctrl-a:reload({reload_all})", "--bind", f"ctrl-s:reload({reload_here})",
        "--border=rounded", "--border-label= rex-procs · tab mark · ^k stop · ^x kill · ^r refresh · ^a all · esc quit ",
    ]
    feed = subprocess.Popen(fzf, stdin=subprocess.PIPE, text=True)
    feed.communicate(render(list_rows(args.all)) + "\n")


def parser():
    p = argparse.ArgumentParser(
        prog="rex-procs",
        description="Browse the processes running under your Rex panes, with memory and CPU, and stop them.",
        epilog=EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("-a", "--all", action="store_true", help="include every Rex session, not just this one")
    sub = p.add_subparsers(dest="command", metavar="COMMAND")
    ls = sub.add_parser("list", help="print the process table and exit")
    ls.add_argument("-a", "--all", action="store_true", help="include every Rex session")
    ls.set_defaults(run=cmd_list)
    kill = sub.add_parser("kill", help="send a signal to pids (used by the browser)")
    kill.add_argument("signal", help="TERM, KILL, ...")
    kill.add_argument("pids", nargs="+")
    kill.set_defaults(run=cmd_kill)
    show = sub.add_parser("show", help="describe one pid (used by the browser preview)")
    show.add_argument("pid")
    show.set_defaults(run=cmd_show)
    p.set_defaults(run=cmd_ui)
    return p


def main(argv=None):
    args = parser().parse_args(argv)
    args.run(args)


if __name__ == "__main__":
    main()
