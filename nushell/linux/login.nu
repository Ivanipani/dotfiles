# login.nu — runs once for login shells (`nu --login`), after config.nu.
# The login shell is responsible for the environment inherited by every
# subshell and child process. config.nu also sources this so non-login
# interactive shells get the same environment. `path add` is idempotent,
# so running it twice for a login shell is harmless.
#
# Parity with zsh/linux/01-env.zsh + common integrations.

# Repo root, resolved through the config.nu symlink (linux -> nushell -> dotfiles)
$env.DOTFILE_DIR = ($nu.config-path | path expand | path dirname | path dirname | path dirname)

use std/util "path add"
path add ...[
    "~/.local/bin"
    "~/scripts"
    "/usr/local/go/bin"
    ($env.HOME | path join "go/bin")
    ($env.HOME | path join ".cargo/bin")
    ($env.HOME | path join ".bun/bin")
]

$env.EDITOR = "nvim"
$env.GOPATH = ($env.HOME | path join "go")
$env.XDG_CONFIG_HOME = ($env.HOME | path join ".config")
$env.ZELLIJ_CONFIG_DIR = ($env.HOME | path join ".config/zellij")
