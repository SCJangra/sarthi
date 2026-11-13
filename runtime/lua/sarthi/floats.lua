-- Tells sarthi about floating windows, and external ones, before Neovim draws them, so sarthi can decide how each
-- looks, e.g. whether it casts a shadow: it knows the settings, and Neovim knows each window's filetype, `w:sarthi` (its
-- own values of the settings), `winblend`, its outline's color and background, and the floating window it's attached
-- to, if any. Sends a window again only when these change.
local rpc = require('sarthi.rpc')

local M = {}

-- What was last sent for each window, by handle
local sent = {}

-- The outline's highlight group in the settings, for managed windows that don't give their own
local highlight = 'FloatBorder'

--- Whether `win` is a floating window
local function is_float(win)
  return win ~= nil and vim.api.nvim_win_is_valid(win) and vim.api.nvim_win_get_config(win).relative ~= ''
end

--- The text color of `win`'s outline's highlight group, if it has one: sarthi only learns the colors of Neovim's UI
--- groups from Neovim. Its own group, or the settings' if it's managed, or else the default.
local function outline_color(win)
  local sarthi = vim.w[win].sarthi
  local own = type(sarthi) == 'table' and type(sarthi.outline) == 'table' and sarthi.outline.highlight
  local managed = type(sarthi) == 'table' and sarthi.managed == true
  local name = type(own) == 'string' and own or managed and highlight or 'FloatBorder'
  return vim.api.nvim_get_hl(0, { name = name, link = false }).fg
end

--- `win`'s background, as Neovim picks it: a floating window's `NormalFloat`, or an external one's `Normal`, or what
--- its 'winhighlight' maps them to, from its highlight namespace if it has one; `nil` for the default background
local function background(win, external)
  local map = {}
  for _, item in ipairs(vim.split(vim.wo[win].winhighlight, ',', { trimempty = true })) do
    local from, to = item:match('^([^:]+):(.+)$')
    if from then
      map[from] = to
    end
  end
  local ns = math.max(vim.api.nvim_get_hl_ns({ winid = win }), 0)
  local function color(name)
    local bg = vim.api.nvim_get_hl(ns, { name = name, link = false }).bg
    if bg == nil and ns ~= 0 then
      bg = vim.api.nvim_get_hl(0, { name = name, link = false }).bg
    end
    return bg
  end
  if not external and map.NormalFloat then
    return color(map.NormalFloat)
  elseif map.Normal then
    return color(map.Normal)
  end
  return color(external and 'Normal' or 'NormalFloat')
end

--- What's sent about `win`, if it's a floating or external window
local function facts(win)
  local config = vim.api.nvim_win_get_config(win)
  if config.relative == '' and not config.external then
    return nil
  end
  return {
    win = win,
    filetype = vim.bo[vim.api.nvim_win_get_buf(win)].filetype,
    sarthi = vim.w[win].sarthi,
    winblend = vim.wo[win].winblend,
    outline_color = outline_color(win),
    background = background(win, config.external),
    parent = config.relative == 'win' and is_float(config.win) and config.win or nil,
  }
end

local function forward()
  local windows = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local window = facts(win)
    if window then
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

--- Sends `win` now, e.g. as `require('sarthi').api` sets it up, so sarthi has its style before Neovim draws it: Neovim's
--- redraw comes after this on the channel.
function M.send(win)
  local window = facts(win)
  if window and not vim.deep_equal(sent[win], window) then
    sent[win] = window
    rpc.notify('floats_changed', { window })
  end
end

--- Sends every floating window again, after the settings changed to `settings`
function M.resend(settings)
  highlight = vim.tbl_get(settings, 'outline', 'highlight') or 'FloatBorder'
  sent = {}
  forward()
end

-- SafeState comes after a plugin set a new window's variables and before Neovim draws it. It doesn't come while a
-- mapping runs or an operator is pending, so the events that set a window's buffer and filetype forward too.
vim.api.nvim_create_autocmd({ 'WinNew', 'BufWinEnter', 'FileType', 'ColorScheme', 'SafeState' }, {
  group = vim.api.nvim_create_augroup('sarthi.floats', {}),
  callback = forward,
})

-- 'cmdheight', which external windows stay above, like Neovim keeps floating windows; it's per tab page.
local function send_cmdheight()
  rpc.notify('cmdheight_changed', vim.o.cmdheight)
end
send_cmdheight()
vim.api.nvim_create_autocmd({ 'OptionSet', 'TabEnter' }, {
  group = vim.api.nvim_create_augroup('sarthi.cmdheight', {}),
  callback = function(args)
    if args.event == 'TabEnter' or args.match == 'cmdheight' then
      send_cmdheight()
    end
  end,
})

return M
