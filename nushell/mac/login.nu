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

# Nix (Determinate). The installer only writes zsh/bash/fish integration, so
# nushell needs the equivalent of profile.d/nix-daemon.sh by hand.
const nix_default = "/nix/var/nix/profiles/default"
if ($nix_default | path exists) {
    let nix_link = ($env.HOME | path join ".nix-profile")
    path add ($nix_link | path join "bin") ($nix_default | path join "bin")
    $env.NIX_PROFILES = $"($nix_default) ($nix_link)"
    $env.XDG_DATA_DIRS = ([
        ($env.XDG_DATA_DIRS? | default "/usr/local/share:/usr/share")
        ($nix_link | path join "share")
        ($nix_default | path join "share")
    ] | str join ":")
    # Mirrors nix-daemon.sh's fallback: no system CA bundle exists on macOS.
    let ca = ($nix_default | path join "etc/ssl/certs/ca-bundle.crt")
    if ($ca | path exists) { $env.NIX_SSL_CERT_FILE = $ca }
}
