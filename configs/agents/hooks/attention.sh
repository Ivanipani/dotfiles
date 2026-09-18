#!/bin/sh
# Mark the current pane as waiting or completed.
set -eu
[ -n "${ZELLIJ:-}" ] && zellij pipe --name "zellij-attention::${1:?state required}::${ZELLIJ_PANE_ID:-}" >/dev/null 2>&1 || true
