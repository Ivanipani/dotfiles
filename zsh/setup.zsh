#!/usr/bin/env zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"

check_dep() {
  if ! command -v "$1" &>/dev/null; then
    echo "WARNING: '$1' not found"
  fi
}

cmd_setup() {
  local os="$(uname -s)"
  local platform
  case "$os" in
    Darwin) platform="mac" ;;
    Linux)  platform="linux" ;;
    *)      echo "Unsupported OS: $os"; exit 1 ;;
  esac

  echo "Platform: $platform"
  echo "Checking dependencies..."

  # Shared dependencies
  for dep in nvim direnv just zellij starship fd fzf go git wget; do
    check_dep "$dep"
  done

  # Mac-only dependencies
  if [[ "$platform" == "mac" ]]; then
    for dep in cargo wezterm atuin bun; do
      check_dep "$dep"
    done
  fi

  echo "Creating symlinks..."
  ln -sf "$SCRIPT_DIR/$platform/config" "$HOME/.zshrc"
  ln -sf "$SCRIPT_DIR/starship.toml" "$HOME/.config/starship.toml"
  mkdir -p "$HOME/scripts"
  ln -sf "$SCRIPT_DIR/scripts/try" "$HOME/scripts/try"
  echo "Done."
}

cmd_install() {
  ZSH="${ZSH:-$HOME/.oh-my-zsh}"

  if [[ ! -d "$ZSH" ]]; then
    sh -c "$(wget https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh -O -)"
  else
    echo "Oh-My-Zsh is already installed"
  fi

  local plugins=(
    "zsh-autosuggestions|https://github.com/zsh-users/zsh-autosuggestions.git"
    "zsh-syntax-highlighting|https://github.com/zsh-users/zsh-syntax-highlighting.git"
    "zsh-vi-mode|https://github.com/jeffreytse/zsh-vi-mode.git"
  )

  for entry in "${plugins[@]}"; do
    local name="${entry%%|*}"
    local url="${entry##*|}"
    if [[ ! -d "$ZSH/plugins/$name" ]]; then
      git clone "$url" "$ZSH/plugins/$name"
    else
      echo "$name is already installed"
    fi
  done
}

case "${1:-}" in
  setup)   cmd_setup ;;
  install) cmd_install ;;
  *)       echo "Usage: $0 {setup|install}"; exit 1 ;;
esac
