-- Migrated from zellij (default_mode "locked": Rex with no mode active).
-- zellij's "normal" mode is the Rex mode "command", entered with ctrl+a.

local modes = { "command", "pane", "tab", "resize", "move", "scroll", "session", "workspace" }
for _, m in ipairs(modes) do
  rex.mode(m, { exclusive = true })
end

-- The stepper lists each key by its action's title, so every key that needs a
-- wrapper gets its own short, specific action instead of a generic one.
local function act(name, title, run)
  rex.action{ name = name, title = title, run = run }
  return name
end

-- Run an action, then leave every mode.
local function done(name, title, action, args)
  return act(name, title, function()
    rex.client.queue(action, args or {})
    rex.client.queue("client.mode.exit_all")
  end)
end

-- Leave every mode and enter another.
local mode_titles = {
  command = "Command", pane = "Pane", tab = "Tab", resize = "Resize",
  move = "Move", scroll = "Scroll", session = "Session", workspace = "Workspace",
}
local enter_mode = {}
for m, title in pairs(mode_titles) do
  enter_mode[m] = act("goto_" .. m, title, function()
    rex.client.queue("client.mode.exit_all")
    rex.client.queue("client.mode.enter", { name = m })
  end)
end
local lock = act("lock", "Exit", function() rex.client.queue("client.mode.exit_all") end)

-- locked: ctrl+a enters command mode
rex.bind("ctrl+a", enter_mode.command)

-- Keys in every mode: esc, enter and ctrl+a lock again; alt+h/l change tab,
-- alt+j/k change session
local nav = {
  ["alt+h"] = "client.tab.previous", ["alt+l"] = "client.tab.next",
  ["alt+k"] = "session.previous",    ["alt+j"] = "session.next",
}
for _, m in ipairs(modes) do
  for _, key in ipairs({ "ctrl+a", "enter", "esc" }) do
    rex.bind(m .. "/" .. key, lock)
  end
  for key, action in pairs(nav) do rex.bind(m .. "/" .. key, action) end
end
for key, action in pairs(nav) do rex.bind(key, action) end

-- Global navigation (zellij "shared"): ctrl+h/j/k/l move pane focus in every mode
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("ctrl+" .. key, "pane.focus", { direction = dir })
  for _, m in ipairs(modes) do
    rex.bind(m .. "/ctrl+" .. key, "pane.focus", { direction = dir })
  end
end

-- Mode switches: key -> mode, in the modes where zellij binds it
local function switches(key, target, from)
  for _, m in ipairs(from) do
    rex.bind(m .. "/" .. key, enter_mode[target])
  end
end
switches("m", "move",    { "command", "pane", "tab", "resize", "scroll", "session", "workspace" })
switches("o", "session", { "command", "pane", "tab", "resize", "move", "scroll" })
switches("t", "tab",     { "command", "pane", "resize", "move", "scroll", "session", "workspace" })
switches("p", "pane",    { "command", "tab", "resize", "scroll", "workspace" })
switches("s", "scroll",  { "command", "resize", "move" })
switches("r", "resize",  { "command", "move", "scroll", "session", "workspace" })
switches("w", "workspace", { "command", "pane", "tab", "resize", "move", "scroll", "session" })

-- Back to command mode (zellij "normal")
for key, m in pairs({ p = "pane", t = "tab", r = "resize", m = "move", s = "scroll", o = "session", w = "workspace" }) do
  rex.bind(m .. "/" .. key, enter_mode.command)
end

-- shared_among normal/locked
for _, prefix in ipairs({ "", "command/" }) do
  rex.bind(prefix .. "alt+i", "client.tab.move", { direction = "backward" })
  rex.bind(prefix .. "alt+o", "client.tab.move", { direction = "forward" })
  rex.bind(prefix .. "alt+n", "pane.split", { direction = "auto" })
  rex.bind(prefix .. "alt+f", "pane.go_to")
