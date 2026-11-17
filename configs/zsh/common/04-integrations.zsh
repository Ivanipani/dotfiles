# Optional integrations: skipped silently when absent (console output during
# init breaks the Powerlevel10k instant prompt).
(( $+commands[direnv] )) && eval "$(direnv hook zsh)"
(( $+commands[just] )) && eval "$(just --completions zsh)"
(( $+commands[wezterm] )) && eval "$(wezterm shell-completion --shell zsh)"
(( $+commands[atuin] )) && eval "$(atuin init zsh)"

if [[ -s "$HOME/.bun/_bun" ]]; then
  source "$HOME/.bun/_bun"
  export BUN_INSTALL="$HOME/.bun"
  export PATH="$BUN_INSTALL/bin:$PATH"
fi

[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"

(( $+commands[zellij] )) && export ZELLIJ_CONFIG_DIR="$HOME/.config/zellij"
