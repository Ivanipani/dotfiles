-- Selection editor for `jj split`, `jj diffedit` and `jj squash -i`, wired up
-- as `ui.diff-editor` in vcs/jj/config.toml. The left pane is the "before"
-- state and is read-only; the right pane is what the commit will contain, and
-- toggling a line or hunk moves it in or out of that side.
return {
    "julienvincent/hunk.nvim",
    cmd = { "DiffEditor" },
    config = function()
        require("hunk").setup({
            keys = {
                global = {
                    -- <C-s> reads as "save this selection"; the default
                    -- <leader><Cr> is an awkward chord to hit repeatedly.
                    accept = { "<C-s>" },
                    quit = { "q" },
                    focus_tree = { "<leader>e" },
                },

                tree = {
                    expand_node = { "l", "<Right>" },
                    collapse_node = { "h", "<Left>" },
                    open_file = { "<Cr>" },
                    toggle_file = { "a" },
                },

                diff = {
                    toggle_line = { "a" },
                    toggle_hunk = { "A" },
                    -- Toggles the line on both sides at once, for edits that
                    -- only make sense as a pair.
                    toggle_line_pair = { "s" },

                    prev_hunk = { "[h" },
                    next_hunk = { "]h" },

                    toggle_focus = { "<Tab>" },
                },
            },

            ui = {
                tree = { mode = "nested", width = 35 },
                layout = "vertical",
                -- `q` throws away every selection, so make it deliberate.
                confirm_before_quit = true,
            },

            hooks = {
                -- Neither pane says which side it is or what the keys do; a
                -- winbar keeps both visible for the whole session.
                on_diff_mount = function(context)
                    local left = vim.api.nvim_buf_get_name(context.buf):match("^hunk://")
                    vim.wo[context.win].winbar = left
                        and "%#DiffDelete# before %*%#Comment#  read-only%*"
                        or table.concat({
                            "%#DiffAdd# after %*%<%#Comment#",
                            "  a line · A hunk · s pair · ]h/[h nav",
                            " · <Tab> switch · <C-s> confirm · q abort · g? help%*",
                        })
                end,
            },
        })
    end,
}
