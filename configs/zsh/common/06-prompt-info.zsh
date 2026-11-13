# Minimal stand-ins for the oh-my-zsh prompt helpers the vendored gnzh theme calls.
# They read the ZSH_THEME_* prefix/suffix variables the theme sets.

git_prompt_info() {
    local ref
    ref=$(git symbolic-ref --short HEAD 2>/dev/null) \
        || ref=$(git rev-parse --short HEAD 2>/dev/null) \
        || return 0
    echo "${ZSH_THEME_GIT_PROMPT_PREFIX}${ref}${ZSH_THEME_GIT_PROMPT_SUFFIX}"
}

virtualenv_prompt_info() {
    [[ -n "$VIRTUAL_ENV" ]] || return 0
    echo "${ZSH_THEME_VIRTUALENV_PREFIX}${VIRTUAL_ENV:t}${ZSH_THEME_VIRTUALENV_SUFFIX}"
}

# Ruby version segment is not wanted; the theme still calls it.
ruby_prompt_info() { :; }
