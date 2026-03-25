if (( $+commands[direnv] )); then
  eval "$(direnv hook zsh)"
else
  echo "[zsh] direnv not found, skipping hook" >&2
fi

if (( $+commands[just] )); then
  eval "$(just --completions zsh)"
else
  echo "[zsh] just not found, skipping completions" >&2
fi

if (( $+commands[wezterm] )); then
  eval "$(wezterm shell-completion --shell zsh)"
else
  echo "[zsh] wezterm not found, skipping shell completion" >&2
fi

if (( $+commands[atuin] )); then
  eval "$(atuin init zsh)"
else
  echo "[zsh] atuin not found, skipping history integration" >&2
fi

if [[ -s "$HOME/.bun/_bun" ]]; then
  source "$HOME/.bun/_bun"
  export BUN_INSTALL="$HOME/.bun"
  export PATH="$BUN_INSTALL/bin:$PATH"
else
  echo "[zsh] bun not found, skipping completions" >&2
fi

if [[ -f "$HOME/.cargo/env" ]]; then
  . "$HOME/.cargo/env"
else
  echo "[zsh] cargo env not found, skipping rust setup" >&2
fi

# if (( $+commands[go] )); then
#   export GOPATH="$HOME/go"
#   export PATH="$HOME/go/bin:/usr/local/go/bin:$PATH"
# else
#   echo "[zsh] go not found, skipping go paths" >&2
# fi
export GOPATH="$HOME/go"
export PATH="$HOME/go/bin:/usr/local/go/bin:$PATH"

if (( $+commands[zellij] )); then
  export ZELLIJ_CONFIG_DIR="$HOME/.config/zellij"
else
  echo "[zellij] go not found, skipping completions" >&2
fi
