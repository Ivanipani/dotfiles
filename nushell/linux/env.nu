if (which atuin | is-not-empty) {atuin init nu | save -f ~/.atuin.nu}

const NU_PLUGIN_DIRS = [
  ($nu.current-exe | path dirname)
  ...$NU_PLUGIN_DIRS
]
