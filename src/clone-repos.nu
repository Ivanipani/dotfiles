#!/usr/bin/env nu
# Clone every repo in repos.txt into the projects dir as a colocated jj repo.
# Safe to re-run: repos that already exist are skipped.

def main [
    --list: path # repo list, one URL per line
    --dest: path # where to clone
] {
    let list = $list | default ($env.FILE_PWD | path join repos.txt)
    let dest = $dest | default ($env.PROJECTS_DIR? | default "~/projects") | path expand
    mkdir $dest

    let urls = open $list
        | lines
        | str trim
        | where {|l| $l != "" and not ($l | str starts-with "#") }

    let failed = $urls | each {|url|
        let name = $url | path basename | str replace -r '\.git$' ''
        let target = $dest | path join $name
        if ($target | path exists) {
            print $"skip  ($name) \(exists)"
            return null
        }
        print $"clone ($name)"
        try {
            mkdir $target
            cd $target
            jj git init --colocate
            jj git remote add origin $url
            jj git fetch --remote origin
            jj new 'trunk()'
            null
        } catch {|e|
            rm -rf $target
            print -e $"fail  ($name): ($e.msg)"
            $url
        }
    } | compact

    if ($failed | is-not-empty) {
        error make {msg: $"($failed | length) repo\(s) failed: ($failed | str join ', ')"}
    }
}
