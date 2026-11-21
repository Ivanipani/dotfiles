//! Processes running under Rex panes: collection and signalling.

use crate::rex::Rex;
use anyhow::{bail, Result};
use claw::output::Tabular;
use claw::table::{self, Cell};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub rss_kb: u64,
    pub cpu: f32,
    pub etime: String,
    pub args: String,
}

/// A process and the Rex session whose pane it runs under.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub proc: Proc,
    pub session: String,
}

impl Tabular for Row {
    const COLUMNS: &'static [&'static str] = &["SESSION", "PID", "MEM", "CPU%", "TIME", "COMMAND"];

    fn row(&self) -> Vec<Cell> {
        vec![
            Cell::new(&self.session),
            Cell::new(self.proc.pid),
            Cell::new(mem(self.proc.rss_kb)),
            Cell::new(format!("{:.1}", self.proc.cpu)),
            table::dim(&self.proc.etime),
            Cell::new(&self.proc.args),
        ]
    }
}

/// Combined CPU and resident memory of a set of processes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub cpu: f32,
    pub rss_kb: u64,
}

impl Usage {
    pub fn of<'a>(procs: impl IntoIterator<Item = &'a Proc>) -> Self {
        procs.into_iter().fold(Self::default(), |u, p| Self { cpu: u.cpu + p.cpu, rss_kb: u.rss_kb + p.rss_kb })
    }
}

/// The machine's capacity and load.
#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub cores: u32,
    pub mem_kb: u64,
    pub load: [f32; 3],
}

/// The Rex rows plus what the whole system uses, for attributing a share to Rex.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub rows: Vec<Row>,
    pub all: Usage,
    pub host: Option<Host>,
}

/// Parse `sysctl -n hw.ncpu hw.memsize vm.loadavg`.
pub fn parse_sysctl(text: &str) -> Option<Host> {
    let mut lines = text.lines();
    let cores = lines.next()?.trim().parse().ok()?;
    let mem_kb = lines.next()?.trim().parse::<u64>().ok()? / 1024;
    let load: Vec<f32> = lines.next()?.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    Some(Host { cores, mem_kb, load: load.try_into().ok()? })
}

fn pct(part: f64, whole: f64) -> String {
    if whole > 0.0 { format!("{:.0}%", 100.0 * part / whole) } else { "—".into() }
}

/// Two lines (cpu, memory) saying how much of the system `rex` accounts for.
pub fn summary(rex: Usage, all: Usage, host: Option<&Host>) -> [String; 2] {
    let (cpu, rss) = (f64::from(rex.cpu), rex.rss_kb as f64);
    let mut cpu_line = format!("cpu {:.2} cores · {} of all use", cpu / 100.0, pct(cpu, f64::from(all.cpu)));
    let mut mem_line = format!("mem {} · {} of all resident", mem(rex.rss_kb), pct(rss, all.rss_kb as f64));
    if let Some(h) = host {
        let [one, five, fifteen] = h.load;
        cpu_line += &format!(" · {} of {} cores · load {one:.2} {five:.2} {fifteen:.2}", pct(cpu, f64::from(h.cores) * 100.0), h.cores);
        mem_line += &format!(" · {} of {}", pct(rss, h.mem_kb as f64), mem(h.mem_kb));
    }
    [cpu_line, mem_line]
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

fn read_host() -> Option<Host> {
    let out = Command::new("sysctl").args(["-n", "hw.ncpu", "hw.memsize", "vm.loadavg"]).output().ok()?;
    out.status.success().then(|| parse_sysctl(&String::from_utf8_lossy(&out.stdout)))?
}

/// Processes under the panes of every session (or only `session`), excluding this program's own tree.
pub fn collect(rex: &Rex, session: Option<&str>) -> Result<Snapshot> {
    let procs = read_ps()?;
    let roots: HashMap<_, _> = rex.pane_pids(session).into_iter().map(|(label, pid)| (pid, label)).collect();
    let me = std::process::id();
    let mut exclude = ancestors(&procs, me);
    exclude.extend(descendants(&procs, &HashMap::from([(me, String::new())])).into_keys());
    Ok(Snapshot { rows: build_rows(&procs, &roots, &exclude), all: Usage::of(&procs), host: read_host() })
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

    fn host() -> Host {
        Host { cores: 4, mem_kb: 1_048_576, load: [1.5, 1.0, 0.5] }
    }

    #[test]
    fn usage_of_sums_cpu_and_memory() {
        assert_eq!(Usage::of(&parse_ps(PS)[..2]), Usage { cpu: 2.0, rss_kb: 22528 });
    }

    #[test]
    fn parse_sysctl_reads_cores_memory_and_load() {
        // GIVEN
        let text = "8\n17179869184\n{ 5.24 4.47 4.49 }\n";
        // WHEN / THEN
        assert_eq!(parse_sysctl(text), Some(Host { cores: 8, mem_kb: 16 * 1_048_576, load: [5.24, 4.47, 4.49] }));
    }

    #[test]
    fn parse_sysctl_rejects_missing_load() {
        assert_eq!(parse_sysctl("8\n17179869184\n"), None);
    }

    #[test]
    fn summary_cpu_shows_cores_share_of_use_share_of_capacity_and_load() {
        // GIVEN rex uses 200% of the 400% in use on a 4-core machine
        let [cpu, _] = summary(Usage { cpu: 200.0, rss_kb: 0 }, Usage { cpu: 400.0, rss_kb: 0 }, Some(&host()));
        // THEN
        assert_eq!(cpu, "cpu 2.00 cores · 50% of all use · 50% of 4 cores · load 1.50 1.00 0.50");
    }

    #[test]
    fn summary_mem_shows_share_of_resident_and_of_physical_memory() {
        let [_, mem] = summary(Usage { cpu: 0.0, rss_kb: 262_144 }, Usage { cpu: 0.0, rss_kb: 524_288 }, Some(&host()));
        assert_eq!(mem, "mem 256M · 50% of all resident · 25% of 1.0G");
    }

    #[test]
    fn summary_without_host_omits_capacity_and_load() {
        let [cpu, _] = summary(Usage { cpu: 50.0, rss_kb: 0 }, Usage { cpu: 100.0, rss_kb: 0 }, None);
        assert_eq!(cpu, "cpu 0.50 cores · 50% of all use");
    }

    #[test]
    fn summary_with_nothing_in_use_shows_a_dash_instead_of_dividing_by_zero() {
        let [cpu, _] = summary(Usage::default(), Usage::default(), None);
        assert_eq!(cpu, "cpu 0.00 cores · — of all use");
    }

    #[test]
    fn mem_formats_kilobytes_megabytes_and_gigabytes() {
        assert_eq!([mem(512), mem(20480), mem(3 * 1_048_576)], ["512K", "20M", "3.0G"]);
    }
}
