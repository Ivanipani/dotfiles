#!/usr/bin/env zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
CONFIG_HOME="$HOME/.config/zellij"

check_dep() {
  if ! command -v "$1" &>/dev/null; then
    echo "WARNING: '$1' not found"
  fi
}

cmd_setup() {
  check_dep "zellij"

  echo "Creating symlinks..."
  mkdir -p "$CONFIG_HOME"
  ln -sf "$SCRIPT_DIR/config.kdl" "$CONFIG_HOME/config.kdl"

  rm -rf "$CONFIG_HOME/layouts"
  ln -sf "$SCRIPT_DIR/layouts" "$CONFIG_HOME/layouts"

  rm -rf "$CONFIG_HOME/plugins"
  ln -sf "$SCRIPT_DIR/plugins" "$CONFIG_HOME/plugins"

  echo "Done."
}

do_install() {
  if [ ! -f ./plugins/vim-zellij-navigator.wasm ]; then
    wget -P ./plugins https://github.com/hiasr/vim-zellij-navigator/releases/download/0.3.0/vim-zellij-navigator.wasm
  else
    echo "vim-zellij-navigator plugin is already installed"
  fi
  if [ ! -f ./plugins/zellij-autolock.wasm ]; then
    wget -P ./plugins https://github.com/fresh2dev/zellij-autolock/releases/download/0.2.2/zellij-autolock.wasm
  else
    echo "zellij-autolock plugin is already installed"
  fi
  if [ ! -f ./plugins/zellij-sessionizer.wasm ]; then
    wget -P ./plugins https://github.com/laperlej/zellij-sessionizer/releases/download/v0.4.3/zellij-sessionizer.wasm
  else
    echo "zellij-sessionizer plugin is already installed"
  fi
  if [ ! -f ./plugins/zellij-switch.wasm ]; then
    wget -P ./plugins https://github.com/mostafaqanbaryan/zellij-switch/releases/download/0.2.1/zellij-switch.wasm
  else
    echo "zellij-switch plugin is already installed"
  fi
}

cmd_setup
do_install
