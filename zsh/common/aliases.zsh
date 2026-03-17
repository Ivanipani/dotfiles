alias l="ls -lh"
alias la="ls -lha"
alias lt="ls -ltr"
alias lat="ls -latr"

alias v="nvim"
alias vimdiff="nvim -d"

alias ec="$EDITOR $HOME/.zshrc" # edit .zshrc
alias sc="source $HOME/.zshrc"  # reload zsh configuration

alias docker-stop-all='docker stop $(docker ps -aq) && docker rm $(docker ps -aq)'

