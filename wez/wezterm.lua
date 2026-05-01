local wezterm = require 'wezterm'
local config = {}

config.color_scheme = "Ayu Mirage"
config.font = wezterm.font "Inconsolata Nerd Font"

config.keys = {
    {key = "L", mods = "CTRL", action = wezterm.action.ShowDebugOverlay},
    {key = "p", mods = "CTRL|SHIFT", action = wezterm.action.SpawnCommandInNewWindow {}},
}

config.term = "xterm-256color"
return config
