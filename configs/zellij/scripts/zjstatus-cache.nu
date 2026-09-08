#!/usr/bin/env nu

# One computation per interval, shared by every status bar in the session.
#
# zellij loads a plugin instance per pane and a pane belongs to one tab, so a
# session with five tabs runs five zjstatus instances, each on its own timer.
# There is no way to share the instance — but there is no reason to repeat its
# work. The first instance to find the cache stale recomputes; the rest read
# what it wrote. Five tabs cost one `top` sample per interval, not five.

export def dir [] {
    let base = ($env.XDG_CACHE_HOME? | default ($nu.home-path | path join ".cache"))
    let dir = ($base | path join "zjstatus")
    mkdir $dir
    $dir
}

# `ttl` belongs just under the widget's interval: one instance per interval then
# finds the value expired and refreshes it, and every other instance that ticks
# inside that window is served.
#
# `key` names the value, so anything the value depends on has to be in it —
# for the jj widget that means the directory it was asked about.
export def cached [key: string, ttl: duration, compute: closure] {
    let file = (dir | path join $key)
    let cached = (try { { age: ((date now) - (ls $file | get 0.modified)), text: (open --raw $file | into string) } } catch { null })
    if $cached != null and $cached.age < $ttl {
        return $cached.text
    }

    # Claim the interval before doing the work rather than after. The others
    # then see a fresh file and serve the previous value instead of each
    # repeating the same sample, and the window in which two can claim at once
    # is the microseconds between the stat above and this touch. Nothing to
    # claim on the first run: let every instance compute once rather than serve
    # the empty file a touch would create.
    if $cached != null { touch $file }

    let value = (do $compute | into string)
    # Rename rather than write in place, so a reader never catches half a line.
    let tmp = $"($file).($nu.pid)"
    $value | save --force --raw $tmp
    mv --force $tmp $file
    $value
}
