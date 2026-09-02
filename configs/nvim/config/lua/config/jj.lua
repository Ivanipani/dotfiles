-- Entry points for jj's editors. `:JJResolve` is what `jj resolve` launches
-- (see vcs/jj/config.toml); the autocmd covers conflicted files opened
-- normally, since jj materialises conflict markers into the working copy.
local AUGROUP = vim.api.nvim_create_augroup("jj-conflict-detect", {clear = true})

local function conflict() return require("jj.conflict") end

--- jj hands the merge tool `$output` plus the three sides as sibling temp
--- files. Fall back to deriving them from the buffer name when the paths did
--- not survive being spliced into a single `-c` argument (e.g. spaces).
local SIDES = {"left", "base", "right"}

local function readable_sides(paths)
    local sides = {}
    for i, side in ipairs(SIDES) do
        if vim.fn.filereadable(paths[i] or "") == 0 then return nil end
        sides[side] = paths[i]
    end
    return sides
end

local function resolve_sides(args)
    local sides = readable_sides(args)
    if sides then return sides end

    local dir, name = vim.api.nvim_buf_get_name(0):match("^(.*)/output_(.*)$")
    if not dir then return nil end

    return readable_sides(vim.tbl_map(function(side)
        return ("%s/%s_%s"):format(dir, side, name)
    end, SIDES))
end

--- Open the three sides jj gave us in a scratch tab, in diff mode.
local function diff3()
    local sides = vim.b.jj_resolve_sides
    if not sides then
        vim.notify("No left/base/right files for this buffer",
                   vim.log.levels.WARN)
        return
    end

    vim.cmd("tabnew")
    for i, side in ipairs(SIDES) do
        if i > 1 then vim.cmd("vsplit") end
        vim.cmd("edit " .. vim.fn.fnameescape(sides[side]))
        vim.bo.modifiable = false
        vim.bo.buflisted = false
        vim.wo.winbar = "%#JjConflictMarker# " .. side .. " %*"
        vim.cmd("diffthis")
        vim.keymap.set("n", "q", "<cmd>tabclose<cr>",
                       {buffer = true, nowait = true, desc = "Close diff"})
    end
    vim.cmd("wincmd t")
end

local function finish()
    local remaining = conflict().count(0)
    if remaining > 0 then
        local choice = vim.fn.confirm(
                           ("%d conflict(s) still marked. Hand back to jj anyway?"):format(
                               remaining), "&Yes\n&No", 2)
        if choice ~= 1 then return end
    end
    vim.cmd("wqall")
end

local function abort()
    if vim.fn.confirm("Abort without resolving?", "&Yes\n&No", 2) == 1 then
        -- Non-zero exit is how jj is told the resolution was cancelled.
        vim.cmd("cquit")
    end
end

vim.api.nvim_create_user_command("JJResolve", function(params)
    local buf = vim.api.nvim_get_current_buf()
    vim.b[buf].jj_resolve_sides = resolve_sides(params.fargs)

    local function map(lhs, rhs, desc)
        vim.keymap.set("n", lhs, rhs, {buffer = buf, desc = desc, nowait = true})
    end
    map("<leader>jd", diff3, "[D]iff left / base / right")

    -- `finish` and `abort` exit nvim, so they are only safe when jj spawned us
    -- as the merge tool -- which is exactly when its temp files are alongside.
    if vim.b[buf].jj_resolve_sides then
        vim.b[buf].jj_resolve_session = true
        map("<C-s>", finish, "Finish and hand back to jj")
        map("<leader>js", finish, "[S]ave and hand back to jj")
        map("q", abort, "Quit without resolving")
        map("<leader>jq", abort, "[Q]uit without resolving")
    end

    vim.wo.winbar = "%{%v:lua.require'jj.conflict'.winbar()%}"

    if conflict().watch(buf) > 0 then
        conflict().goto_conflict("next")
    else
        vim.notify("No conflict markers found in " .. vim.fn.expand("%:t"),
                   vim.log.levels.WARN)
    end
end, {nargs = "*", desc = "Resolve jj conflict markers in the current buffer"})

vim.api.nvim_create_autocmd({"BufReadPost", "BufWritePost"}, {
    group = AUGROUP,
    desc = "Attach the conflict UI to files jj left markers in",
    callback = function(event)
        if vim.bo[event.buf].buftype ~= "" then return end
        if event.buf ~= vim.api.nvim_get_current_buf() then return end
        -- Cheap prefilter so this stays free for ordinary files.
        if vim.fn.search("^<<<<<<<", "nw") == 0 then return end
        conflict().watch(event.buf)
    end
})
