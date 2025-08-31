-- lua/plugins/monorepo.lua
return {
    "imNel/monorepo.nvim",
    dependencies = { "nvim-lua/plenary.nvim", "nvim-telescope/telescope.nvim" },
    config = function()
        ----------------------------------------------------------------
        -- Setup: persist under repo-local .nvim so teams share the list
        ----------------------------------------------------------------
        local M = require("monorepo")
        local function get_git_root()
            local dotgit = vim.fs.find(".git", {
                upward = true,
                type = "directory"
            })[1]
            if not dotgit then
                return nil
            end
            return vim.fs.dirname(dotgit)
        end

        local git_root = get_git_root()
        local data_path = git_root and (git_root .. "/.nvim") or vim.fn.stdpath("data")
        if data_path and vim.fn.isdirectory(data_path) == 0 then
            vim.fn.mkdir(data_path, "p")
        end

        M.setup({
            autoload_telescope = true,
            silent = true,
            data_path = data_path
        })

        ----------------------------------------------------------------
        -- Helpers
        ----------------------------------------------------------------
        local function rel_from_root(root, abs_dir)
            local rel = abs_dir:gsub("^" .. vim.pesc(root), "")
            if rel == "" then
                return "/"
            end
            if rel:sub(1, 1) ~= "/" then
                rel = "/" .. rel
            end
            return rel
        end

        local function file_exists(p)
            return vim.fn.filereadable(p) == 1
        end
        local function dir_of(p)
            return vim.fn.fnamemodify(p, ":h")
        end

        local function is_python_project(dir)
            return file_exists(dir .. "/pyproject.toml") or file_exists(dir .. "/setup.cfg") or
                file_exists(dir .. "/setup.py") or file_exists(dir .. "/requirements.txt")
        end

        local function is_rust_project(dir)
            return file_exists(dir .. "/Cargo.toml")
        end

        -- Parse .gitmodules and return a list of ABSOLUTE submodule directories
        local function git_submodule_dirs(root)
            local mods = {}
            local gm = root .. "/.gitmodules"
            if vim.fn.filereadable(gm) == 1 then
                local lines = vim.fn.readfile(gm)
                for _, line in ipairs(lines) do
                    -- lines look like: path = resil/libraries/foo
                    local path = line:match("^%s*path%s*=%s*(.+)%s*$")
                    if path then
                        local abs = vim.fn.fnamemodify(root .. "/" .. path, ":p"):gsub("/+$", "")
                        table.insert(mods, abs)
                    end
                end
            end
            return mods
        end

        -- Scan your declared monorepo paths for Python/Rust
        local function glob_projects_under(root)
            local results = {}
            local patterns = { "resil/libraries/*/pyproject.toml", "resil/libraries/*/setup.cfg",
                "resil/libraries/*/Cargo.toml", "resil/services/*/pyproject.toml",
                "resil/services/*/setup.cfg", "resil/services/*/Cargo.toml" }
            for _, pat in ipairs(patterns) do
                for _, f in ipairs(vim.fn.globpath(root, pat, false, true)) do
                    results[dir_of(f)] = true
                end
            end
            return vim.tbl_keys(results)
        end

        local function discover_projects(root)
            local seen = {}
            local function add_dir(abs_dir)
                if not abs_dir or abs_dir == "" then
                    return
                end
                abs_dir = abs_dir:gsub("/+$", "")
                if seen[abs_dir] then
                    return
                end
                -- Only add if it looks like a Python or Rust project
                if is_python_project(abs_dir) or is_rust_project(abs_dir) then
                    seen[abs_dir] = true
                    M.add_project(rel_from_root(root, abs_dir))
                end
            end

            -- 1) Submodules from .gitmodules
            for _, d in ipairs(git_submodule_dirs(root)) do
                add_dir(d)
            end

            -- 2) Explicit monorepo paths you gave
            for _, d in ipairs(glob_projects_under(root)) do
                add_dir(d)
            end
        end

        ----------------------------------------------------------------
        -- Commands + Autocmd
        ----------------------------------------------------------------
        vim.api.nvim_create_user_command("MonorepoRescan", function()
            local root = get_git_root()
            if not root then
                vim.notify("Monorepo: no git root found", vim.log.levels.WARN)
                return
            end
            M.change_monorepo(root)
            discover_projects(root)
            require("telescope").extensions.monorepo.monorepo()
        end, {
            desc = "Rebuild monorepo project list from .gitmodules and resil/* paths"
        })

        -- Point to repo on open; only discover if list is empty/minimal
        vim.api.nvim_create_autocmd("VimEnter", {
            callback = function()
                local root = get_git_root()
                if not root then
                    return
                end
                M.change_monorepo(root)
                local vars = M.monorepoVars or {}
                local list = vars[root]
                if not list or (#list == 1 and list[1] == "/") then
                    discover_projects(root)
                end
            end
        })

        ----------------------------------------------------------------
        -- Keymaps
        ----------------------------------------------------------------
        local map = vim.keymap.set
        map("n", "<leader>ms", function()
            require("telescope").extensions.monorepo.monorepo()
        end, {
            desc = "[S]witch Project"
        })
        map("n", "<leader>mr", "<cmd>MonorepoRescan<CR>", {
            desc = "[R]escan Projects"
        })
    end
}
