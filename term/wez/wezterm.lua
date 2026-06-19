local wezterm = require 'wezterm'
local config = {}

config.color_scheme = "Ayu Mirage"
config.font = wezterm.font "Inconsolata Nerd Font"

config.keys = {
    {key = "L", mods = "CTRL", action = wezterm.action.ShowDebugOverlay},
    {key = "p", mods = "CTRL|SHIFT", action = wezterm.action.SpawnCommandInNewWindow {}},
}

config.term = "xterm-256color"

-- Ensure nushell (and any XDG-aware tool) uses ~/.config when launched as the
-- login shell, where zsh never runs to export this.
config.set_environment_variables = {
    XDG_CONFIG_HOME = wezterm.home_dir .. "/.config",
}

return config
