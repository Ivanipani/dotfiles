-- Migrated from zellij (default_mode "locked": Rex with no mode active).
-- zellij's "normal" mode is the Rex mode "command", entered with ctrl+a.

local modes = { "command", "pane", "tab", "resize", "move", "scroll", "session", "float" }
for _, m in ipairs(modes) do
  rex.mode(m, { exclusive = true })
end

-- The stepper lists each key by its action's title, so every key that needs a
-- wrapper gets its own short, specific action instead of a generic one.
-- `section` is the menu section (category) of every action defined after it.
local section = "Modes"
local function act(name, title, run)
  rex.action{ name = name, title = title, category = section, run = run }
  return name
end

-- Leave every mode, then run an action. Leaving first keeps a picker the action
-- opens (go to, switch session) from being dismissed by the mode exit.
local function done(name, title, action, args)
  return act(name, title, function()
    rex.client.queue("client.mode.exit_all")
    rex.client.queue(action, args or {})
  end)
end

-- Run an action and stay in the mode.
local function step(name, title, action, args)
  return act(name, title, function() rex.client.queue(action, args or {}) end)
end

-- Leave every mode and enter another.
local mode_titles = {
  command = "Command", pane = "Pane", tab = "Tab", resize = "Resize",
  move = "Move", scroll = "Scroll", session = "Session",
  float = "Float",
}
local enter_mode = {}
for m, title in pairs(mode_titles) do
  enter_mode[m] = act("goto_" .. m, title, function()
    rex.client.queue("client.mode.exit_all")
    rex.client.queue("client.mode.enter", { name = m })
  end)
end

section = "Command"

-- locked: ctrl+a enters command mode
rex.bind("ctrl+a", enter_mode.command)

-- No exit keys are bound in modes: Rex leaves a mode on esc by itself, and a
-- binding would list "Exit All Modes" in every mode's menu.

-- Global keys, outside any mode: alt+j/k change tab, alt+h/l change session,
-- ctrl+h/j/k/l move pane focus
for key, action in pairs({
  ["alt+k"] = "client.tab.previous", ["alt+j"] = "client.tab.next",
  ["alt+h"] = "session.previous",    ["alt+l"] = "session.next",
}) do rex.bind(key, action) end
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("ctrl+" .. key, "pane.focus", { direction = dir })
end

-- Mode switches from command mode
for key, target in pairs({ m = "move", o = "session", t = "tab", p = "pane", s = "scroll",
                           r = "resize", l = "float" }) do
  rex.bind("command/" .. key, enter_mode[target])
end

-- Back to command mode (zellij "normal")
for key, m in pairs({ p = "pane", t = "tab", r = "resize", m = "move", s = "scroll", o = "session" }) do
  rex.bind(m .. "/" .. key, enter_mode.command)
end

-- alt+ keys work outside modes only; the command menu lists just modes (and go-to).
rex.bind("alt+i", "client.tab.move", { direction = "backward" })
rex.bind("alt+o", "client.tab.move", { direction = "forward" })
rex.bind("alt+n", "pane.split", { direction = "auto" })
rex.bind("alt+f", "pane.go_to")
-- Bound directly (not queued): a queued pane.go_to never opens the switcher.
rex.bind("command/f", "pane.go_to")

-- pane
section = "Pane"
rex.bind("pane/f", done("pane_zoom", "Zoom pane", "pane.zoom"))
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("pane/" .. key, done("split_" .. dir, "Split " .. dir, "pane.split", { direction = dir }))
end
rex.bind("pane/r", done("pane_rename", "Rename pane", "pane.rename"))
rex.bind("pane/d", done("pane_close", "Close pane", "pane.close"))

-- tab
section = "Tab"
rex.bind("tab/b", done("pane_to_tab", "Pane to tab", "pane.move_to_new_tab"))
rex.bind("tab/h", step("tab_previous", "Previous tab", "client.tab.previous"))
rex.bind("tab/l", step("tab_next", "Next tab", "client.tab.next"))
rex.bind("tab/n", done("tab_new", "New tab", "client.tab.new"))
rex.bind("tab/r", done("tab_rename", "Rename tab", "client.tab.rename"))
rex.bind("tab/d", done("tab_close", "Close tab", "client.tab.close"))

-- session
section = "Session"
rex.bind("session/j", step("session_next", "Next", "session.next"))
rex.bind("session/k", step("session_previous", "Previous", "session.previous"))
rex.bind("session/f", done("session_pick", "Switch", "session.switch"))
rex.bind("session/n", done("session_new", "New", "session.new"))
rex.bind("session/r", done("session_rename", "Rename", "session.rename"))
rex.bind("session/d", done("session_close", "Close", "session.close"))

-- scroll (rex scrolls by page)
section = "Scroll"
local page_down = step("scroll_page_down", "Page down", "client.scroll.page_down")
local page_up = step("scroll_page_up", "Page up", "client.scroll.page_up")
rex.bind("scroll/j", page_down)
rex.bind("scroll/k", page_up)
rex.bind("scroll/ctrl+d", page_down)
rex.bind("scroll/ctrl+u", page_up)
rex.bind("scroll//", done("scroll_find", "Find", "client.find.open"))
rex.bind("scroll/n", step("scroll_find_next", "Next match", "client.find.next"))
rex.bind("scroll/shift+n", step("scroll_find_previous", "Prev match", "client.find.previous"))

