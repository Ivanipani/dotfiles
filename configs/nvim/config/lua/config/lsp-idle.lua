-- Stop every LSP server after nvim has been idle for a while (servers like
-- rust-analyzer hold GBs). They restart when you come back: re-attach
-- via :edit, which re-fires FileType.
local IDLE_MS = 15 * 60 * 1000

local timer = assert(vim.uv.new_timer())

local function stop_servers()
    for _, client in ipairs(vim.lsp.get_clients()) do
        client:stop()
    end
end

-- Any activity restarts the countdown.
local function arm()
    timer:stop()
    timer:start(IDLE_MS, 0, vim.schedule_wrap(stop_servers))
end

local function restart_servers()
    arm()
    if #vim.lsp.get_clients() > 0 then
        return
    end
    for _, buf in ipairs(vim.api.nvim_list_bufs()) do
        if vim.api.nvim_buf_is_loaded(buf) and vim.bo[buf].buftype == '' and vim.bo[buf].filetype ~= '' then
            vim.api.nvim_exec_autocmds('FileType', { buffer = buf })
        end
    end
end

local group = vim.api.nvim_create_augroup('lsp-idle', { clear = true })
vim.api.nvim_create_autocmd({ 'CursorMoved', 'CursorMovedI', 'TextChanged', 'InsertEnter', 'BufEnter' }, {
    group = group,
    callback = arm,
})
-- Coming back to the terminal after a long absence: bring servers back.
vim.api.nvim_create_autocmd('FocusGained', { group = group, callback = restart_servers })
arm()
