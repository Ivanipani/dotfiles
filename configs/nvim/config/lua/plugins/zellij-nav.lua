return {
        "swaits/zellij-nav.nvim",
        lazy = true,
        event = "VeryLazy",
        -- lazy.nvim reads opts as named keys on this table; a positional
        -- third element is silently dropped (which is why desc never showed).
        keys = {
                { "<c-h>", "<cmd>ZellijNavigateLeft<cr>",  silent = true, desc = "Window/pane left" },
                { "<c-j>", "<cmd>ZellijNavigateDown<cr>",  silent = true, desc = "Window/pane down" },
                { "<c-k>", "<cmd>ZellijNavigateUp<cr>",    silent = true, desc = "Window/pane up" },
                { "<c-l>", "<cmd>ZellijNavigateRight<cr>", silent = true, desc = "Window/pane right" },
        },
        opts = {},
}
