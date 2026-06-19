# login.nu — runs once for login shells (`nu --login`), after config.nu.
# The login shell is responsible for the environment inherited by every
# subshell and child process. config.nu also sources this so non-login
# interactive shells get the same environment. `path add` is idempotent,
# so running it twice for a login shell is harmless.
#
# Parity with zsh/mac/01-env.zsh + zsh/mac/main.zsh.

# Repo root, resolved through the config.nu symlink (mac -> nushell -> dotfiles)
$env.DOTFILE_DIR = ($nu.config-path | path expand | path dirname | path dirname | path dirname)

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
$env.KUBECONFIG = ([
    ($env.HOME | path join ".kube/config")
    ($env.HOME | path join ".kube/doghouse")
] | str join ":")