-- Floats --------------------------------------------------------------------
-- Global keys (locked and command mode):
--   ctrl+a w workspaces   alt+g jjui   alt+t scratch shell   alt+a claude   alt+c codex
--   alt+w hide/show all floats
--   alt+, / alt+.  focus the previous / next float
-- Float mode (ctrl+a, then l): n new, f float pane, e tile float, x close,
-- m hide, c center, z maximize, tab cycle, l back. Moving and resizing a float
-- is done in the move and resize modes, which act on the focused float.

local TERMINAL = "com.superlogical.terminal"
local STEP = 0.05
local MIN = 0.15

local function call(ctx, method, args)
  args.session_id = ctx.session_id
  local result, err = rex.call(method, args)
  if err ~= nil then error(method .. ": " .. tostring(err), 0) end
  return result
end

local function active_window(ctx)
  local view = call(ctx, "session.view", {})
  for _, win in ipairs(view.windows) do
    if win.window_id == view.active_window_id then return win end
  end
end

local function layer_has(layer, block_id)
  for _, b in ipairs(layer.blocks) do
    if b.block_id == block_id then return true end
  end
  return false
end

local function floats(win)
  local out = {}
  for _, layer in ipairs(win.layers) do
    if layer.kind == "floating" then out[#out + 1] = layer end
  end
  return out
end

-- The floating layer holding the focused block, with its index among floats.
local function focused_float(win)
  for i, layer in ipairs(floats(win)) do
    if layer_has(layer, win.focused_block_id) then return layer, i end
  end
end

local function find_float(win, label)
  for _, layer in ipairs(floats(win)) do
    for _, b in ipairs(layer.blocks) do
      if b.label == label then return layer end
    end
  end
end

local function show(ctx, layer)
  call(ctx, "session.set_layer_state",
    { layer_id = layer.layer_id, state = "normal", focus = true })
end

local function hide(ctx, layer)
  call(ctx, "session.set_layer_state", { layer_id = layer.layer_id, state = "minimized" })
end

-- Working directory of the focused terminal: its foreground program's, else the shell's.
local function focused_cwd(ctx)
  if not ctx.block_id then return nil end
  local ok, p = pcall(rex.block.call, TERMINAL, "process", { block_id = ctx.block_id })
  if not ok or type(p) ~= "table" then return nil end
  local fg, shell = p.foreground, p.ancestors and p.ancestors[1]
  return (fg and fg.cwd) or (shell and shell.cwd)
end

-- `command` is a shell command line, run by a login shell in the focused cwd.
local function open_float(ctx, label, command, bounds)
  local options = { cwd = focused_cwd(ctx) }
  if command then options.command = { "zsh", "-lic", command } end
  call(ctx, "session.new_layer", {
    bounds = bounds or { 0.1, 0.1, 0.8, 0.8 }, focus = true,
    layout = { block = { flavor = TERMINAL .. ".shell", label = label, options = options } },
  })
end

-- Show/focus/hide a named float. Creates it running `command` when missing.
local function toggle_float(label, command, bounds)
  return function(ctx)
    local win = active_window(ctx)
    local layer = find_float(win, label)
    if not layer then
      open_float(ctx, label, command, bounds)
    elseif layer.state == "minimized" or not layer_has(layer, win.focused_block_id) then
      show(ctx, layer)
    else
      hide(ctx, layer)
    end
  end
end

section = "Float"

-- A float action; `stay` keeps the current key mode active afterwards.
local function float_action(name, title, fn, stay)
  rex.action{
    name = name, title = title, category = section,
    run = function(ctx)
      fn(ctx, active_window(ctx))
      if not stay then rex.client.queue("client.mode.exit_all") end
    end,
  }
  return name
end

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Apply `change(x, y, w, h)` to the focused float's bounds, kept inside the window.
local function reshape(change)
  return function(ctx, win)
    local layer = focused_float(win)
    if not layer then return end
    local b = layer.bounds
    local x, y, w, h = change(b[1], b[2], b[3], b[4])
    w, h = clamp(w, MIN, 1), clamp(h, MIN, 1)
    call(ctx, "session.resize_layer", {
      layer_id = layer.layer_id,
      bounds = { clamp(x, 0, 1 - w), clamp(y, 0, 1 - h), w, h },
    })
  end
end

local function cycle(delta)
  return function(ctx, win)
    local all = floats(win)
    if #all == 0 then return end
    local _, i = focused_float(win)
    show(ctx, all[((i or (delta > 0 and 0 or 1)) - 1 + delta) % #all + 1])
  end
end

local function any_visible(win)
  for _, layer in ipairs(floats(win)) do
    if layer.state ~= "minimized" then return true end
  end
end

local function toggle_all(ctx, win)
  local hiding = any_visible(win)
  for _, layer in ipairs(floats(win)) do
    if hiding then hide(ctx, layer) else call(ctx, "session.set_layer_state",
      { layer_id = layer.layer_id, state = "normal" }) end
  end
end

rex.action{ name = "workspace_jjui", category = section, title = "jjui", run = toggle_float("jjui", "exec jjui", { 0.05, 0.05, 0.9, 0.9 }) }
rex.action{ name = "float_scratch", category = section, title = "Scratch shell", run = toggle_float("scratch", "exec zsh") }
rex.action{ name = "float_claude", category = section, title = "Claude", run = toggle_float("claude", "exec claude") }
rex.action{ name = "float_codex", category = section, title = "Codex", run = toggle_float("codex", "exec codex") }
float_action("float_toggle_all", "Hide/show all", toggle_all, true)
float_action("float_prev", "Prev float", cycle(-1), true)
float_action("float_next", "Next float", cycle(1), true)

for key, action in pairs({
  ["alt+g"] = "workspace_jjui", ["alt+t"] = "float_scratch", ["alt+a"] = "float_claude", ["alt+c"] = "float_codex",
  ["alt+w"] = "float_toggle_all", ["alt+[Comma]"] = "float_prev", ["alt+[Period]"] = "float_next",
}) do
  rex.bind(key, action)
end

-- Workspace sets (rex-workspace): ctrl+a w pulls the manager TUI up as a
-- near-fullscreen float, from anywhere. It closes itself when the TUI exits.
section = "Command"
local toggle_workspace = toggle_float("workspace", "exec rex-workspace", { 0.03, 0.04, 0.94, 0.92 })
rex.bind("command/w", act("workspace", "Workspaces", function(ctx)
  rex.client.queue("client.mode.exit_all")
  toggle_workspace(ctx)
end))
section = "Float"

-- float mode
local function float_key(key, name, title, fn, stay)
  rex.bind("float/" .. key, float_action(name, title, fn, stay))
end

float_key("n", "float_new", "New float", function(ctx) open_float(ctx, "float") end)
float_key("f", "float_pane", "Float pane", function(ctx, win)
  call(ctx, "session.float_block", { block_id = ctx.block_id, bounds = { 0.1, 0.1, 0.8, 0.8 } })
end)
float_key("e", "float_tile", "Tile", function(ctx, win)
  local layer = focused_float(win)
  if not layer then return end
  call(ctx, "session.move_block", {
    block_id = win.focused_block_id, anchor_block_id = win.layers[1].blocks[1].block_id,
  })
end)
float_key("x", "float_close", "Close float", function(ctx, win)
  local layer = focused_float(win)
  if layer then call(ctx, "session.close_layer", { layer_id = layer.layer_id }) end
end)
float_key("m", "float_hide", "Hide float", function(ctx, win)
  local layer = focused_float(win)
  if layer then hide(ctx, layer) end
end)
float_key("w", "float_all_toggle", "Hide/show all", toggle_all)
float_key("c", "float_center", "Center",
  reshape(function(_, _, w, h) return (1 - w) / 2, (1 - h) / 2, w, h end), true)
float_key("z", "float_max", "Maximize",
  reshape(function() return 0.02, 0.02, 0.96, 0.96 end), true)
float_key("tab", "float_cycle", "Next float", cycle(1), true)
float_key("shift+tab", "float_cycle_back", "Prev float", cycle(-1), true)
float_key("space", "float_raise", "Raise float", function(ctx, win)
  local layer = focused_float(win)
  if layer then call(ctx, "session.raise_layer", { layer_id = layer.layer_id }) end
end, true)

rex.bind("float/l", enter_mode.command)

-- resize and move modes: act on the focused float, else on the tiled pane
local moves = {
  h = { "left",  function(x, y, w, h) return x - STEP, y, w, h end },
  j = { "down",  function(x, y, w, h) return x, y + STEP, w, h end },
  k = { "up",    function(x, y, w, h) return x, y - STEP, w, h end },
  l = { "right", function(x, y, w, h) return x + STEP, y, w, h end },
}
local sizes = {
  h = { "left",  function(x, y, w, h) return x, y, w - STEP, h end },
  j = { "down",  function(x, y, w, h) return x, y, w, h + STEP end },
  k = { "up",    function(x, y, w, h) return x, y, w, h - STEP end },
  l = { "right", function(x, y, w, h) return x, y, w + STEP, h end },
}

local function float_or_pane(name, title, change, action, dir)
  local on_float = reshape(change)
  rex.action{
    name = name, title = title, category = section,
    run = function(ctx)
      local win = active_window(ctx)
      if focused_float(win) then on_float(ctx, win)
      else rex.client.queue(action, { direction = dir }) end
    end,
  }
  return name
end

for key, v in pairs(sizes) do
  section = "Resize"
  rex.bind("resize/" .. key, float_or_pane("resize_" .. v[1], "Resize " .. v[1], v[2], "pane.resize", v[1]))
end
for key, v in pairs(moves) do
  section = "Move"
  rex.bind("move/" .. key, float_or_pane("move_" .. v[1], "Move " .. v[1], v[2], "pane.move", v[1]))
end
