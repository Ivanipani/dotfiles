-- MOST IMPORTANT KEYBINDS FROM THIS PLUGIN:
-- ' for marks
-- " for registers
return {
    -- Main LSP Configuration
    'neovim/nvim-lspconfig',
    dependencies = {
        -- Useful status updates for LSP.
        { 'j-hui/fidget.nvim', opts = {} },

        -- Allows extra capabilities provided by blink.cmp
        'saghen/blink.cmp',
    },
    config = function()
        -- Brief aside: **What is LSP?**
        --
        -- LSP is an initialism you've probably heard, but might not understand what it is.
        --
        -- LSP stands for Language Server Protocol. It's a protocol that helps editors
        -- and language tooling communicate in a standardized fashion.
        --
        -- In general, you have a "server" which is some tool built to understand a particular
        -- language (such as `gopls`, `lua_ls`, `rust_analyzer`, etc.). These Language Servers
        -- (sometimes called LSP servers, but that's kind of like ATM Machine) are standalone
        -- processes that communicate with some "client" - in this case, Neovim!
        --
        -- LSP provides Neovim with features like:
        --  - Go to definition
        --  - Find references
        --  - Autocompletion
        --  - Symbol Search
        --  - and more!
        --
        -- Thus, Language Servers are external tools that must be installed separately from
        -- Neovim. This is where `mason` and related plugins come into play.
        --
        -- If you're wondering about lsp vs treesitter, you can check out the wonderfully
        -- and elegantly composed help section, `:help lsp-vs-treesitter`

        --  This function gets run when an LSP attaches to a particular buffer.
        --    That is to say, every time a new file is opened that is associated with
        --    an lsp (for example, opening `main.rs` is associated with `rust_analyzer`) this
        --    function will be executed to configure the current buffer
        -- rust-analyzer's experimental/externalDocs only ever returns a *web*
        -- URL (it never populates a local path). Rewrite it onto the locally
        -- generated docs so opening them costs no network request:
        -- docs.rs -> target/doc, doc.rust-lang.org -> rustup sysroot.
        local function local_doc(url, root)
            local rest = url:match('^https://docs%.rs/[^/]+/[^/]+/(.+)$')
            if rest then
                return root .. '/target/doc/' .. rest, 'cargo'
            end
            rest = url:match('^https://doc%.rust%-lang%.org/[^/]+/(.+)$')
            if rest then
                local sysroot = vim.fn.system({ 'rustc', '--print', 'sysroot' }):gsub('%s+$', '')
                return sysroot .. '/share/doc/rust/html/' .. rest, 'rustup'
            end
            return nil
        end

        local cargo_doc_running = false

        -- Open a local doc page, building it first if it is not there yet.
        -- NOTE: deliberately *not* --no-deps. That only builds this crate, so
        -- any symbol from a dependency would still 404 after the build.
        local function open_local_doc(path, root, source)
            -- keep any #anchor for the browser, drop it to stat the file
            local file, frag = path:match('^([^#]*)(.*)$')
            local function open()
                vim.ui.open(vim.uri_from_fname(file) .. frag)
            end

            if vim.fn.filereadable(file) == 1 then
                return open()
            end
            if source ~= 'cargo' then
                -- std docs ship as a rustup component; cargo cannot build them.
                vim.notify('Run `rustup component add rust-docs`',
                    vim.log.levels.WARN, { title = 'Missing std docs' })
                return
            end
            if cargo_doc_running then
                vim.notify('Already building', vim.log.levels.INFO, { title = 'cargo doc' })
                return
            end

            cargo_doc_running = true
            vim.notify('Building docs (first run can be slow)',
                vim.log.levels.INFO, { title = 'cargo doc' })
            vim.system({ 'cargo', 'doc' }, { cwd = root, text = true }, function(res)
                cargo_doc_running = false
                vim.schedule(function()
                    if res.code ~= 0 then
                        vim.notify(vim.trim(res.stderr or ''):sub(1, 400),
                            vim.log.levels.ERROR, { title = 'cargo doc failed' })
                    elseif vim.fn.filereadable(file) == 0 then
                        vim.notify('Built, but no page for this symbol',
                            vim.log.levels.WARN, { title = 'cargo doc' })
                    else
                        open()
                    end
                end)
            end)
        end

        vim.api.nvim_create_autocmd('LspAttach', {
            group = vim.api.nvim_create_augroup('kickstart-lsp-attach', { clear = true }),
            callback = function(event)
                -- NOTE: Remember that Lua is a real programming language, and as such it is possible
                -- to define small helper and utility functions so you don't have to repeat yourself.
                --
                -- In this case, we create a function that lets us more easily define mappings specific
                -- for LSP related items. It sets the mode, buffer and description for us each time.
                local map = function(keys, func, desc, mode)
                    mode = mode or 'n'
                    vim.keymap.set(mode, keys, func, { buffer = event.buf, desc = 'LSP: ' .. desc })
                end

                -- Rename the variable under your cursor.
                --  Most Language Servers support renaming across files, etc.
                map('<leader>lr', vim.lsp.buf.rename, '[R]ename')

                -- Execute a code action, usually your cursor needs to be on top of an error
                -- or a suggestion from your LSP for this to activate.
                map('<leader>la', vim.lsp.buf.code_action, 'Goto Code [A]ction', { 'n', 'x' })

                -- Find references for the word under your cursor.
                map('<leader>lR', require('telescope.builtin').lsp_references, 'Goto [R]eferences')

                -- Jump to the implementation of the word under your cursor.
                --  Useful when your language has ways of declaring types without an actual implementation.
                map('<leader>li', require('telescope.builtin').lsp_implementations, 'Goto [I]mplementation')

                -- Jump to the definition of the word under your cursor.
                --  This is where a variable was first declared, or where a function is defined, etc.
                --  To jump back, press <C-t>.
                map('<leader>ld', require('telescope.builtin').lsp_definitions, 'Goto [D]efinition')

                -- WARN: This is not Goto Definition, this is Goto Declaration.
                --  For example, in C this would take you to the header.
                map('<leader>lD', vim.lsp.buf.declaration, 'Goto [D]eclaration')

                -- Fuzzy find all the symbols in your current document.
                --  Symbols are things like variables, functions, types, etc.
                map('<leader>lO', require('telescope.builtin').lsp_document_symbols, 'Open Document Symbols')

                -- Fuzzy find all the symbols in your current workspace.
                --  Similar to document symbols, except searches over your entire project.
                map('<leader>lW', require('telescope.builtin').lsp_dynamic_workspace_symbols, 'Open Workspace Symbols')

                -- Jump to the type of the word under your cursor.
                --  Useful when you're not sure what type a variable is and you want to see
                --  the definition of its *type*, not where it was *defined*.
                map('<leader>lt', require('telescope.builtin').lsp_type_definitions, 'Goto [T]ype Definition')

                -- Hover, sized and bordered. vim.lsp.buf.hover sets a focus_id,
                -- so pressing K a second time jumps *into* the window -- that is
                -- how you scroll a long docstring (then q or <C-w>c to close).
                local float_opts = {
                    border = 'rounded',
                    max_width = 100,
                    max_height = 25,
                    wrap = true
                }

                map('K', function()
                    vim.lsp.buf.hover(vim.tbl_extend('force', float_opts, {}))
                end, 'Hover docs (press K again to scroll)')

                -- rust-analyzer advertises experimental.hoverRange: it reports
                -- the type of a whole selected expression. The Range goes in the
                -- `position` field -- a separate `range` field is rejected with
                -- "missing field `position`".
                map('K', function()
                    local c = vim.lsp.get_clients({ bufnr = event.buf, method = 'textDocument/hover' })[1]
                    if not c then
                        return
                    end
                    vim.api.nvim_feedkeys(
                        vim.api.nvim_replace_termcodes('<Esc>', true, false, true), 'nx', false)
                    local rp = vim.lsp.util.make_given_range_params(
                        vim.api.nvim_buf_get_mark(event.buf, '<'),
                        vim.api.nvim_buf_get_mark(event.buf, '>'),
                        event.buf, c.offset_encoding)
                    c:request('textDocument/hover',
                        { textDocument = rp.textDocument, position = rp.range },
                        function(_, result)
                            if not (result and result.contents) then
                                vim.notify('No type for selection', vim.log.levels.INFO)
                                return
                            end
                            local md = vim.lsp.util.convert_input_to_markdown_lines(result.contents)
                            if vim.tbl_isempty(md) then
                                return
                            end
                            vim.lsp.util.open_floating_preview(md, 'markdown',
                                vim.tbl_extend('force', float_opts, { focus_id = 'hover-range' }))
                        end, event.buf)
                end, 'Hover type of selection', 'x')

                -- Peek the definition in a float instead of jumping to it.
                map('<leader>lp', function()
                    local c = vim.lsp.get_clients({ bufnr = event.buf, method = 'textDocument/definition' })[1]
                    if not c then
                        return
                    end
                    local params = vim.lsp.util.make_position_params(0, c.offset_encoding)
                    c:request('textDocument/definition', params, function(_, result)
                        if not result or vim.tbl_isempty(result) then
                            vim.notify('No definition found', vim.log.levels.INFO)
                            return
                        end
                        local target = vim.islist(result) and result[1] or result
                        vim.lsp.util.preview_location(target,
                            vim.tbl_extend('force', float_opts, { focus_id = 'peek-definition' }))
                    end, event.buf)
                end, '[P]eek definition (no jump)')

                -- Who calls this function? Results land in the quickfix list.
                map('<leader>lc', vim.lsp.buf.incoming_calls, 'In[c]oming calls')

                -- Source actions, standard LSP kinds: ruff answers both for
                -- Python (isort + every safe autofix), vtsls for TS.
                map('<leader>lo', function()
                    vim.lsp.buf.code_action({
                        context = { only = { 'source.organizeImports' } },
                        apply = true
                    })
                end, '[O]rganize imports')

                map('<leader>lF', function()
                    vim.lsp.buf.code_action({
                        context = { only = { 'source.fixAll' } },
                        apply = true
                    })
                end, '[F]ix all auto-fixable lints')

                -- NOTE: `client` is only bound further down this callback, so
                -- look it up here rather than depending on statement order.
                local attached = vim.lsp.get_client_by_id(event.data.client_id)
                if attached and attached.name == 'rust_analyzer' then
                    map('<leader>lk', function()
                        local params = vim.lsp.util.make_position_params(0, attached.offset_encoding)
                        attached:request('experimental/externalDocs', params, function(err, url)
                            if err or type(url) ~= 'string' then
                                vim.notify('No documentation for this symbol', vim.log.levels.WARN)
                                return
                            end
                            local root = attached.root_dir or vim.fn.getcwd()
                            local path, source = local_doc(url, root)
                            if not path then
                                vim.notify('Unrecognised docs URL: ' .. url, vim.log.levels.WARN)
                                return
                            end
                            open_local_doc(path, root, source)
                        end, event.buf)
                    end, 'Open local doc[k]s (cargo doc)')
                end

                -- ruff's hover only restates noqa codes, and whichever client
                -- answers first wins the float -- leave hover to basedpyright.
                if attached and attached.name == 'ruff' then
                    attached.server_capabilities.hoverProvider = false
                end

                -- This function resolves a difference between neovim nightly (version 0.11) and stable (version 0.10)
                ---@param client vim.lsp.Client
                ---@param method vim.lsp.protocol.Method
                ---@param bufnr? integer some lsp support methods only in specific files
                ---@return boolean
                local function client_supports_method(client, method, bufnr)
                    if vim.fn.has 'nvim-0.11' == 1 then
                        return client:supports_method(method, bufnr)
                    else
                        return client.supports_method(method, {
                            bufnr = bufnr
                        })
                    end
                end

                -- The following two autocommands are used to highlight references of the
                -- word under your cursor when your cursor rests there for a little while.
                --    See `:help CursorHold` for information about when this is executed
                --
                -- When you move your cursor, the highlights will be cleared (the second autocommand).
                local client = vim.lsp.get_client_by_id(event.data.client_id)
                if client and
                    client_supports_method(client, vim.lsp.protocol.Methods.textDocument_documentHighlight, event.buf) then
                    local highlight_augroup = vim.api.nvim_create_augroup('kickstart-lsp-highlight', {
                        clear = false
                    })
                    vim.api.nvim_create_autocmd({ 'CursorHold', 'CursorHoldI' }, {
                        buffer = event.buf,
                        group = highlight_augroup,
                        callback = vim.lsp.buf.document_highlight
                    })

                    vim.api.nvim_create_autocmd({ 'CursorMoved', 'CursorMovedI' }, {
                        buffer = event.buf,
                        group = highlight_augroup,
                        callback = vim.lsp.buf.clear_references
                    })

                    vim.api.nvim_create_autocmd('LspDetach', {
                        group = vim.api.nvim_create_augroup('kickstart-lsp-detach', {
                            clear = true
                        }),
                        callback = function(event2)
                            vim.lsp.buf.clear_references()
                            vim.api.nvim_clear_autocmds {
                                group = 'kickstart-lsp-highlight',
                                buffer = event2.buf
                            }
                        end
                    })
                end

                -- The following code creates a keymap to toggle inlay hints in your
                -- code, if the language server you are using supports them
                if client and client_supports_method(client, vim.lsp.protocol.Methods.textDocument_inlayHint, event.buf) then
                    -- On by default: for Rust the inferred types are most of
                    -- the value. <leader>th turns them off when they crowd.
                    vim.lsp.inlay_hint.enable(true, { bufnr = event.buf })
                    map('<leader>th', function()
                        vim.lsp.inlay_hint.enable(not vim.lsp.inlay_hint.is_enabled {
                            bufnr = event.buf
                        }, { bufnr = event.buf })
                    end, '[T]oggle Inlay [H]ints')
                end
            end
        })

        -- A rename is project-wide: the server sends edits for every file
        -- that mentions the symbol. nvim applies them to buffers but writes
        -- nothing, so the files you never opened sit modified and invisible --
        -- it reads as "it only renamed in this file", and :q! throws the rest
        -- away. Write back whatever the edit touched.
        local rename_method = vim.lsp.protocol.Methods.textDocument_rename
        local apply_rename = vim.lsp.handlers[rename_method]

        vim.lsp.handlers[rename_method] = function(err, result, ctx)
            apply_rename(err, result, ctx)
            if err or not result then
                return
            end

            local uris = {}
            for uri in pairs(result.changes or {}) do
                uris[uri] = true
            end
            -- documentChanges also carries create/rename/delete entries, which
            -- have no textDocument and nothing to save.
            for _, change in ipairs(result.documentChanges or {}) do
                if change.textDocument then
                    uris[change.textDocument.uri] = true
                end
            end

            local written = 0
            for uri in pairs(uris) do
                local buf = vim.uri_to_bufnr(uri)
                if vim.api.nvim_buf_is_loaded(buf) and vim.bo[buf].modified then
                    -- noautocmd: a file the rename opened behind your back
                    -- should not also get format-on-save run over it.
                    local ok = pcall(vim.api.nvim_buf_call, buf, function()
                        vim.cmd('silent noautocmd write')
                    end)
                    written = written + (ok and 1 or 0)
                end
            end

            if written > 0 then
                vim.notify(('Renamed in %d file%s'):format(written, written == 1 and '' or 's'),
                    vim.log.levels.INFO, { title = 'LSP rename' })
            end
        end

        -- rust-analyzer messages are long and frequently multi-line, so as
        -- virtual text they run off the right edge (`wrap` is off). Collapse
        -- them to a short inline marker and let virtual_lines render the full
        -- message, wrapped, under the cursor.
        local virt_text_max_width = 60

        -- Budget the marker against what is actually left on the line, so it
        -- can never spill past the right edge. Returning nil drops the marker
        -- entirely when the line is already too long; the sign column, the
        -- underline and virtual_lines still flag it.
        local function compact_message(diagnostic)
            local msg = diagnostic.message:gsub('%s+', ' '):gsub('^%s+', '')

            local avail = virt_text_max_width
            local win = vim.fn.bufwinid(diagnostic.bufnr)
            if win ~= -1 then
                local info = vim.fn.getwininfo(win)[1]
                local line = vim.api.nvim_buf_get_lines(diagnostic.bufnr, diagnostic.lnum,
                    diagnostic.lnum + 1, false)[1] or ''
                -- textoff drops the number/sign gutter; 6 covers spacing + prefix.
                avail = math.min(avail,
                    info.width - info.textoff - vim.fn.strdisplaywidth(line) - 6)
            end

            if avail < 12 then
                return nil
            end
            if vim.fn.strdisplaywidth(msg) > avail then
                msg = vim.fn.strcharpart(msg, 0, avail - 1) .. '…'
            end
            return msg
        end

        local virtual_text_opts = {
            source = 'if_many',
            spacing = 2,
            format = compact_message
        }

        -- Diagnostic Config
        -- See :help vim.diagnostic.Opts
        vim.diagnostic.config {
            severity_sort = true,
            float = {
                border = 'rounded',
                source = 'if_many'
            },
            underline = {
                severity = vim.diagnostic.severity.ERROR
            },
            signs = vim.g.have_nerd_font and {
                text = {
                    [vim.diagnostic.severity.ERROR] = '󰅚 ',
                    [vim.diagnostic.severity.WARN] = '󰀪 ',
                    [vim.diagnostic.severity.INFO] = '󰋽 ',
                    [vim.diagnostic.severity.HINT] = '󰌶 '
                }
            } or {},
            virtual_text = virtual_text_opts,
            virtual_lines = {
                current_line = true
            }
        }

        -- virtual_text is rendered once, when diagnostics are set, so the width
        -- budget above goes stale after a resize or a new split. An empty
        -- config call re-shows every namespace, recomputing it.
        vim.api.nvim_create_autocmd({ 'VimResized', 'WinResized' }, {
            group = vim.api.nvim_create_augroup('diagnostic-refit', { clear = true }),
            callback = function()
                vim.diagnostic.config {}
            end
        })

        -- Toggle between the compact markers and every diagnostic expanded in
        -- full, for when a whole file's worth of errors needs reading at once.
        vim.keymap.set('n', '<leader>td', function()
            local expanded = vim.diagnostic.config().virtual_text == false
            vim.diagnostic.config {
                virtual_text = expanded and virtual_text_opts or false,
                virtual_lines = expanded and { current_line = true } or true
            }
        end, { desc = '[T]oggle [D]iagnostic detail' })

        local capabilities = require('blink.cmp').get_lsp_capabilities()
        vim.lsp.config["lua_ls"] = {
            capabilities = capabilities,
            settings = {
                Lua = {
                    completion = {
                        callSnippet = 'Replace'
                    }
                    -- You can toggle below to ignore Lua_LS's noisy `missing-fields` warnings
                    -- diagnostics = { disable = { 'missing-fields' } },
                }
            }
        }
        vim.lsp.enable("lua_ls")

        vim.lsp.config("rust_analyzer", {
            capabilities = capabilities,
            settings = {
                ['rust-analyzer'] = {
                    cargo = {
                        allFeatures = true
                    },
                    -- Defaults already enable type/parameter/chaining hints;
                    -- these are the ones that ship off.
                    inlayHints = {
                        closureReturnTypeHints = { enable = 'with_block' },
                        lifetimeElisionHints = { enable = 'skip_trivial', useParameterNames = true },
                        discriminantHints = { enable = 'fieldless' },
                        rangeExclusiveHints = { enable = true },
                        expressionAdjustmentHints = { enable = 'reborrow' },
                        closingBraceHints = { enable = true, minLines = 20 },
                        maxLength = 40 -- default 25; long generics get elided
                    },
                    -- Make hover carry the whole picture: struct fields, enum
                    -- variants, trait items and untruncated generics.
                    hover = {
                        actions = { enable = true },
                        documentation = { enable = true, keywords = { enable = true } },
                        links = { enable = true },
                        memoryLayout = { enable = true, size = 'both', alignment = 'both', niches = true },
                        show = { fields = 15, enumVariants = 15, traitAssocItems = 10 },
                        maxSubstitutionLength = 80 -- default 20
                    },
                    -- Clippy instead of plain `cargo check` on save.
                    -- --no-deps keeps the lints (and the wait) to this workspace.
                    check = {
                        command = 'clippy',
                        extraArgs = { '--no-deps' }
                    }
                }
            }
        })
        vim.lsp.enable("rust_analyzer")

        -- Python is split over two servers: basedpyright does types,
        -- completion, inlay hints and navigation; ruff does lints, autofixes
        -- and import sorting. basedpyright is the open-source pyright fork --
        -- it re-adds the pieces upstream reserves for Pylance (inlay hints,
        -- semantic tokens, call hierarchy) and infers more than ty, which is
        -- still alpha, or pylsp, which leans on jedi.

        -- Point basedpyright at the interpreter the project actually uses.
        -- Without this it resolves imports against the system python and every
        -- third-party import reads as unresolved. venv-selector (<leader>v)
        -- overrides this at runtime when a different venv is picked.
        local function python_path(root)
            local venv = vim.env.VIRTUAL_ENV
            if venv and venv ~= '' then
                return venv .. '/bin/python'
            end
            for _, dir in ipairs({ '.venv', 'venv' }) do
                local py = root .. '/' .. dir .. '/bin/python'
                if vim.uv.fs_stat(py) then
                    return py
                end
            end
            return vim.fn.exepath('python3')
        end

        vim.lsp.config["basedpyright"] = {
            capabilities = capabilities,
            -- Mutate the settings table in place: the client already holds a
            -- reference to it by now, so reassigning config.settings would be
            -- dropped on the floor. Only fill in a path nobody set -- when
            -- venv-selector restarts the client it passes the picked venv here,
            -- and that choice has to win over the guess.
            before_init = function(_, config)
                local py = config.settings.python or {}
                if not py.pythonPath then
                    py.pythonPath = python_path(config.root_dir or vim.fn.getcwd())
                    config.settings.python = py
                end
            end,
            settings = {
                basedpyright = {
                    analysis = {
                        -- Diagnose the whole project, not just open buffers --
                        -- the equivalent of `cargo check` reporting the workspace.
                        diagnosticMode = 'workspace',
                        typeCheckingMode = 'standard',
                        autoSearchPaths = true,
                        -- Completions offer symbols that are not imported yet and
                        -- add the import on accept.
                        autoImportCompletions = true,
                        -- Off by default upstream; these are the Rust-style
                        -- inferred-type annotations. <leader>th toggles them.
                        inlayHints = {
                            variableTypes = true,
                            functionReturnTypes = true,
                            callArgumentNames = true,
                            genericTypes = true
                        },
                        -- Ruff already flags these (F401/F841/F821). One
                        -- squiggle per problem.
                        diagnosticSeverityOverrides = {
                            reportUnusedImport = 'none',
                            reportUnusedVariable = 'none',
                            reportUndefinedVariable = 'none'
                        }
                    },
                    -- ruff owns import sorting, via its isort rules.
                    disableOrganizeImports = true
                }
            }
        }
        vim.lsp.enable("basedpyright")

        -- Ruff as a second client: lint diagnostics and quick fixes as you
        -- type, in the same spirit as clippy-on-save for Rust. Formatting
        -- still goes through conform (plugins/conform.lua).
        vim.lsp.config["ruff"] = {
            capabilities = capabilities,
            init_options = {
                settings = {
                    -- A project's ruff.toml/pyproject.toml wins; the rules
                    -- below only apply where the project sets none.
                    configurationPreference = 'filesystemFirst',
                    lint = {
                        -- Beyond ruff's default E4/E7/E9/F: imports, pyupgrade,
                        -- bugbear, simplify, ruff's own lints.
                        extendSelect = { 'I', 'UP', 'B', 'SIM', 'RUF' }
                    }
                }
            }
        }
        vim.lsp.enable("ruff")

        vim.lsp.config["vtsls"] = {
            capabilities = capabilities,
            settings = { }
        }
        vim.lsp.enable("vtsls")

        -- helm-ls.nvim sets the helm filetype; helm_ls proxies to yamlls.
        vim.lsp.config["helm_ls"] = {
            capabilities = capabilities,
            settings = {
                ['helm-ls'] = { yamlls = { path = 'yaml-language-server' } },
            },
        }
        vim.lsp.enable("helm_ls")
    end
}
