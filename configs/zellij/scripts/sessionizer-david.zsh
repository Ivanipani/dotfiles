#!/usr/bin/env zsh
# Zellij Session Manager — thin launcher.
#
# Fuzzy-finding directories, layout selection, and session switching used to
# live here (fzf + fd + zellij-switch). That logic is now the zellij-sessionizer
# plugin (src/zellij-sessionizer), aliased as "sessionizer" in config.kdl. A
# plugin only runs inside a zellij session, so this script's only job is
# making sure one exists, then opening it.
set -e

if [[ -n "${ZELLIJ:-}" ]]; then
    zellij action launch-or-focus-plugin --floating --move-to-focused-tab sessionizer
else
    # No session yet to launch a plugin into. Start one with the normal
    # default layout, then reopen this (session mode -> f) once inside.
    exec zellij
fi
