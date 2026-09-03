#!/usr/bin/env nu

# CPU / memory / battery for the zjstatus bar, as a share of the whole machine.
#
# Emits one line of zjstatus markup (the widget runs with `rendermode "dynamic"`,
# so `#[...]` in stdout is parsed and `$name` resolves against the `color_*`
# palette declared in the zjstatus plugin block).
#
# Every number is normalised to 0-100 across the whole system, so a value means
# the same thing on an 8-core laptop as on a 64-core box.

# nf-fa glyphs; the terminal font is already a Nerd Font for zellij's separators.
const CPU_ICON = "\u{f2db}"
const MEM_ICON = "\u{f1c0}"
const CHARGING_ICON = "\u{f0e7}"
# Battery fill, emptiest first, indexed by quintile.
const BATTERY_ICONS = ["\u{f244}", "\u{f243}", "\u{f242}", "\u{f241}", "\u{f240}"]

const WARN_AT = 70
const CRIT_AT = 90

# Colour by load rather than by kind: the bar should only pull the eye when a
# number is worth looking at.
def level_color [pct: float] {
    if $pct >= $CRIT_AT { "$red" } else if $pct >= $WARN_AT { "$peach" } else { "$subtext0" }
}

def gauge [icon: string, pct: float] {
    $"#[fg=$overlay0]($icon) #[fg=(level_color $pct)]($pct | math round)%"
}

# macOS reports a ready-made breakdown; despite the man page's "since boot"
# wording for the first sample, `-l 1` tracks a real interval (verified against
# `top -l 2` under load). Linux has no such summary, so difference /proc/stat
# over a short window instead.
def cpu_percent [] {
    if $nu.os-info.name == "macos" {
        let line = (^top -l 1 -n 0 -s 0 | lines | where { |l| $l starts-with "CPU usage" } | get -i 0)
        if $line == null { return null }
        let idle = ($line | parse --regex '(?<idle>[0-9.]+)% idle' | get -i 0.idle)
        if $idle == null { return null }
        100.0 - ($idle | into float)
    } else {
        let a = (proc_stat_ticks)
        sleep 120ms
        let b = (proc_stat_ticks)
        if $a == null or $b == null { return null }
        let total = ($b.total - $a.total)
        if $total <= 0 { return null }
        100.0 * (($total - ($b.idle - $a.idle)) | into float) / ($total | into float)
    }
}

# The aggregate "cpu" row of /proc/stat. After the label the numbers run
# user nice system idle iowait ..., so idle time is the 4th and 5th of them.
def proc_stat_ticks [] {
    let line = (open --raw /proc/stat | lines | where { |l| $l starts-with "cpu " } | get -i 0)
    if $line == null { return null }
    let f = ($line | split row --regex '\s+' | skip 1 | where { |x| $x != "" } | each { |x| $x | into int })
    { total: ($f | math sum), idle: (($f | get 3) + ($f | get 4)) }
}

def mem_percent [] {
    # `sys mem` hands back filesizes, which have no direct float conversion.
    let m = (sys mem)
    let total = ($m.total | into int)
    if $total == 0 { return null }
    100.0 * ($m.used | into int | into float) / ($total | into float)
}

# `null` on a machine with no battery, which is also how a desktop opts out.
def battery [] {
    if $nu.os-info.name == "macos" {
        let out = (^pmset -g batt | complete)
        if $out.exit_code != 0 { return null }
        let row = ($out.stdout | parse --regex '(?<pct>\d+)%; (?<state>[a-z ]+)' | get -i 0)
        if $row == null { return null }
        { percent: ($row.pct | into int), charging: ($row.state != "discharging") }
    } else {
        let dir = (glob /sys/class/power_supply/BAT* | get -i 0)
        if $dir == null { return null }
        {
            percent: (open --raw ([$dir capacity] | path join) | str trim | into int)
            charging: ((open --raw ([$dir status] | path join) | str trim) != "Discharging")
        }
    }
}

def render_battery [] {
    let b = (try { battery } catch { null })
    if $b == null { return "" }

    let icon = if $b.charging {
        $CHARGING_ICON
    } else {
        $BATTERY_ICONS | get ([($b.percent * 5 // 100), 4] | math min)
    }
    let color = if $b.charging {
        "$green"
    } else if $b.percent <= 15 {
        "$red"
    } else if $b.percent <= 30 {
        "$peach"
    } else {
        "$subtext0"
    }
    $"#[fg=$overlay0] ($icon) #[fg=($color)]($b.percent)%"
}

def main [] {
    let cpu = (try { cpu_percent } catch { null })
    let mem = (try { mem_percent } catch { null })

    mut parts = []
    if $cpu != null { $parts = ($parts | append (gauge $CPU_ICON $cpu)) }
    if $mem != null { $parts = ($parts | append (gauge $MEM_ICON $mem)) }
    $parts = ($parts | append (render_battery))

    print ($parts | where { |p| $p != "" } | str join "#[fg=$surface2]  ")
}
