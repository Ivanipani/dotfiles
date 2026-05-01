prompt off

export ZPLUG_HOME=$HOME/.zplug
export XDG_CONFIG_HOME="$HOME/.config"
ZSH_CONFIG_HOME="$XDG_CONFIG_HOME/zsh"

# export DOTFILE_DIR=$(realpath $HOME/.zshrc/../../..)
export DOTFILE_DIR=$(dirname "$(dirname "$(dirname "$(realpath "$HOME/.zshrc")")")")

source $XDG_CONFIG_HOME/zsh/01-env.zsh
source $XDG_CONFIG_HOME/zsh/02-aliases.zsh
source $XDG_CONFIG_HOME/zsh/03-functions.zsh
source $XDG_CONFIG_HOME/zsh/04-integrations.zsh
source $XDG_CONFIG_HOME/zsh/05-options.zsh
source $XDG_CONFIG_HOME/zsh/06-plugins.zsh


