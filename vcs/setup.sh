#!/usr/bin/env zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
JJ_CONFIG_HOME="$HOME/.config/jj"

check_dep() {
  if ! command -v "$1" &>/dev/null; then
    echo "WARNING: '$1' not found"
  fi
}

cmd_setup() {
  check_dep "jj"
  check_dep "git"

  echo "Creating symlinks..."

  mkdir -p "$JJ_CONFIG_HOME"
  ln -sf "$SCRIPT_DIR/jj/config.toml" "$JJ_CONFIG_HOME/config.toml"

  ln -sf "$SCRIPT_DIR/git/gitconfig" "$HOME/.gitconfig"
  ln -sf "$SCRIPT_DIR/git/gitignore_global" "$HOME/.gitignore_global"

  echo "Done."
}

cmd_setup
