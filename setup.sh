#!/usr/bin/env zsh
set -euo pipefail

DOTFILES="${0:A:h}"

run() {
  echo "\n==> $1"
  shift
  "$@"
}

# zellij
run "zellij" zsh "$DOTFILES/zellij/setup.sh"

# nvim
run "nvim" zsh "$DOTFILES/nvim/setup.sh"

# vcs
run "vcs" zsh "$DOTFILES/vcs/setup.sh"

# zsh (last — other modules don't depend on it, but it depends on symlinks being in place)
run "zsh" zsh "$DOTFILES/zsh/setup.sh"

echo "\n==> All done."
