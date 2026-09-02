-- lazy.nvim
return {
    "folke/snacks.nvim",
    -- Not lazy, and early: enabling the notifier replaces vim.notify, so it has
    -- to be in place before anything else notifies.
    lazy = false,
    priority = 1000,
    ---@type snacks.Config
    opts = {
        image = {
            -- your image configuration comes here
            -- or leave it empty to use the default settings
            -- refer to the configuration section below
        },
        -- Floating, self-dismissing notifications instead of cmdline messages.
        -- style: "fancy" (bordered, title bar) | "compact" | "minimal"
        notifier = {
            enabled = true,
            style = "fancy",
            timeout = 2500,
            top_down = true,
            margin = { top = 0, right = 1, bottom = 0 },
            width = { min = 20, max = 0.4 },
        }
    }
}
