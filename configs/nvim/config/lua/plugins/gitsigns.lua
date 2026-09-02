return { -- Adds git related signs to the gutter, as well as utilities for managing changes
    'lewis6991/gitsigns.nvim',
    -- Was `keys = {}` with no event/cmd/ft, i.e. no load trigger at all: the
    -- gutter signs and every <leader>g mapping only appeared once something
    -- else happened to require gitsigns (conform's format_diffs, on save).
    event = { 'BufReadPre', 'BufNewFile' },
    opts = {
        signs = {
            add = { text = '+' },
            change = { text = '~' },
            delete = { text = '_' },
            topdelete = { text = '‾' },
            changedelete = { text = '~' },
        },
        on_attach = function(bufnr)
            local gitsigns = require 'gitsigns'

            local function map(mode, l, r, opts)
                opts = opts or {}
                opts.buffer = bufnr
                vim.keymap.set(mode, l, r, opts)
            end

            -- Navigation: i goes back, o goes forward.
            map('n', '<leader>go', function()
                gitsigns.nav_hunk 'next'
            end, { desc = 'Next hunk (f[o]rward)' })

            map('n', '<leader>gi', function()
                gitsigns.nav_hunk 'prev'
            end, { desc = 'Prev hunk (back, l[i]ke <C-i>)' })

            map('n', '<leader>gp', gitsigns.preview_hunk, { desc = '[P]review hunk' })
            map('n', '<leader>gb', gitsigns.blame_line, { desc = '[B]lame line' })
            -- State toggles live under <leader>t, not with the git actions.
            map('n', '<leader>tb', gitsigns.toggle_current_line_blame, { desc = 'Inline git [B]lame' })
        end
    },
}
