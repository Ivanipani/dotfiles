#!/usr/bin/env nu

# jj working-copy summary for the zjstatus bar.
#
# Emits one line of zjstatus markup (the widget runs with `rendermode "dynamic"`,
# so `#[...]` in stdout is parsed and `$name` resolves against the `color_*`
# palette declared in the zjstatus plugin block). Nothing here knows a hex code.
#
# Read-only on purpose: `--ignore-working-copy` skips the snapshot, so a bar
# ticking every few seconds never takes the working-copy lock or races jjui.
# The cost is that edits show up only once something else snapshots.

# The nearest bookmark at or below @: both the label we show and the anchor the
# stack height is measured against, so the two always agree.
const BASE = "heads(::@- & bookmarks())"

# `latest(..., N)` caps the walk. Without a bookmark ancestor `BASE..@` widens to
# `::@`, and a status bar ticking every few seconds must not enumerate a whole
# repo's history to count to three.
const AHEAD_CAP = 99
const AHEAD = $"latest\(($BASE)..@, ($AHEAD_CAP))"

# One `jj log` call answers everything: ask for the three revsets at once and tag
# each row with the flags that say which one it came from.
const REVSET = $"present\(@) | ($BASE) | ($AHEAD)"

# Plain, not interpolated: the template is full of parens and escapes that nu
# would try to read as its own. `{ahead}` is substituted at the call site.
const TEMPLATE = '
  if(current_working_copy, "w", "-") ++
  if(self.contained_in("{ahead}"), "a", "-") ++ "\x1f" ++
  change_id.shortest(8).prefix() ++ "\x1f" ++
  change_id.shortest(8).rest() ++ "\x1f" ++
  description.first_line() ++ "\x1f" ++
  bookmarks.map(|b| b.name()).join(" ") ++ "\x1f" ++
  if(conflict, "c") ++ if(divergent, "d") ++ if(empty, "e") ++ if(immutable, "i") ++
  "\n"
'

const SEP = "\u{1f}"
const DESC_MAX = 44

# `#[` starts a format part, so a description containing one would be swallowed
# as markup. Break the pair rather than dropping the character.
def sanitize [text: string] {
    $text | str replace --all "#[" "# ["
}

def truncate [text: string] {
    if ($text | str length) <= $DESC_MAX {
        $text
    } else {
        # Inclusive range, and the ellipsis counts, so this lands on DESC_MAX.
        ($text | str substring 0..($DESC_MAX - 2) | str trim --right) + "…"
    }
}

def parse_row [line: string] {
    let f = ($line | split row $SEP)
    {
        working_copy: ($f.0 | str starts-with "w")
        ahead: ($f.0 | str ends-with "a")
        prefix: $f.1
        rest: $f.2
        description: $f.3
        bookmarks: (if ($f.4 | is-empty) { [] } else { $f.4 | split row " " })
        marks: $f.5
    }
}

def main [] {
    let template = ($TEMPLATE | str replace "{ahead}" $AHEAD)
    let out = (jj log --ignore-working-copy --no-graph --color never --quiet -r $REVSET -T $template | complete)
    if $out.exit_code != 0 { return }

    let rows = (
        $out.stdout | lines | where { |l| ($l | str trim | is-not-empty) } | each { |l| parse_row $l }
    )
    let wc = ($rows | where working_copy | get -i 0)
    if $wc == null { return }

    # Change id: the unique prefix is what you actually type, so it carries the
    # accent and the disambiguating rest trails off dim.
    mut parts = [$"#[fg=$mauve,bold]($wc.prefix)#[fg=$overlay0]($wc.rest)"]

    let desc = ($wc.description | str trim)
    $parts = ($parts | append (
        if ($desc | is-empty) {
            "#[fg=$overlay1,italic] (no description)"
        } else {
            $"#[fg=$text] (truncate (sanitize $desc))"
        }
    ))

    # Bookmarks on @ itself are where you are; otherwise fall back to the
    # nearest ancestor bookmark plus the distance, which is where you branched.
    $parts = ($parts | append (
        if ($wc.bookmarks | is-not-empty) {
            $"#[fg=$green] \u{e0a0} ($wc.bookmarks | str join ' ')"
        } else {
            let base = ($rows | where { |r| not $r.working_copy and ($r.bookmarks | is-not-empty) } | get -i 0)
            if $base == null {
                ""
            } else {
                # An empty working copy sitting on trunk is not "one ahead", so
                # the stack height counts real commits only.
                let ahead = ($rows | where { |r| $r.ahead and not ($r.marks | str contains "e") } | length)
                let shown = if $ahead >= $AHEAD_CAP { $"($AHEAD_CAP)+" } else { $"($ahead)" }
                let arrow = if $ahead > 0 { $"#[fg=$sapphire] ↑($shown)" } else { "" }
                $"#[fg=$overlay1] \u{e0a0} ($base.bookmarks | first)($arrow)"
            }
        }
    ))

    let flags = [
        ["c", "#[fg=$red] ✖"]
        ["d", "#[fg=$peach] ⑂"]
        ["e", "#[fg=$overlay1] ∅"]
        ["i", $"#[fg=$yellow] \u{f023}"]
    ]
    $parts = ($parts | append (
        $flags | where { |f| $wc.marks | str contains $f.0 } | each { |f| $f.1 } | str join ""
    ))

    print ($parts | str join "")
}
