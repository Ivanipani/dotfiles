source $ZPLUG_HOME/init.zsh

zplug "robbyrussell/oh-my-zsh", use:"$OMZSH_USE"
zplug "themes/gnzh", from:oh-my-zsh, as:theme
zplug "zsh-users/zsh-autosuggestions"
zplug "zsh-users/zsh-syntax-highlighting", defer:2

if ! zplug check; then
  zplug install
fi

zplug load
