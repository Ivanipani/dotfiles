-- NOTE: Plugins can specify dependencies.
--
-- The dependencies are proper plugin specifications as well - anything
-- you do for a plugin at the top level, you can do for a dependency.
--
-- Use the `dependencies` key to specify the dependencies of a particular plugin

return { -- Fuzzy Finder (files, lsp, etc)
    'nvim-telescope/telescope.nvim',
    event = 'VimEnter',
    dependencies = {
        'nvim-lua/plenary.nvim',
        { -- If encountering errors, see telescope-fzf-native README for installation instructions
            'nvim-telescope/telescope-fzf-native.nvim',

            -- `build` is used to run some command when the plugin is installed/updated.
            -- This is only run then, not every time Neovim starts up.
            build = 'make',

            -- `cond` is a condition used to determine whether this plugin should be
            -- installed and loaded.
            cond = function()
                return vim.fn.executable 'make' == 1
            end,
        },
        { 'nvim-telescope/telescope-ui-select.nvim' },

        -- Useful for getting pretty icons, but requires a Nerd Font.
        { 'nvim-tree/nvim-web-devicons',            enabled = vim.g.have_nerd_font },
    },
    config = function()
        -- Telescope is a fuzzy finder that comes with a lot of different things that
        -- it can fuzzy find! It's more than just a "file finder", it can search
        -- many different aspects of Neovim, your workspace, LSP, and more!
        --
        -- The easiest way to use Telescope, is to start by doing something like:
        --  :Telescope help_tags
        --
        -- After running this command, a window will open up and you're able to
        -- type in the prompt window. You'll see a list of `help_tags` options and
        -- a corresponding preview of the help.
        --
        -- Two important keymaps to use while in Telescope are:
        --  - Insert mode: <c-/>
        --  - Normal mode: ?
        --
        -- This opens a window that shows you all of the keymaps for the current
        -- Telescope picker. This is really useful to discover what Telescope can
        -- do as well as how to actually do it!

        -- [[ Configure Telescope ]]
        -- See `:help telescope` and `:help telescope.setup()`
        local actions = require 'telescope.actions'

        -- Picker keybind cheatsheet (<C-x> split, <C-v> vsplit, <C-t> tab, ...).
        -- NOTE: telescope hardcodes this popup to the full window width and
        -- docks it top/bottom (actions/init.lua sets minwidth/maxwidth to
        -- vim.o.columns and col = 0), so it cannot be placed beside the picker.
        local function picker_help(prompt_bufnr)
            pcall(actions.which_key, prompt_bufnr, {
                only_show_current_mode = false, -- list insert AND normal mode
                name_width = 32,
                keybind_width = 10,
                max_height = 0.45,
                separator = '  ',
            })
        end

        require('telescope').setup {
            defaults = {
                -- Prompt at the top. telescope defaults sorting_strategy to
                -- 'descending', which puts the best match at the *bottom* --
                -- correct for a bottom prompt, wrong for a top one. Flip it so
                -- the best match stays next to what you are typing.
                sorting_strategy = 'ascending',
                layout_config = {
                    horizontal = { prompt_position = 'top' },
                    vertical = { prompt_position = 'top' },
                },
                -- On demand only: <Esc> then '?'. Telescope's own <C-/>/<C-_>
                -- insert-mode bindings still work, with default formatting.
                mappings = {
                    n = { ['?'] = picker_help },
                },
            },
            pickers = { find_files = { hidden = false } },
            extensions = {
                ['ui-select'] = {
                    require('telescope.themes').get_dropdown(),
                },
            },
        }

        -- Enable Telescope extensions if they are installed
        pcall(require('telescope').load_extension, 'fzf')
        pcall(require('telescope').load_extension, 'ui-select')

        -- See `:help telescope.builtin`
        local builtin = require 'telescope.builtin'

        -- builtin.resume replays the entries telescope cached when the picker
        -- closed, so results are stale for anything edited since. Record how
        -- each picker was opened and re-run it instead, prompt restored.
        local last_picker
        local token = 0
        for name, open_picker in pairs(builtin) do
            if name ~= 'resume' and name ~= 'pickers' then
                builtin[name] = function(opts)
                    opts = opts or {}
                    -- Pin what the picker resolves from the cursor, so the
                    -- re-run searches the same thing from wherever we resume.
                    opts.bufnr = opts.bufnr or vim.api.nvim_get_current_buf()
                    opts.winnr = opts.winnr or vim.api.nvim_get_current_win()
                    if name == 'grep_string' then
                        opts.search = opts.search or vim.fn.expand '<cword>'
                    end

                    token = token + 1
                    if opts.cache_picker ~= false then
                        -- rides along on the cached picker, to tell whether it
                        -- is the one we recorded (extensions are not wrapped)
                        opts.cache_picker = vim.tbl_extend('force', opts.cache_picker or {}, { token = token })
                    end

                    -- pickers mutate the opts they are handed, so keep a copy
                    local ok, snapshot = pcall(vim.deepcopy, opts)
                    last_picker = {
                        name = name,
                        token = token,
                        opts = ok and snapshot or vim.tbl_extend('force', {}, opts),
                    }

                    return open_picker(opts)
                end
            end
        end

        local function resume_fresh()
            local cached = require('telescope.state').get_global_key 'cached_pickers'
            local previous = cached and cached[1]
            local cache = previous and previous.cache_picker or {}
            if not last_picker or cache.token ~= last_picker.token then
                -- not ours (extension picker, or nothing cached): replay it
                return builtin.resume()
            end

            local opts = vim.tbl_extend('force', last_picker.opts, { default_text = cache.cached_prompt })
            if opts.bufnr and not vim.api.nvim_buf_is_valid(opts.bufnr) then
                opts.bufnr = nil
            end
            if opts.winnr and not vim.api.nvim_win_is_valid(opts.winnr) then
                opts.winnr = nil
            end
            builtin[last_picker.name](opts)
        end

        vim.keymap.set('n', '<leader>sh', builtin.help_tags, { desc = '[S]earch [H]elp' })
        vim.keymap.set('n', '<leader>sk', builtin.keymaps, { desc = '[S]earch [K]eymaps' })
        vim.keymap.set('n', '<leader>sf', builtin.find_files, { desc = '[S]earch [F]iles' })
        vim.keymap.set('n', '<leader>ss', builtin.builtin, { desc = '[S]earch [S]elect Telescope' })
        vim.keymap.set('n', '<leader>sw', builtin.grep_string, { desc = '[S]earch current [W]ord' })
        vim.keymap.set('n', '<leader>sg', builtin.live_grep, { desc = '[S]earch by [G]rep' })
        vim.keymap.set('n', '<leader>sd', builtin.diagnostics, { desc = '[S]earch [D]iagnostics' })
        vim.keymap.set('n', '<leader>sr', resume_fresh, { desc = '[S]earch [R]esume' })
        vim.keymap.set('n', '<leader>sj', builtin.jumplist, { desc = '[S]earch [J]umplist' })
        vim.keymap.set('n', '<leader>sm', builtin.marks, { desc = '[S]earch [M]arks' })
        vim.keymap.set('n', '<leader>s.', builtin.oldfiles, { desc = '[S]earch Recent Files ("." for repeat)' })
        vim.keymap.set('n', '<leader><leader>', builtin.buffers, { desc = '[ ] Find existing buffers' })

        -- Slightly advanced example of overriding default behavior and theme
        vim.keymap.set('n', '<leader>/', function()
            -- You can pass additional configuration to Telescope to change the theme, layout, etc.
            builtin.current_buffer_fuzzy_find(require('telescope.themes').get_dropdown {
                winblend = 10,
                previewer = false,
            })
        end, { desc = '[/] Fuzzily search in current buffer' })

        -- It's also possible to pass additional configuration options.
        --  See `:help telescope.builtin.live_grep()` for information about particular keys
        vim.keymap.set('n', '<leader>s/', function()
            builtin.live_grep {
                grep_open_files = true,
                prompt_title = 'Live Grep in Open Files',
            }
        end, { desc = '[S]earch [/] in Open Files' })

        -- Shortcut for searching your Neovim configuration files
        vim.keymap.set('n', '<leader>sn', function()
            builtin.find_files { cwd = vim.fn.stdpath 'config' }
        end, { desc = '[S]earch [N]eovim files' })
    end,
}
