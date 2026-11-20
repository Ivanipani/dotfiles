# Vendored in configs/zsh/plugins (linked to $XDG_CONFIG_HOME/zsh/plugins).
_dot_plugins="$XDG_CONFIG_HOME/zsh/plugins"

# Not for `zsh -ic CMD`: no prompt is drawn, and p10k would start gitstatus,
# whose lock file collides with a shell CMD `exec`s (same pid and second).
[[ -n $ZSH_EXECUTION_STRING ]] || source "$_dot_plugins/powerlevel10k/powerlevel10k.zsh-theme"
source "$_dot_plugins/zsh-autosuggestions/zsh-autosuggestions.plugin.zsh"
source "$_dot_plugins/zsh-vi-mode/zsh-vi-mode.plugin.zsh"
# Must load last so it wraps every widget defined above.
source "$_dot_plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.plugin.zsh"

unset _dot_plugins
