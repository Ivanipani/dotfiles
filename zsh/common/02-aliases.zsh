alias l="ls -lh --color=auto"
alias la="ls -lha --color=auto"
alias lt="ls -ltr --color=auto"
alias lat="ls -latr --color=auto"

alias v="nvim"
alias vimdiff="nvim -d"

alias ec="$EDITOR $HOME/.zshrc" # edit .zshrc
alias sc="source $HOME/.zshrc"  # reload zsh configuration

alias docker-stop-all='docker stop $(docker ps -aq) && docker rm $(docker ps -aq)'

alias k="kubectl"
