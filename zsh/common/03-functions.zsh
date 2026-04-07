if (( $+commands[just] && $+commands[zellij] )); then

  zle -N ns
  bindkey '^f' ns

  function cs {
    emulate -L zsh -o pipefail
    setopt localoptions localtraps no_aliases

    TRAPINT() { zle -I; zle reset-prompt; return 130 }

    zle -I

    $DOTFILES/zellij/scripts/sessionizer-david.zsh
    local ret=$?

    zle reset-prompt
    return $ret
  }

  zle -N cs
else
  echo "[zsh] just/zellij not found, skipping sessionizer (ns, cs)" >&2
fi

if (( $+commands[fd] && $+commands[fzf] )); then
  function f {
    local selection
    selection=$(fd --type directory --base-directory "$HOME" \
                   | fzf --height=50% --padding=1% --border=double \
                         --header="CTRL-C or ESC to quit") || return

    local target="$HOME/$selection"

    if [[ -d $target ]]; then
      cd -- "$target" || echo "Failed to cd into '$target'"
    else
      echo "Directory '$target' does not exist."
    fi
  }
else
  echo "[zsh] fd/fzf not found, skipping directory picker (f)" >&2
fi
