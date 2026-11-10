-- Tells nvui about floating windows before Neovim draws them, so nvui can decide which have a shadow: it knows the
-- settings (`shadow.exclude_filetypes`), and Neovim knows each window's filetype and `w:nvui_shadow`. Sends a window
-- again only when these change.
local rpc = require('nvui.rpc')

local M = {}

-- What was last sent for each window, by handle
local sent = {}

local function forward()
  local windows = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      local window = {
        win = win,
        filetype = vim.bo[vim.api.nvim_win_get_buf(win)].filetype,
        shadow = vim.w[win].nvui_shadow,
      }
      local last = sent[win]
      if not last or last.filetype ~= window.filetype or last.shadow ~= window.shadow then
        sent[win] = window
        table.insert(windows, window)
      end
    end
  end
  -- Forget closed windows.
  for win in pairs(sent) do
    if not vim.api.nvim_win_is_valid(win) then
      sent[win] = nil
    end
  end
  if #windows > 0 then
    rpc.notify('floats_changed', windows)
  end
end

--- Sends every floating window again, e.g. after the settings changed
function M.resend()
  sent = {}
  forward()
end

-- SafeState comes after a plugin set a new window's variables and before Neovim draws it. It doesn't come while a
-- mapping runs or an operator is pending, so the events that set a window's buffer and filetype forward too.
vim.api.nvim_create_autocmd({ 'WinNew', 'BufWinEnter', 'FileType', 'SafeState' }, {
  group = vim.api.nvim_create_augroup('nvui.floats', {}),
  callback = forward,
})

return M
