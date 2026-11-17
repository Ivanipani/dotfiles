-- Migrated from zellij (default_mode "locked": Rex with no mode active).
-- zellij's "normal" mode is the Rex mode "command", entered with ctrl+a.

local modes = { "command", "pane", "tab", "resize", "move", "scroll", "session", "workspace" }
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
switches("m", "move",    { "command", "pane", "tab", "resize", "scroll", "session", "workspace" })
switches("o", "session", { "command", "pane", "tab", "resize", "move", "scroll" })
switches("t", "tab",     { "command", "pane", "resize", "move", "scroll", "session", "workspace" })
switches("p", "pane",    { "command", "tab", "resize", "scroll", "workspace" })
switches("s", "scroll",  { "command", "resize", "move" })
switches("r", "resize",  { "command", "move", "scroll", "session", "workspace" })
switches("w", "workspace", { "command", "pane", "tab", "resize", "move", "scroll", "session" })

-- Back to command mode (zellij "normal")
for key, m in pairs({ p = "pane", t = "tab", r = "resize", m = "move", s = "scroll", o = "session", w = "workspace" }) do
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

-- workspace sets (rex-workspace): a float runs the picker, jjui is a float per project tab
rex.action{
  name  = "workspace_float",
  title = "Run a rex-workspace command in a floating pane",
  args  = { sub = "string" },
  run   = function(ctx, args)
    rex.call("session.new_layer", {
      session_id = ctx.session_id,
      bounds = { 0.15, 0.2, 0.7, 0.6 },
      focus = true,
      layout = { block = {
        flavor = "com.superlogical.terminal.shell",
        label = "workspace",
        options = { command = { "zsh", "-lic", "exec rex-workspace " .. args.sub } },
      } },
    })
    rex.client.queue("client.mode.exit_all")
  end,
}

rex.action{
  name  = "workspace_jjui",
  title = "Show or hide the active tab's jjui float",
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
for key, sub in pairs({ o = "open", n = "new", a = "add", x = "remove", d = "delete" }) do
  rex.bind("workspace/" .. key, "workspace_float", { sub = sub })
end
rex.bind("workspace/j", "then_normal", { action = "workspace_jjui" })
