vim.keymap.set("n", "<Esc>", "<cmd>nohlsearch<CR>", {desc = "Clear search highlight"})
vim.keymap.set("i", "<C-c>", "<Esc>", {desc = "Escape"})

vim.keymap.set("n", "<leader>q", vim.diagnostic.setloclist,
               {desc = "Open diagnostics [Q]uickfix list"})

vim.keymap.set("n", "<left>", "<cmd>echo 'Use h to move!!'<CR>")
vim.keymap.set("n", "<right>", "<cmd>echo 'Use l to move!!'<CR>")
vim.keymap.set("n", "<up>", "<cmd>echo 'Use k to move!!'<CR>")
vim.keymap.set("n", "<down>", "<cmd>echo 'Use j to move!!'<CR>")

-- Window navigation is <C-h/j/k/l>, defined in plugins/zellij-nav.lua so it
-- crosses between neovim splits and zellij panes with the same keys.

-- i goes back, o goes forward -- same direction sense as <leader>gi/<leader>go.
-- NOTE: terminals send <C-i> and <Tab> as the same byte, so this moves <Tab> too.
vim.keymap.set("n", "<C-i>", "<C-o>", {desc = "Jump back"})
vim.keymap.set("n", "<C-o>", "<C-i>", {desc = "Jump forward"})

vim.keymap.set("n", "<C-p>", "<cmd>Telescope find_files<CR>",
               {desc = "Find files"})
-- Jumplist/marks pickers live with the rest of <leader>s in plugins/telescope.lua

vim.keymap.set("v", "J", ":m >+1<CR>gv=gv", {desc = "Move selection down"})
vim.keymap.set("v", "K", ":m <-2<CR>gv=gv", {desc = "Move selection up"})
vim.keymap.set("n", "J", "mzJ`z", {desc = "Join line, keep cursor"})
vim.keymap.set("n", "<C-d>", "<C-d>zz", {desc = "Half page down, centered"})
vim.keymap.set("n", "<C-u>", "<C-u>zz", {desc = "Half page up, centered"})
vim.keymap.set("n", "n", "nzzzv", {desc = "Next match, centered"})
vim.keymap.set("n", "N", "Nzzzv", {desc = "Prev match, centered"})
vim.keymap.set("x", "<leader>`", "\"_dP",
               {desc = "Paste over selection, keep register"})
vim.keymap.set("n", "Q", "<nop>")

-- [Y]ank menu. `clipboard` is unnamedplus, so a plain `y` already reaches the
-- system clipboard; these cover the copies a motion can't express.
local function yank_to_clipboard(text, what)
    vim.fn.setreg("+", text)
    vim.notify(text, vim.log.levels.INFO,
               {title = "Yanked " .. what, icon = ""})
end

vim.keymap.set("n", "<leader>ya", function()
    vim.cmd("%yank +")
    vim.notify(("%d lines"):format(vim.api.nvim_buf_line_count(0)),
               vim.log.levels.INFO,
               {title = "Yanked whole file", icon = ""})
end, {desc = "[A]ll - entire file"})

vim.keymap.set("n", "<leader>yl", "\"+yy", {desc = "Current [L]ine"})

vim.keymap.set("n", "<leader>yp", function()
    yank_to_clipboard(vim.fn.expand("%:p"), "path")
end, {desc = "Absolute [P]ath"})

vim.keymap.set("n", "<leader>yr", function()
    yank_to_clipboard(vim.fn.expand("%:."), "relative path")
end, {desc = "[R]elative path"})

vim.keymap.set("n", "<leader>yn", function()
    yank_to_clipboard(vim.fn.expand("%:t"), "file name")
end, {desc = "File [N]ame"})

vim.keymap.set("n", "<leader>yd", function()
    yank_to_clipboard(vim.fn.expand("%:p:h"), "directory")
end, {desc = "[D]irectory"})
