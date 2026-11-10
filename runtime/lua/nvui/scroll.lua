-- Tells nvui which windows scrolled, before Neovim redraws them, so nvui can decide which slide: it knows the
-- settings (`scroll.exclude_filetypes`), and Neovim knows each window's filetype and `w:nvui_smooth_scroll`.
local rpc = require('nvui.rpc')

vim.api.nvim_create_autocmd('WinScrolled', {
  group = vim.api.nvim_create_augroup('nvui.scroll', {}),
  callback = function()
    local windows = {}
    for win in pairs(vim.v.event) do
      win = tonumber(win)
      if win and vim.api.nvim_win_is_valid(win) then
        local filetype = vim.bo[vim.api.nvim_win_get_buf(win)].filetype
        table.insert(windows, { win = win, filetype = filetype, smooth_scroll = vim.w[win].nvui_smooth_scroll })
      end
    end
    if #windows > 0 then
      rpc.notify('win_scrolled', windows)
    end
  end,
})
