# Nushell Skill

This skill provides guidance for using Nushell (`nu`) both as an interactive shell and as a programming language. Use it when writing `.nu` scripts, modules, custom commands, or pipelines, or when answering questions about Nushell syntax and idioms.

**Official docs:** https://www.nushell.sh/book/

## Core Mental Model

Nushell is **structured-data-first**. Unlike POSIX shells that pass plain text between commands, Nu commands communicate via typed values — lists, records, and tables. A pipeline transforms a *stream of structured data*, not bytes. When in doubt about a value, pipe it to `describe` to see its type, and `help <command>` to see its input/output type signature.

```nu
ls | where type == dir | sort-by modified | select name size   # data flows as a table
42 | describe                                                   # => int
```

## Data Types

| Type | Literal example | Notes |
|------|-----------------|-------|
| `int` | `-100`, `0xff`, `0o234`, `0b1010` | hex/octal/binary prefixes |
| `float` | `1.5`, `-15.333` | |
| `string` | `"text"`, `'text'`, `` `text` ``, `$"hi ($name)"` | double-backtick = bare word; `$"..."` = interpolation |
| `bool` | `true`, `false` | |
| `datetime` | `2000-01-01`, `(date now)` | |
| `duration` | `2min`, `3.14day`, `12sec` | |
| `filesize` | `64mb`, `0.5kB`, `1GiB` | |
| `range` | `1..10`, `0..<5`, `2..4..20` | inclusive `..`, exclusive `..<`, stride `start..second..end` |
| `binary` | `0x[FE FF]`, `0b[1010]` | |
| `list` | `[0 1 'two' 3]` | space- or comma-separated |
| `record` | `{name: "Nu", rank: 99}` | |
| `table` | `[{x:12 y:15} {x:8 y:9}]` | a list of records |
| `closure` | `{\|a\| $a > 5}` | anonymous fn; params in `\| \|` |
| `cell-path` | `$.name.0`, `.2` | path into structured data |
| `nothing` | `null` | use optional operator `?` to tolerate missing |

Inspect with `describe`; convert with `into <type>` (e.g. `"42" | into int`, `$x | into string`).

## Pipelines

A pipeline is **input (source) → filters → output (sink)**:

```nu
open Cargo.toml | update workspace.dependencies.base64 0.24.2 | save Cargo_new.toml
```

### The `$in` variable
`$in` holds the current pipeline input. This is the backbone of Nu composability:

```nu
def double [] { $in * 2 }        # $in = pipeline input in first position
4 | $in * $in | $in / 2          # => 8 ; $in = previous expression elsewhere
```

`$in` is `null` with no input. It does **not** carry across a `;` — use a pipe instead.

### Multi-line pipelines
Wrap long pipelines in `(` `)` or break after `|`:

```nu
let year = (
  "01/22/2021"
  | parse "{month}/{day}/{year}"
  | get year
)
```

### External commands
Nu auto-renders tables/lists to text before piping to external commands. Convert explicitly with `to text` and prefix external commands with `^` to bypass any Nu builtin of the same name:

```nu
ls /usr/share/ | get name | to text | ^grep foo
```

### Redirection
`out>` / `o>` for stdout, `err>` / `e>` for stderr, `out+err>` to combine:

```nu
^cmd out> out.log err> err.log
^cmd o+e> combined.log
```

## Working with Lists

```nu
let xs = [Mark Tami Amanda]
$xs.1                              # index access => Tami
$xs | get 1                        # same
$xs | length                       # 3
$xs | is-empty
$xs | first 2 ; $xs | last 2       # take from ends
$xs | skip 1 ; $xs | drop 1        # remove from ends
$xs | append X ; $xs | prepend X   # add
$xs ++ [Y Z]                       # concatenate
$xs | insert 1 NEW                 # insert at index
$xs | update 1 CHANGED             # replace at index
[3 8 4] | where $it > 4            # filter => [8]
$xs | each {|e| $"hi ($e)" }       # map
$xs | enumerate | each {|e| $"($e.index): ($e.item)" }
[3 8 4] | reduce {|it, acc| $acc + $it }            # fold => 15
[3 8 4] | reduce --fold 1 {|it, acc| $acc * $it }   # => 96
$xs | any {|e| $e == "Tami" } ; $xs | all {|e| ($e | str length) > 2 }
'Tami' in $xs                       # membership
[1 [2 3] 4] | flatten               # => [1 2 3 4]
$xs | wrap Name                     # list -> single-column table
```

## Working with Records

```nu
let r = {apples: 543, bananas: 411}
$r.apples                           # field access
$r | get apples
$r | insert pears 21                # add field
$r | update apples 600              # change field
$r | merge {title: "Mayor"}         # combine records
{...$r, title: "Mayor"}             # spread operator
$r | items {|k, v| $"($k)=($v)" }   # iterate key/value
$r | transpose key value            # record -> 2-col table
```

## Working with Tables

