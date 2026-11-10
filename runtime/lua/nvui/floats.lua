-- Tells nvui about floating windows before Neovim draws them, so nvui can decide which cast a shadow: it knows the
-- settings (`shadow.exclude_filetypes`), and Neovim knows each window's filetype, `w:nvui_shadow`, `winblend`, and the
-- floating window it's attached to, if any. Sends a window again only when these change.
local rpc = require('nvui.rpc')

local M = {}

-- What was last sent for each window, by handle
local sent = {}

-- What's sent about each window, besides its handle; any may be nil
local FACTS = { 'filetype', 'shadow', 'winblend', 'parent' }

--- Whether `win` is a floating window
local function is_float(win)
  return win ~= nil and vim.api.nvim_win_is_valid(win) and vim.api.nvim_win_get_config(win).relative ~= ''
end

local function forward()
  local windows = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local config = vim.api.nvim_win_get_config(win)
    if config.relative ~= '' then
      local window = {
        win = win,
        filetype = vim.bo[vim.api.nvim_win_get_buf(win)].filetype,
        shadow = vim.w[win].nvui_shadow,
        winblend = vim.wo[win].winblend,
        parent = config.relative == 'win' and is_float(config.win) and config.win or nil,
      }
      local last = sent[win]
      local changed = not last
      for _, key in ipairs(FACTS) do
        changed = changed or last[key] ~= window[key]
      end
      if changed then
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
