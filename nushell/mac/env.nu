# env.nu — runs first at startup, before config.nu.
# Search paths (NU_LIB_DIRS / NU_PLUGIN_DIRS) are set in config.nu, since a
# `const` only applies to the file it is declared in and config.nu is the
# canonical place (matching Nushell's default config).

if (which atuin | is-not-empty) { atuin init nu | save -f ~/.atuin.nu }