```nu
ls | select name size               # keep columns
ls | get name                       # one column as a list
ls | reject type modified           # drop columns
ls | where size > 1mb               # filter rows
ls | sort-by size | reverse
ls | first 5 ; ls | skip 2
ls | insert kind "file"             # add column
ls | update name {|r| $r.name | str upcase}
ls | rename fname ftype fsize
ls | move name --after size
ls | group-by type
$a | append $b ; $a ++ $b           # stack rows
$a | merge $b                        # join columns side-by-side
ls | length
```

## Variables

```nu
let val = 42                  # immutable (preferred)
mut count = 0                 # mutable
$count += 1                   # += -= *= /= ; ++= for lists
const FILE = 'lib.nu'         # parse-time constant (needed by `source`/`use`)
```

Variables shadow in inner scopes without affecting outer ones. **Closures cannot capture mutable variables** — this is by design and enables drop-in parallelism: swap `each` for `par-each` to parallelize. Prefer immutable/functional style over `mut` + loops.

## Operators

- Arithmetic: `+ - * /`, `//` floor-div, `mod`, `**` power
- Comparison: `== != < <= > >=`
- Logical: `and or xor not` (short-circuit)
- Regex/string: `=~` (or `like`), `!~` (or `not-like`), `starts-with`, `ends-with`
- Membership: `in`, `not-in`, `has`, `not-has`
- Bitwise: `bit-and bit-or bit-xor bit-shl bit-shr`
- Lists: `++` concatenate

Precedence (high→low): `()` → `**` → `* / // mod` → `+ -` → bit-shift → comparison/membership → bitwise → logical → assignment → `not`. Nu enforces strict types — mismatches error rather than coerce.

## Control Flow

`if`/`match` are **expressions** that return values.

```nu
if $x > 0 { 'pos' } else if $x == 0 { 'zero' } else { 'neg' }

match $foo {
  {name: 'bar', count: $n} if $n < 5 => ($n + 3),   # record pattern + guard
  {name: _, count: $n}              => ($n + 7),     # _ wildcard binding
  [$first, ..$rest]                 => $first,        # list pattern + rest
  _                                  => 1             # catch-all
}

for x in [1 2 3] { print ($x * $x) }
mut i = 0; while $i < 10 { $i += 1 }
loop { if $done { break }; continue }
```

**Prefer pipeline commands** (`each`, `where`, `reduce`, `par-each`) over `for`/`while` loops: loops can't appear in pipelines and force mutable state. Reach for loops only for side effects that don't fit a pipeline.

## Custom Commands

```nu
# Greet someone (this comment is the command's help text)
def greet [
  name: string         # required, typed positional
  greeting = "Hello"   # default value
  age?: int            # optional positional
  --caps               # boolean switch -> $caps
  --times (-n): int    # flag with short alias -> $times
  ...rest: string      # rest parameter
] {
  $"($greeting), ($name)!"
}
```

- Dashed flags become underscores in the body: `--all-caps` → `$all_caps`.
- Spread a list into rest params: `greet "CEO" ...$guests`.
- Pipeline input arrives via `$in`; declare types with a signature:

```nu
def inc []: int -> int { $in + 1 }              # input -> output type
def "str join" [sep?: string]: [                # multiple signatures
  list -> string
  string -> string
] { ... }
```

- `def --env` persists `$env` changes and `cd` to the caller.
- `def --wrapped passthru [...rest] { ^real-cmd ...$rest }` forwards all args/flags.
- Last expression is the implicit return; use `return` for early exit, `| ignore` to discard output.
- Group subcommands with quoted names: `def "git undo" [] { ... }`.

## Scripts

```nu
#!/usr/bin/env nu
# myscript.nu — definitions run first, so order is flexible

def main [x: int] { $x + 10 }
```

```sh
nu myscript.nu 100      # => 110 (arg parsed to int)
```

Subcommands via `main`:

```nu
def "main build" [] { print "building" }
def "main run" []   { print "running" }
def main [] { print "default" }     # base main REQUIRED for subcommands
```

```sh
nu myscript.nu          # default
nu myscript.nu build    # subcommand
```

- Shebang `#!/usr/bin/env nu`; for stdin use `#!/usr/bin/env -S nu --stdin` (input in `$in`).
- Run in a fresh instance: `nu script.nu`. Run in the current session: `source script.nu`.

## Modules

A module is a `<name>.nu` file or a `<name>/mod.nu` directory.

```nu
# inc.nu
export def main []: int -> int { $in + 1 }   # becomes `inc` when imported
export def square [] { $in * $in }
export const PI = 3.14159
export alias ll = ls -l
def helper [] { ... }                          # no `export` => private
export-env { $env.MY_VAR = "set on import" }
```

```nu
use inc.nu              # imports `inc` namespace -> `inc square`, `inc`
use inc.nu *            # bring exports into current scope unqualified
use inc.nu square       # import just one command
42 | inc                # => 43
```

- An export can't share the module's name — use `export def main`, which takes the module name on import.
- `export module sub` nests a submodule; `export use other.nu *` re-exports (flattens) another module's contents.

## Error Handling

```nu
def my-command [x] {
  let span = (metadata $x).span        # locate the offending arg
  error make {
    msg: "this is fishy"
    label: { text: "fish right here", span: $span }
  }
}

try {
  risky-thing
} catch {|err|
  print $"failed: ($err.msg)"
}
```

