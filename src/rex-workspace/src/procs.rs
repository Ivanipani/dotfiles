//! Processes running under Rex panes: collection and signalling.

use crate::rex::Rex;
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub rss_kb: u64,
    pub cpu: f32,
    pub etime: String,
    pub args: String,
}

/// A process and the Rex session whose pane it runs under.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub proc: Proc,
    pub session: String,
}

/// Parse `ps -axo pid=,ppid=,rss=,pcpu=,etime=,args=`; malformed lines are skipped.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines().filter_map(parse_line).collect()
}

fn token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace)?;
    Some((&s[..end], s[end..].trim_start()))
}

fn parse_line(line: &str) -> Option<Proc> {
    let (pid, rest) = token(line)?;
    let (ppid, rest) = token(rest)?;
    let (rss, rest) = token(rest)?;
    let (cpu, rest) = token(rest)?;
    let (etime, args) = token(rest)?;
    Some(Proc {
        pid: pid.parse().ok()?,
        ppid: ppid.parse().ok()?,
        rss_kb: rss.parse().ok()?,
        cpu: cpu.parse().ok()?,
        etime: etime.into(),
        args: args.into(),
    })
}

/// Every process at or below a root, mapped to that root's session label.
pub fn descendants(procs: &[Proc], roots: &HashMap<u32, String>) -> HashMap<u32, String> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p.pid);
    }
    let mut owner = HashMap::new();
    for (&root, session) in roots {
        let mut stack = vec![root];
        while let Some(pid) = stack.pop() {
            if owner.insert(pid, session.clone()).is_none() {
                stack.extend(children.get(&pid).into_iter().flatten());
            }
        }
    }
    owner
}

/// The pid and every parent above it.
pub fn ancestors(procs: &[Proc], pid: u32) -> HashSet<u32> {
    let parent: HashMap<_, _> = procs.iter().map(|p| (p.pid, p.ppid)).collect();
    let mut chain = HashSet::new();
    let mut at = pid;
    while chain.insert(at) {
        match parent.get(&at) {
            Some(&up) => at = up,
            None => break,
        }
    }
    chain
}

/// Rows under the roots minus `exclude`, biggest memory first.
pub fn build_rows(procs: &[Proc], roots: &HashMap<u32, String>, exclude: &HashSet<u32>) -> Vec<Row> {
    let owner = descendants(procs, roots);
    let mut rows: Vec<_> = procs
        .iter()
        .filter(|p| !exclude.contains(&p.pid))
        .filter_map(|p| Some(Row { proc: p.clone(), session: owner.get(&p.pid)?.clone() }))
        .collect();
    rows.sort_by(|a, b| b.proc.rss_kb.cmp(&a.proc.rss_kb));
    rows
}

fn read_ps() -> Result<Vec<Proc>> {
    let out = Command::new("ps").args(["-axo", "pid=,ppid=,rss=,pcpu=,etime=,args="]).output()?;
    if !out.status.success() {
        bail!("ps failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(parse_ps(&String::from_utf8_lossy(&out.stdout)))
}

/// Processes under the panes of every session (or only `session`), excluding this program's own tree.
pub fn collect(rex: &Rex, session: Option<&str>) -> Result<Vec<Row>> {
    let procs = read_ps()?;
    let roots: HashMap<_, _> = rex.pane_pids(session).into_iter().map(|(label, pid)| (pid, label)).collect();
    let me = std::process::id();
    let mut exclude = ancestors(&procs, me);
    exclude.extend(descendants(&procs, &HashMap::from([(me, String::new())])).into_keys());
    Ok(build_rows(&procs, &roots, &exclude))
}

/// Send `signal` (`TERM`, `KILL`) to `pid`.
pub fn signal(signal: &str, pid: u32) -> Result<()> {
    let out = Command::new("kill").arg(format!("-{signal}")).arg(pid.to_string()).output()?;
    if !out.status.success() {
        bail!("kill -{signal} {pid}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub fn mem(kb: u64) -> String {
    match kb {
        0..=1023 => format!("{kb}K"),
        1024..=1_048_575 => format!("{}M", kb / 1024),
        _ => format!("{:.1}G", kb as f64 / 1_048_576.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS: &str = "  100     1  2048  0.5    01:00 -zsh\n  101   100 20480  1.5 01:02:03 nvim --embed .\n  102   101  1024  0.0    00:10 rust-analyzer\n  200     1  4096  0.0    05:00 unrelated daemon\n";

    fn roots(pairs: &[(u32, &str)]) -> HashMap<u32, String> {
        pairs.iter().map(|(p, s)| (*p, s.to_string())).collect()
    }

    #[test]
    fn parse_ps_keeps_the_full_command_with_spaces() {
        // GIVEN / WHEN
        let procs = parse_ps(PS);
        // THEN
        assert_eq!(procs[1].args, "nvim --embed .");
    }

    #[test]
    fn parse_ps_reads_resident_kilobytes() {
        assert_eq!(parse_ps(PS)[1].rss_kb, 20480);
    }

    #[test]
    fn parse_ps_skips_malformed_lines() {
        assert!(parse_ps("garbage\n").is_empty());
    }

    #[test]
    fn descendants_includes_grandchildren_of_a_root() {
        assert_eq!(descendants(&parse_ps(PS), &roots(&[(100, "s")])).get(&102), Some(&"s".to_string()));
    }

    #[test]
    fn descendants_excludes_processes_outside_every_root() {
        assert!(!descendants(&parse_ps(PS), &roots(&[(100, "s")])).contains_key(&200));
    }

    #[test]
    fn ancestors_walks_up_to_the_top_known_parent() {
        assert_eq!(ancestors(&parse_ps(PS), 102), HashSet::from([102, 101, 100, 1]));
    }

    #[test]
    fn build_rows_sorts_biggest_memory_first() {
        let rows = build_rows(&parse_ps(PS), &roots(&[(100, "s")]), &HashSet::new());
        assert_eq!(rows.iter().map(|r| r.proc.pid).collect::<Vec<_>>(), [101, 100, 102]);
    }

    #[test]
    fn build_rows_drops_excluded_pids() {
        let rows = build_rows(&parse_ps(PS), &roots(&[(100, "s")]), &HashSet::from([101]));
        assert!(rows.iter().all(|r| r.proc.pid != 101));
    }

    #[test]
    fn build_rows_labels_each_row_with_its_root_session() {
        let rows = build_rows(&parse_ps(PS), &roots(&[(100, "work"), (200, "other")]), &HashSet::new());
        assert_eq!(rows.iter().find(|r| r.proc.pid == 200).unwrap().session, "other");
    }

    #[test]
    fn mem_formats_kilobytes_megabytes_and_gigabytes() {
        assert_eq!([mem(512), mem(20480), mem(3 * 1_048_576)], ["512K", "20M", "3.0G"]);
    }
}
