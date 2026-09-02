return {
    "AckslD/nvim-neoclip.lua",
    dependencies = {
        { 'nvim-telescope/telescope.nvim' },
    },

    -- setup() installs the TextYankPost autocmd that records history, so the
    -- plugin has to be running *before* the first yank. Loading on <leader>p
    -- alone meant history only started at the first picker open.
    -- Not lazy: a clipboard manager that misses yanks is useless, and
    -- VeryLazy would still leave a gap before UIEnter.
    lazy = false,

    keys = {
        {
            '<leader>p',
            '<cmd>Telescope neoclip<cr>',
            desc = 'Paste history (Neoclip)',
        },
    },
    config = function()
        require('neoclip').setup()
    end,
}
