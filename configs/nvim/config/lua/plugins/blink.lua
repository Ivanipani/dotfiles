return { -- Autocompletion
    'saghen/blink.cmp',
    event = 'VimEnter',
    version = '1.*',
    dependencies = {
        'folke/lazydev.nvim',
    },
    --- @module 'blink.cmp'
    --- @type blink.cmp.Config
    opts = {
        keymap = {
            -- 'default' (recommended) for mappings similar to built-in completions
            --   <c-y> to accept ([y]es) the completion.
            --    This will auto-import if your LSP supports it.
            --    This will expand snippets if the LSP sent a snippet.
            -- 'super-tab' for tab to accept
            -- 'enter' for enter to accept
            -- 'none' for no mappings
            --
            -- For an understanding of why the 'default' preset is recommended,
            -- you will need to read `:help ins-completion`
            --
            -- No, but seriously. Please read `:help ins-completion`, it is really good!
            --
            -- All presets have the following mappings:
            -- <tab>/<s-tab>: move to right/left of your snippet expansion
            -- <c-space>: Open menu or open docs if already open
            -- <c-n>/<c-p> or <up>/<down>: Select next/previous item
            -- <c-e>: Hide menu
            -- <c-k>: Toggle signature help
            --
            -- See :h blink-cmp-config-keymap for defining your own keymap
            preset = 'default',

            -- Docs for the selected item, in one keypress. <c-space> chains
            -- 'show' first, so it takes two presses from an open menu; this is
            -- a straight toggle, falling back to the normal <c-d> dedent when
            -- the menu is closed.
            ['<C-d>'] = { 'show_documentation', 'hide_documentation', 'fallback' },

            -- Scroll whichever float is up: docs when the menu is open,
            -- otherwise the signature window.
            ['<C-f>'] = { 'scroll_documentation_down', 'scroll_signature_down', 'fallback' },
            ['<C-b>'] = { 'scroll_documentation_up', 'scroll_signature_up', 'fallback' },

            -- For more advanced Luasnip keymaps (e.g. selecting choice nodes, expansion) see:
            --    https://github.com/L3MON4D3/LuaSnip?tab=readme-ov-file#keymaps
        },

        appearance = {
            -- 'mono' (default) for 'Nerd Font Mono' or 'normal' for 'Nerd Font'
            -- Adjusts spacing to ensure icons are aligned
            nerd_font_variant = 'mono',
        },

        completion = {
            -- Docs stay opt-in (<c-d> toggles, <c-space> also reaches them);
            -- auto_show_delay_ms only applies if auto_show is flipped on.
            documentation = {
                auto_show = false,
                auto_show_delay_ms = 500,
                treesitter_highlighting = true,
                -- Same geometry as the LSP hover float in plugins/lsp.lua, so
                -- docs read identically whether they come from K or the menu.
                window = {
                    border = 'rounded',
                    max_width = 100,
                    max_height = 25,
                    scrollbar = true,
                },
            },
        },

        sources = {
            default = { 'lsp', 'path', 'lazydev' },
            providers = {
                lazydev = { module = 'lazydev.integrations.blink', score_offset = 100 },
            },
        },


        -- Blink.cmp includes an optional, recommended rust fuzzy matcher,
        -- which automatically downloads a prebuilt binary when enabled.
        --
        -- By default, we use the Lua implementation instead, but you may enable
        -- the rust implementation via `'prefer_rust_with_warning'`
        --
        -- See :h blink-cmp-config-fuzzy for more information
        fuzzy = { implementation = 'lua' },

        -- Shows a signature help window while you type arguments for a function.
        -- <c-k> toggles it, <c-f>/<c-b> scroll it.
        signature = {
            enabled = true,
            trigger = {
                -- Re-show the params after accepting a completion that ends in
                -- `(`, so the arg list is there before you type anything.
                show_on_accept_on_trigger_character = true,
                show_on_insert_on_trigger_character = true,
            },
            window = {
                border = 'rounded',
                max_width = 100,
                max_height = 25,
                -- Params alone are rarely enough: pull in the method's doc text.
                show_documentation = true,
            },
        },
    },
}
