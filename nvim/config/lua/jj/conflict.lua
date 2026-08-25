-- Conflict-marker editor used as jj's merge tool, and auto-attached to any
-- buffer jj materialised conflicts into. Assumes git-style markers, which
-- vcs/jj/config.toml sets via `ui.conflict-marker-style = "git"`:
--
--     <<<<<<< <side 1 label>      -- "ours"
--     ...
--     ||||||| <base label>        -- optional, only in diff3 output
--     ...
--     =======
--     ...                         -- "theirs"
--     >>>>>>> <side 2 label>
--
-- jj falls back to its own `%%%%%%%`/`+++++++` style for conflicts with more
-- than two sides; those blocks are skipped rather than mis-parsed.
local M = {}

local NS = vim.api.nvim_create_namespace("jj-conflict")
local AUGROUP = vim.api.nvim_create_augroup("jj-conflict", {clear = true})
local MIN_MARKER = 7

local HIGHLIGHTS = {
    JjConflictOurs = "DiffAdd",
    JjConflictBase = "DiffChange",
    JjConflictTheirs = "DiffText",
    JjConflictMarker = "Visual",
    JjConflictHint = "Comment"
}

local KEYS = {
    {"co", "ours", "Take ours (side 1)"}, {"ct", "theirs", "Take theirs (side 2)"},
    {"cb", "base", "Take base"}, {"ca", "both", "Take both sides"},
    {"cn", "none", "Take neither"}
}

local function define_highlights()
    for name, link in pairs(HIGHLIGHTS) do
        vim.api.nvim_set_hl(0, name, {link = link, default = true})
    end
end

--- Length of a run of `char` starting the line, if it is a marker.
---@return integer?
local function marker_length(line, char)
    local n = 0
    while line:sub(n + 1, n + 1) == char do n = n + 1 end
    if n >= MIN_MARKER then return n end
end

--- Text following a marker, e.g. `zuzrtpuu 57bc871a "left"`.
local function marker_label(line, len)
    return vim.trim(line:sub(len + 1))
end

--- Parse every two-sided conflict in `bufnr`.
--- Line numbers are 1-indexed; content ranges are inclusive and may be empty
--- (`stop < start`) when a side contributes no lines.
---@return table[]
function M.parse(bufnr)
    local lines = vim.api.nvim_buf_get_lines(bufnr or 0, 0, -1, false)
    local conflicts, cur = {}, nil

    for i, line in ipairs(lines) do
        local len = marker_length(line, "<")
        if len then
            cur = {
                ours_marker = i,
                ours_label = marker_label(line, len),
                ours = {i + 1, i}
            }
        elseif cur then
            len = marker_length(line, "|")
            if len then
                cur.base_marker = i
                cur.base_label = marker_label(line, len)
                cur.ours[2] = i - 1
                cur.base = {i + 1, i}
            elseif marker_length(line, "=") then
                cur.sep = i;
                (cur.base or cur.ours)[2] = i - 1
                cur.theirs = {i + 1, i}
            else
                len = marker_length(line, ">")
                if len then
                    -- A block with no `=======` is a >2-sided jj conflict.
                    if cur.sep then
                        cur.theirs[2] = i - 1
                        cur.end_marker = i
                        cur.theirs_label = marker_label(line, len)
                        table.insert(conflicts, cur)
                    end
                    cur = nil
                end
            end
        end
    end

    return conflicts
end

local function mark(bufnr, lnum, group, hint)
    vim.api.nvim_buf_set_extmark(bufnr, NS, lnum - 1, 0, {
        line_hl_group = group,
        virt_text = hint and {{" " .. hint .. " ", "JjConflictHint"}} or nil,
        virt_text_pos = "eol"
    })
end

local function mark_range(bufnr, range, group)
    for lnum = range[1], range[2] do mark(bufnr, lnum, group) end
end

local function render(bufnr, conflicts)
    vim.api.nvim_buf_clear_namespace(bufnr, NS, 0, -1)

    for _, c in ipairs(conflicts) do
        mark(bufnr, c.ours_marker, "JjConflictMarker", "co")
        mark_range(bufnr, c.ours, "JjConflictOurs")
        if c.base_marker then
            mark(bufnr, c.base_marker, "JjConflictMarker", "cb")
            mark_range(bufnr, c.base, "JjConflictBase")
        end
        mark(bufnr, c.sep, "JjConflictMarker", "ct")
        mark_range(bufnr, c.theirs, "JjConflictTheirs")
        mark(bufnr, c.end_marker, "JjConflictMarker", "ca both  cn none")
    end
end

