return {
    'nvim-lualine/lualine.nvim',
    dependencies = { 'nvim-tree/nvim-web-devicons' },
    opts = {
        -- Each section listed here replaces the default wholesale; sections
        -- left out (lualine_a/y/z) keep lualine's defaults.
        sections = {
            -- 'branch' dropped: jj keeps HEAD detached, so it only ever
            -- rendered the short commit sha.
            lualine_b = { 'diff', 'diagnostics' },
            -- path = 1 is `%:~:.` -- relative to cwd, which neo-tree keeps in
            -- sync with its root (bind_to_cwd).
            lualine_c = { { 'filename', path = 1 } },
            -- 'fileformat' dropped: the unix/dos/mac platform icon.
            lualine_x = { 'encoding', 'filetype' },
        },
        inactive_sections = {
            lualine_c = { { 'filename', path = 1 } },
        },
        extensions = { 'neo-tree' },
    },
}
