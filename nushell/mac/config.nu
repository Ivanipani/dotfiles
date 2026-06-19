load-env {"SHELL": "nu"}

# Repo root, resolved through the config.nu symlink (mac -> nushell -> dotfiles)
let dotfile_dir = ($nu.config-path | path expand | path dirname | path dirname | path dirname)

# ============================================================================
# Environment (parity with zsh/mac/01-env.zsh + zsh/mac/main.zsh)
# ============================================================================
use std/util "path add"
path add ...[
    "~/.bun/bin"
    "~/.local/bin"
    "~/scripts"
    "/opt/homebrew/bin"
    "/opt/homebrew/sbin"
    "/usr/local/bin"
    "/opt/homebrew/opt/postgresql@18/bin"
    "/usr/local/go/bin"
    ($env.HOME | path join "go/bin")
    ($env.HOME | path join ".cargo/bin")
]

# Google Cloud SDK (only if installed)
const gcloud_bin = "~/github/ivanipani/doghouse/google-cloud-sdk/bin"
if ($gcloud_bin | path expand | path exists) { path add $gcloud_bin }

$env.EDITOR = "nvim"
$env.GOPATH = ($env.HOME | path join "go")
$env.XDG_CONFIG_HOME = ($env.HOME | path join ".config")
$env.ZELLIJ_CONFIG_DIR = ($env.HOME | path join ".config/zellij")
$env.DOTFILE_DIR = $dotfile_dir
$env.KUBECONFIG = ([
    ($env.HOME | path join ".kube/config")
    ($env.HOME | path join ".kube/doghouse")
] | str join ":")

# ============================================================================
# Aliases (parity with zsh/common/02-aliases.zsh)
# ============================================================================
alias l = ls
alias la = ls -a
def lt [] { ls | sort-by modified }
def lat [] { ls -a | sort-by modified }

# alias v = nvim
# alias vim = nvim
alias vimdiff = nvim -d

alias k = kubectl
alias h = history
alias ec = nvim $env.DOTFILE_DIR     # edit dotfiles
def --env sc [] { exec nu }          # reload shell config

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
