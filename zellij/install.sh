#!/usr/bin/env zsh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_DIR="$SCRIPT_DIR/plugins"

if [ ! -f "$PLUGIN_DIR/vim-zellij-navigator.wasm" ]; then
  wget -P "$PLUGIN_DIR" https://github.com/hiasr/vim-zellij-navigator/releases/download/0.3.0/vim-zellij-navigator.wasm
else
  echo "vim-zellij-navigator plugin is already installed"
fi

if [ ! -f "$PLUGIN_DIR/zellij-autolock.wasm" ]; then
  wget -P "$PLUGIN_DIR" https://github.com/fresh2dev/zellij-autolock/releases/download/0.2.2/zellij-autolock.wasm
else
  echo "zellij-autolock plugin is already installed"
fi

if [ ! -f "$PLUGIN_DIR/zellij-sessionizer.wasm" ]; then
  wget -P "$PLUGIN_DIR" https://github.com/laperlej/zellij-sessionizer/releases/download/v0.4.3/zellij-sessionizer.wasm
else
  echo "zellij-sessionizer plugin is already installed"
fi

if [ ! -f "$PLUGIN_DIR/zellij-switch.wasm" ]; then
  wget -P "$PLUGIN_DIR" https://github.com/mostafaqanbaryan/zellij-switch/releases/download/0.2.1/zellij-switch.wasm
else
  echo "zellij-switch plugin is already installed"
fi
