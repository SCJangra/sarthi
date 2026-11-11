-- Tells sarthi about floating windows, and external ones, before Neovim draws them, so sarthi can decide how each
-- looks, e.g. whether it casts a shadow: it knows the settings, and Neovim knows each window's filetype, `w:sarthi` (its
-- own values of the settings), `winblend`, and the floating window it's attached to, if any. Sends a window again only
-- when these change.
local rpc = require('sarthi.rpc')

local M = {}

-- What was last sent for each window, by handle
local sent = {}

--- Whether `win` is a floating window
local function is_float(win)
  return win ~= nil and vim.api.nvim_win_is_valid(win) and vim.api.nvim_win_get_config(win).relative ~= ''
end

local function forward()
  local windows = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local config = vim.api.nvim_win_get_config(win)
    if config.relative ~= '' or config.external then
      local window = {
        win = win,
        filetype = vim.bo[vim.api.nvim_win_get_buf(win)].filetype,
        sarthi = vim.w[win].sarthi,
        winblend = vim.wo[win].winblend,
        parent = config.relative == 'win' and is_float(config.win) and config.win or nil,
      }
      if not vim.deep_equal(sent[win], window) then
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
  group = vim.api.nvim_create_augroup('sarthi.floats', {}),
  callback = forward,
})

return M
