#!/usr/bin/env zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
CONFIG_HOME="$HOME/.config/nvim"

check_dep() {
  if ! command -v "$1" &>/dev/null; then
    echo "WARNING: '$1' not found"
  fi
}

cmd_setup() {
  check_dep "nvim"

  echo "Creating symlinks..."
  rm -rf "$CONFIG_HOME"
  ln -sf "$SCRIPT_DIR/config" "$CONFIG_HOME"

  echo "Done."
}

cmd_setup