end
rex.bind("command/f", done("go_to", "Go to pane or tab", "pane.go_to"))

-- pane
rex.bind("pane/f", done("pane_zoom", "Zoom pane", "pane.zoom"))
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("pane/" .. key, done("split_" .. dir, "Split " .. dir, "pane.split", { direction = dir }))
end
rex.bind("pane/r", done("pane_rename", "Rename pane", "pane.rename"))
rex.bind("pane/d", done("pane_close", "Close pane", "pane.close"))

-- tab
rex.bind("tab/b", done("pane_to_tab", "Move pane to new tab", "pane.move_to_new_tab"))
rex.bind("tab/h", "client.tab.previous")
rex.bind("tab/l", "client.tab.next")
rex.bind("tab/n", done("tab_new", "New tab", "client.tab.new"))
rex.bind("tab/r", done("tab_rename", "Rename tab", "client.tab.rename"))
rex.bind("tab/d", done("tab_close", "Close tab", "client.tab.close"))

-- session
rex.bind("session/j", "session.next")
rex.bind("session/k", "session.previous")
rex.bind("session/f", done("session_pick", "Switch session", "session.switch"))
rex.bind("session/n", done("session_new", "New session", "session.new"))
rex.bind("session/r", done("session_rename", "Rename session", "session.rename"))
rex.bind("session/d", done("session_close", "Close session", "session.close"))

-- scroll (rex scrolls by page)
rex.bind("scroll/j", "client.scroll.page_down")
rex.bind("scroll/k", "client.scroll.page_up")
rex.bind("scroll/ctrl+d", "client.scroll.page_down")
rex.bind("scroll/ctrl+u", "client.scroll.page_up")
rex.bind("scroll//", done("scroll_find", "Find", "client.find.open"))
rex.bind("scroll/n", "client.find.next")
rex.bind("scroll/shift+n", "client.find.previous")

-- resize (grow) and move
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("resize/" .. key, "pane.resize", { direction = dir })
  rex.bind("move/" .. key, "pane.move", { direction = dir })
end

-- workspace sets (rex-workspace): a float runs the picker, jjui is a float per project tab
local function workspace_float(sub)
  return function(ctx)
    rex.call("session.new_layer", {
      session_id = ctx.session_id,
      bounds = { 0.15, 0.2, 0.7, 0.6 },
      focus = true,
      layout = { block = {
        flavor = "com.superlogical.terminal.shell",
        label = "workspace",
        options = { command = { "zsh", "-lic", "exec rex-workspace " .. sub } },
      } },
    })
    rex.client.queue("client.mode.exit_all")
  end
end

rex.action{
  name  = "workspace_jjui",
  title = "Toggle jjui",
  run   = function(ctx)
    local view = rex.call("session.view", { session_id = ctx.session_id })
    for _, win in ipairs(view.windows) do
      if win.window_id == view.active_window_id then
        for _, layer in ipairs(win.layers) do
          for _, b in ipairs(layer.blocks) do
            if layer.kind == "floating" and b.label == "jjui" then
              local show = layer.state == "minimized"
              rex.call("session.set_layer_state", {
                session_id = ctx.session_id, layer_id = layer.layer_id,
                state = show and "normal" or "minimized", focus = show,
              })
              return
            end
          end
        end
      end
    end
  end,
}

rex.bind("alt+g", "workspace_jjui")
rex.bind("command/alt+g", "workspace_jjui")
local workspace_keys = {
  o = { "open", "Open workspace" }, n = { "new", "New workspace" },
  a = { "add", "Add repo to workspace" }, x = { "remove", "Remove repo from workspace" },
  d = { "delete", "Delete workspace" },
}
for key, v in pairs(workspace_keys) do
  rex.bind("workspace/" .. key, act("workspace_" .. v[1], v[2], workspace_float(v[1])))
end
rex.bind("workspace/j", done("jjui_toggle", "Toggle jjui", "workspace_jjui"))
