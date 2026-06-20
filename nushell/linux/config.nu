load-env {"SHELL": "nu"}

# ============================================================================
# Library & plugin search paths (parse-time consts — must live in config.nu).
#
#   use-on-demand libraries (then `use <name>`):
#     ~/.config/nushell/scripts          per-user (Nushell default)
#     ~/.config/nushell/lib              per-user
#     /usr/local/share/nushell/lib       system-wide, shared by every user
#
#   plugins (`plugin add <bin>` / `plugin use <name>`):
#     ~/.config/nushell/plugins          per-user (Nushell default)
#     /usr/local/share/nushell/plugins   system-wide
#
#   zero-`use` drop-in (auto-sourced in interactive shells, no config needed):
#     ~/.config/nushell/autoload                 per-user
#     /usr/local/share/nushell/vendor/autoload   system-wide
# ============================================================================
const NU_LIB_DIRS = [
    $"($nu.default-config-dir)/lib"
    "/usr/local/share/nushell/lib"
    ...$NU_LIB_DIRS
]
const NU_PLUGIN_DIRS = [
    "/usr/local/share/nushell/plugins"
    ...$NU_PLUGIN_DIRS
]

# ============================================================================
# Environment — single source of truth lives in login.nu (the login shell's
# job). Sourced here so non-login interactive shells get it too; `path add`
# is idempotent, so login shells running it again is harmless.
# ============================================================================
source ($nu.default-config-dir | path join "login.nu")

# ============================================================================
# Aliases (parity with zsh/common/02-aliases.zsh + zsh/linux/02-aliases.zsh)
# ============================================================================
alias l = ls
alias la = ls -a
def lt [] { ls | sort-by modified }
def lat [] { ls -a | sort-by modified }

alias v = nvim
alias vim = nvim
alias vimdiff = nvim -d

alias k = kubectl
alias h = history
alias ec = nvim $env.DOTFILE_DIR     # edit dotfiles
def --env sc [] { exec nu }          # reload shell config

alias ldisk = lsblk -d -o "NAME,SIZE,MODEL,TRAN"

def docker-stop-all [] {
    let ids = (^docker ps -aq | lines)
    if ($ids | is-not-empty) {
        ^docker stop ...$ids
        ^docker rm ...$ids
    }
}

# ============================================================================
# Functions (parity with zsh/common/03-functions.zsh)
# ============================================================================

# Zellij sessionizer
def cs [] {
    ^$"($env.DOTFILE_DIR)/zellij/scripts/sessionizer-david.zsh"
}

# fuzzy directory picker -> cd
def --env f [] {
    if (which fd | is-empty) or (which fzf | is-empty) {
        print -e "[nu] fd/fzf not found, skipping directory picker (f)"
        return
    }
    let selection = (
        fd --type directory --base-directory $env.HOME
        | fzf --height=50% --padding=1% --border=double --header="CTRL-C or ESC to quit"
        | str trim
    )
    if ($selection | is-empty) { return }
    let target = ($env.HOME | path join $selection)
    if ($target | path type) == "dir" {
        cd $target
    } else {
        print $"Directory '($target)' does not exist."
    }
}

# ============================================================================
# Integrations (parity with zsh/common/04-integrations.zsh)
# ============================================================================
mkdir ($nu.data-dir | path join "vendor/autoload")
starship init nu | save -f ($nu.data-dir | path join "vendor/autoload/starship.nu")

# ============================================================================
# Behavior (parity with zsh vi-mode / options / plugins)
# Autosuggestions and syntax highlighting are built into Nushell.
# ============================================================================
$env.config.edit_mode = "vi"
# Make the active vi mode obvious. starship blanks PROMPT_INDICATOR but leaves
# the vi-specific indicators unset, so we own these: block cursor + yellow tag
# in NORMAL, bar cursor + green tag in INSERT.
$env.config.cursor_shape = {
    vi_insert: "line"
    vi_normal: "block"
}
$env.PROMPT_INDICATOR_VI_INSERT = $"(ansi { fg: 'black' bg: 'green' }) I (ansi reset) "
$env.PROMPT_INDICATOR_VI_NORMAL = $"(ansi { fg: 'black' bg: 'yellow' }) N (ansi reset) "
$env.config.show_banner = false
$env.config.buffer_editor = "nvim"
$env.config.hooks = {
    pre_prompt: [{ ||
        if (which direnv | is-empty) { return }
        direnv export json | from json | default {} | load-env
        if 'ENV_CONVERSION' in $env and 'PATH' in $env.ENV_CONVERSIONS {
            $env.PATH = do $env.ENV_CONVERSIONS.PATH.from_string $env.PATH
        }
    }]
}

# atuin shell history (generated in env.nu)
const atuin_cfg = "~/.atuin.nu"
if ($atuin_cfg | path exists) { source $atuin_cfg }

def "nu-complete just" [] {
    (^just --dump --unstable --dump-format json | from json).recipes | transpose recipe data | flatten | where {|row| $row.private == false } | select recipe doc parameters | rename value description
}

# Just: A Command Runner
export extern "just" [
    ...recipe: string@"nu-complete just", # Recipe(s) to run, may be with argument(s)
]