--- Conflict the cursor sits in, else the next one below it, else the last.
local function conflict_at_cursor(conflicts)
    local row = vim.api.nvim_win_get_cursor(0)[1]
    for _, c in ipairs(conflicts) do
        if row <= c.end_marker then return c end
    end
    return conflicts[#conflicts]
end

local function jump_to(conflict)
    vim.api.nvim_win_set_cursor(0, {conflict.ours_marker, 0})
    vim.cmd("normal! zz")
end

function M.goto_conflict(direction)
    local conflicts = M.parse(0)
    if #conflicts == 0 then
        vim.notify("No conflicts in this buffer", vim.log.levels.INFO)
        return
    end

    local row = vim.api.nvim_win_get_cursor(0)[1]
    if direction == "next" then
        for _, c in ipairs(conflicts) do
            if c.ours_marker > row then return jump_to(c) end
        end
        return jump_to(conflicts[1])
    end

    for i = #conflicts, 1, -1 do
        if conflicts[i].end_marker < row then return jump_to(conflicts[i]) end
    end
    return jump_to(conflicts[#conflicts])
end

local function slice(lines, range)
    return vim.list_slice(lines, range[1], range[2])
end

--- Replace the conflict under the cursor with one of its sides.
---@param choice "ours"|"theirs"|"base"|"both"|"none"
function M.resolve(choice)
    local bufnr = vim.api.nvim_get_current_buf()
    local conflicts = M.parse(bufnr)
    local c = conflict_at_cursor(conflicts)
    if not c then
        vim.notify("No conflicts in this buffer", vim.log.levels.INFO)
        return
    end

    if choice == "base" and not c.base then
        vim.notify("This conflict has no base section", vim.log.levels.WARN)
        return
    end

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    local kept = {}
    if choice == "ours" then
        kept = slice(lines, c.ours)
    elseif choice == "theirs" then
        kept = slice(lines, c.theirs)
    elseif choice == "base" then
        kept = slice(lines, c.base)
    elseif choice == "both" then
        kept = slice(lines, c.ours)
        vim.list_extend(kept, slice(lines, c.theirs))
    end

    vim.api.nvim_buf_set_lines(bufnr, c.ours_marker - 1, c.end_marker, false,
                               kept)
    vim.api.nvim_win_set_cursor(0, {math.min(c.ours_marker,
                                             vim.api.nvim_buf_line_count(bufnr)),
                                    0})

    M.refresh(bufnr)
    if #M.parse(bufnr) > 0 then M.goto_conflict("next") end
end

function M.count(bufnr) return #M.parse(bufnr or 0) end

function M.winbar()
    local remaining = M.count(0)
    local status = remaining == 0 and "%#DiffAdd# all resolved %*" or
                       ("%%#JjConflictMarker# %d left %%*"):format(remaining)
    local exit = vim.b.jj_resolve_session and " · <C-s> finish · q abort" or ""
    return table.concat({
        "%#JjConflictHint# ", vim.fn.expand("%:t"), " %*", status, "%<",
        "%#JjConflictHint#  co ours · ct theirs · cb base · ca both · cn none",
        " · ]x/[x nav", exit, " · g? help%*"
    })
end

local HELP = {
    "  jj conflict editor", "",
    "  ]x / [x     next / previous conflict",
    "  co          take ours   (side 1, above =======)",
    "  ct          take theirs (side 2, below =======)",
    "  cb          take base   (the ||||||| section)",
    "  ca          take both sides",
    "  cn          take neither",
    "  u           undo",
    "",
    "  Anything else is a normal edit -- write the file and jj",
    "  reads back whatever is left.", "",
    "  during `jj resolve`", "",
    "  <C-s>       hand the result back to jj",
    "  q           abort, leaving the conflict unresolved",
    "  <leader>jd  3-way diff of left / base / right", ""
}

function M.help()
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, HELP)
    vim.bo[buf].modifiable = false

    local width = 0
    for _, line in ipairs(HELP) do width = math.max(width, #line + 2) end

    local win = vim.api.nvim_open_win(buf, true, {
        relative = "editor",
        width = width,
        height = #HELP,
        row = math.floor((vim.o.lines - #HELP) / 2),
        col = math.floor((vim.o.columns - width) / 2),
        style = "minimal",
        border = "rounded",
        title = " help ",
        title_pos = "center"
    })
    vim.wo[win].winhighlight = "NormalFloat:Normal"

    for _, key in ipairs({"q", "<Esc>", "g?"}) do
        vim.keymap.set("n", key, "<cmd>close<cr>", {buffer = buf, nowait = true})
    end
end

local function map(bufnr, lhs, rhs, desc)
    vim.keymap.set("n", lhs, rhs, {buffer = bufnr, desc = desc, nowait = true})
end

local function set_conflict_keys(bufnr)
    if vim.b[bufnr].jj_conflict_keys then return end
    vim.b[bufnr].jj_conflict_keys = true

    map(bufnr, "]x", function() M.goto_conflict("next") end, "Next conflict")
    map(bufnr, "[x", function() M.goto_conflict("prev") end, "Previous conflict")
    map(bufnr, "g?", M.help, "Conflict editor help")
    for _, key in ipairs(KEYS) do
        map(bufnr, key[1], function() M.resolve(key[2]) end, key[3])
    end
end

local function clear_conflict_keys(bufnr)
    if not vim.b[bufnr].jj_conflict_keys then return end
    vim.b[bufnr].jj_conflict_keys = false

    for _, lhs in ipairs({"]x", "[x", "g?"}) do
        pcall(vim.keymap.del, "n", lhs, {buffer = bufnr})
    end
    for _, key in ipairs(KEYS) do
        pcall(vim.keymap.del, "n", key[1], {buffer = bufnr})
    end
end

--- Re-parse the buffer and add or remove the conflict UI to match.
function M.refresh(bufnr)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    if not vim.api.nvim_buf_is_valid(bufnr) then return 0 end

    local conflicts = M.parse(bufnr)
    render(bufnr, conflicts)
    if #conflicts > 0 then
        set_conflict_keys(bufnr)
    else
        clear_conflict_keys(bufnr)
    end
    return #conflicts
end

--- Track `bufnr` for as long as it lives, attaching the conflict UI whenever
--- markers are present. Idempotent.
function M.watch(bufnr)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    if vim.b[bufnr].jj_conflict_watched then return M.refresh(bufnr) end
    vim.b[bufnr].jj_conflict_watched = true

    vim.api.nvim_create_autocmd({"TextChanged", "InsertLeave"}, {
        group = AUGROUP,
        buffer = bufnr,
        callback = function() M.refresh(bufnr) end
    })

    return M.refresh(bufnr)
end

define_highlights()
vim.api.nvim_create_autocmd("ColorScheme",
                            {group = AUGROUP, callback = define_highlights})

return M
