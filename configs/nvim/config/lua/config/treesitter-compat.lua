-- Neovim 0.12 removed the all=false query handler compatibility option.
return function()
    if vim.fn.has('nvim-0.12') == 0 then
        return
    end

    local query = vim.treesitter.query
    local original = {}
    for _, name in ipairs({ 'add_predicate', 'add_directive' }) do
        local register = query[name]
        original[name] = register
        query[name] = function(name, handler, opts)
            if type(opts) == 'table' and opts.all == false then
                local legacy_handler = handler
                handler = function(match, ...)
                    local nodes = {}
                    for id, captures in pairs(match) do
                        nodes[id] = captures[#captures]
                    end
                    return legacy_handler(nodes, ...)
                end
            end
            return register(name, handler, opts)
        end
    end

    -- Re-register only the legacy plugin's handlers, then restore the public API.
    package.loaded['nvim-treesitter.query_predicates'] = nil
    local ok, err = pcall(require, 'nvim-treesitter.query_predicates')
    for name, register in pairs(original) do
        query[name] = register
    end
    if not ok then
        error(err)
    end
end
