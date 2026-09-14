#!/usr/bin/env nu

# Battery for the zjstatus bar.
#
# Emits one line of zjstatus markup (the widget runs with `rendermode "dynamic"`,
# so `#[...]` in stdout is parsed and `$name` resolves against the `color_*`
# palette declared in the zjstatus plugin block).
#
# Every tab runs its own copy of this script, and they share one sample per
# interval. See zjstatus-cache.nu.

const CACHE = path self "zjstatus-cache.nu"
use $CACHE [cached]

# Just under the widget's `command_sys_interval`, so exactly one instance per
# interval refreshes it.
const TTL = 4500ms

# nf-fa glyphs; the terminal font is already a Nerd Font for zellij's separators.
const CHARGING_ICON = "\u{f0e7}"
# Battery fill, emptiest first, indexed by quintile.
const BATTERY_ICONS = ["\u{f244}", "\u{f243}", "\u{f242}", "\u{f241}", "\u{f240}"]

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

def render [] {
    render_battery
}

def main [] {
    print (cached "sys" $TTL { render })
}
