-- Migrated from zellij (default_mode "locked": Rex with no mode active).
-- zellij's "normal" mode is the Rex mode "command", entered with ctrl+a.

local modes = { "command", "pane", "tab", "resize", "move", "scroll", "session" }
for _, m in ipairs(modes) do
  rex.mode(m, { exclusive = true })
end

rex.action{
  name  = "then_normal",
  title = "Run an action and return to normal mode",
  args  = { action = "string", args = "object?" },
  run   = function(ctx, args)
    rex.client.queue(args.action, args.args or {})
    rex.client.queue("client.mode.exit_all")
  end,
}

rex.action{
  name  = "switch_mode",
  title = "Leave every mode and enter another",
  args  = { name = "string" },
  run   = function(ctx, args)
    rex.client.queue("client.mode.exit_all")
    rex.client.queue("client.mode.enter", { name = args.name })
  end,
}

-- locked: ctrl+a enters command mode
rex.bind("ctrl+a", "client.mode.enter", { name = "command" })

-- Keys in every mode: esc, enter and ctrl+a lock again; alt+h/l change tab
for _, m in ipairs(modes) do
  rex.bind(m .. "/ctrl+a", "client.mode.exit_all")
  rex.bind(m .. "/enter", "client.mode.exit_all")
  rex.bind(m .. "/esc", "client.mode.exit_all")
  rex.bind(m .. "/alt+h", "client.tab.previous")
  rex.bind(m .. "/alt+l", "client.tab.next")
end
rex.bind("alt+h", "client.tab.previous")
rex.bind("alt+l", "client.tab.next")

-- Mode switches: key -> mode, in the modes where zellij binds it
local function switches(key, target, from)
  for _, m in ipairs(from) do
    rex.bind(m .. "/" .. key, "switch_mode", { name = target })
  end
end
switches("m", "move",    { "command", "pane", "tab", "resize", "scroll", "session" })
switches("o", "session", { "command", "pane", "tab", "resize", "move", "scroll" })
switches("t", "tab",     { "command", "pane", "resize", "move", "scroll", "session" })
switches("p", "pane",    { "command", "tab", "resize", "scroll" })
switches("s", "scroll",  { "command", "resize", "move" })
switches("r", "resize",  { "command", "move", "scroll", "session" })

-- Back to command mode (zellij "normal")
for key, m in pairs({ p = "pane", t = "tab", r = "resize", m = "move", s = "scroll", o = "session" }) do
  rex.bind(m .. "/" .. key, "switch_mode", { name = "command" })
end

-- shared_among normal/locked
for _, prefix in ipairs({ "", "command/" }) do
  rex.bind(prefix .. "alt+i", "client.tab.move", { direction = "backward" })
  rex.bind(prefix .. "alt+o", "client.tab.move", { direction = "forward" })
  rex.bind(prefix .. "alt+n", "pane.split", { direction = "auto" })
end

-- pane
rex.bind("pane/f", "then_normal", { action = "pane.zoom" })
rex.bind("pane/h", "then_normal", { action = "pane.split", args = { direction = "left" } })
rex.bind("pane/j", "then_normal", { action = "pane.split", args = { direction = "down" } })
rex.bind("pane/k", "then_normal", { action = "pane.split", args = { direction = "up" } })
rex.bind("pane/l", "then_normal", { action = "pane.split", args = { direction = "right" } })
rex.bind("pane/r", "then_normal", { action = "pane.rename" })
rex.bind("pane/d", "then_normal", { action = "pane.close" })

-- tab
rex.bind("tab/b", "then_normal", { action = "pane.move_to_new_tab" })
rex.bind("tab/h", "client.tab.previous")
rex.bind("tab/l", "client.tab.next")
rex.bind("tab/n", "then_normal", { action = "client.tab.new" })
rex.bind("tab/r", "then_normal", { action = "client.tab.rename" })
rex.bind("tab/d", "then_normal", { action = "client.tab.close" })

-- resize (grow)
for key, dir in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  rex.bind("resize/" .. key, "pane.resize", { direction = dir })
  rex.bind("move/" .. key, "pane.move", { direction = dir })
end
