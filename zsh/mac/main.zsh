export ZPLUG_HOME=/opt/homebrew/opt/zplug
export XDG_CONFIG_HOME="$HOME/.config"

export DOTFILE_DIR=$(realpath $HOME/.zshrc/../../..)

source $XDG_CONFIG_HOME/zsh/01-env.zsh
source $XDG_CONFIG_HOME/zsh/02-aliases.zsh
source $XDG_CONFIG_HOME/zsh/03-functions.zsh
source $XDG_CONFIG_HOME/zsh/04-integrations.zsh
source $XDG_CONFIG_HOME/zsh/05-options.zsh
source $XDG_CONFIG_HOME/zsh/06-plugins.zsh


# The next line updates PATH for the Google Cloud SDK.
if [ -f '/Users/ivanperdomo/github/ivanipani/doghouse/google-cloud-sdk/path.zsh.inc' ]; then . '/Users/ivanperdomo/github/ivanipani/doghouse/google-cloud-sdk/path.zsh.inc'; fi

# The next line enables shell command completion for gcloud.
if [ -f '/Users/ivanperdomo/github/ivanipani/doghouse/google-cloud-sdk/completion.zsh.inc' ]; then . '/Users/ivanperdomo/github/ivanipani/doghouse/google-cloud-sdk/completion.zsh.inc'; fi