`error make` produces a formatted error with source highlighting. `try`/`catch` recovers; the catch closure receives the error record.

## Loading & Parsing Data

```nu
open data.json                 # auto-detects format -> structured data
open Cargo.lock | from toml    # explicit parse when extension lies
open file.txt --raw            # raw text, no parsing
open people.txt | lines        # text -> list of lines
open people.txt | lines | split column "|" first last job | str trim
"a=1" | parse "{key}={val}"    # pattern parse -> table
$data | to json                # serialize: to json/csv/yaml/toml/...
$data | save out.json
http get https://example.com/feed.xml
open db.sqlite | query db "select * from t"
```

`from <fmt>` / `to <fmt>` convert between text and structured data: `json csv yaml toml xml nuon ...`. `parse`, `lines`, `split column/row`, and the `str` command family handle unstructured text.

## Plugins (Custom Libraries)

Plugins are separate executables that extend Nu with new commands. Their filename must start with `nu_plugin_`, and they talk to Nu over stdin/stdout (JSON or MessagePack). Use them to pull in capabilities the core lacks.

**Core plugins** ship alongside Nushell: `polars` (DataFrames for large data), `formats` (EML/ICS/INI/plist/VCF), `gstat` (Git status as structured data), `query` (SQL/XML/JSON/HTML), `inc` (version bumping).

```nu
cargo install nu_plugin_gstat --locked   # install a plugin binary (third-party via crates.io/GitHub)
plugin add ~/.cargo/bin/nu_plugin_gstat  # register: records it in the plugin registry file
plugin use gstat                          # load into the session (name without nu_plugin_ prefix)
plugin list                               # show active plugins + their commands
scope commands | where type == plugin     # see commands a plugin added
plugin stop query                         # stop a running plugin process
plugin rm gstat                           # unregister
```

- Registered plugins load automatically at startup; `plugin add` is a one-time step.
- Search path: `const NU_PLUGIN_DIRS` (parse-time) or `$env.NU_PLUGIN_DIRS` (next parse).
- Nu garbage-collects idle plugins (default `stop_after: 10sec`); tune per-plugin:

```nu
$env.config.plugin_gc = {
  default: { enabled: true, stop_after: 10sec }
  plugins: { gstat: { stop_after: 1min } }
}
```

Find plugins on crates.io, GitHub, and `awesome-nu`. Always match the plugin version to your Nushell version.

## DevOps Scripting

Nushell is a strong base for DevOps automation: structured data means you parse tool output into tables instead of fighting `awk`/`sed`/`jq`, and errors short-circuit pipelines instead of silently passing.

### Robust scripts
```nu
#!/usr/bin/env nu
def main [
  env: string         # target environment
  --dry-run           # preview only
] {
  if $env not-in [dev staging prod] {
    error make { msg: $"unknown env: ($env)" }
  }
  ...
}
```

- Validate inputs early with typed params + `error make`; a failing pipeline aborts the script with a clear message.
- Wrap fallible external calls in `try { ^cmd } catch {|e| ... }`.
- Check external exit codes via `$env.LAST_EXIT_CODE`, or use `complete` to capture stdout/stderr/exit together:

```nu
let r = (^kubectl get pods -o json | complete)
if $r.exit_code != 0 { error make { msg: $r.stderr } }
$r.stdout | from json | get items
```

### Parse tool output into data
```nu
^kubectl get pods -o json | from json | get items
  | select metadata.name status.phase
  | where status.phase != "Running"

^docker ps --format '{{.Names}}\t{{.Status}}' | lines | split column "\t" name status

^df -h | detect columns                       # turn columnar text into a table
sys mem | get used                            # built-in system info as structured data
ps | where cpu > 10 | sort-by cpu --reverse   # processes as a table
```

### Environment & config
```nu
$env.AWS_PROFILE = "prod"          # set env for child processes
$env.PATH = ($env.PATH | prepend "/opt/bin")
with-env { TOKEN: $secret } { ^deploy }       # scoped env for one block
open config.yaml | get database.host          # read config of any format as data
```

### Parallelism & remote work
```nu
[web1 web2 web3] | par-each {|host| ^ssh $host uptime }   # fan out concurrently
$urls | par-each {|u| http get $u | get status }
```

`par-each` is a drop-in parallel `each` — ideal for hitting many hosts/endpoints. Combine with `where`/`reduce` to aggregate results into a report table.

## Idioms & Tips

- Discover everything: `help <cmd>`, `help commands`, `<value> | describe`, `scope commands`.
- Prefer functional pipelines over imperative loops; it reads better and parallelizes via `par-each`.
- Use string interpolation `$"...($expr)..."` rather than concatenation.
- Tables are lists of records and records are one-row tables — most row/column commands work across all three.
- Use the optional operator `?` (e.g. `$x.maybe?`) to get `null` instead of an error on missing fields.
- Reach for `where`/`get`/`select`/`update` to slice structured data instead of `grep`/`awk`/`cut`.
