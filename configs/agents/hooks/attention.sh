#!/bin/sh
# Mark the current pane as waiting or completed.
set -eu
# </dev/null: with no payload, `zellij pipe` reads stdin until EOF and hangs on the hook's open pipe.
[ -n "${ZELLIJ:-}" ] && zellij pipe --name "zellij-attention::${1:?state required}::${ZELLIJ_PANE_ID:-}" </dev/null >/dev/null 2>&1 || true
