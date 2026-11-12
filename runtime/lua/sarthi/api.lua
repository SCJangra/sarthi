---@mod sarthi.api Window functions
---@brief [[
---`require("sarthi").api`: Neovim's window functions that take a window config,
---taking a `sarthi` table in it too, for what only Sarthi does with windows.
---Everything else is `vim.api`. Only Sarthi has the module, so a plugin shared
---with the TUI checks `vim.g.sarthi` first, and opens its window another way
---otherwise.
--->lua
---  require('sarthi').api.nvim_open_win(buf, false, {
---    external = true, width = 40, height = 30,
---    sarthi = {
---      place = { relative = 'editor', row = 2, col = 4 },
---      shadow = { opacity = 0.6 },
---    },
---  })
---<
---The `sarthi` table (|sarthi.WindowConfig|) takes `place` and `font` for
---external windows (`external = true`), and the window's own values of the
---settings' sections about a window: `shadow`, `outline`, `corners`, `padding`
---and `scroll`, with the same keys (|sarthi-settings|), each in place of the
---setting's. Those are kept in the window's `w:sarthi`, which a plugin can also
---set itself.
---@brief ]]
local rpc = require('sarthi.rpc')

local api = {}

---Where Sarthi places an external window, over Neovim's screen: `{ center =
---true }` centers it, as it is until placed; otherwise it's placed like a
---floating window, with its `anchor` corner at `row`, `col` of what it's
---`relative` to.
---
---A window's frame, its padding and outline (|sarthi-padding|,
---|sarthi-outline|), is in pixels around its cells, so it goes around
---where the window is placed: the frame starts at the anchor point, and
---the cells move away from it by the frame's width. Relative to a window,
---`row` and `col` count in its cells from where its frame starts, not its
---text, unlike from inside a Neovim `border`. So windows line up in whole
---cells whatever their frames: `col = 0` lines up with the window's left
---edge, and `row` = its height starts just below it. Floating windows
---placed by Neovim go the same.
---@class sarthi.WindowPlace
---@field center? boolean Center it
---@field relative? 'editor'|'win'|'cursor' What it's placed relative to
---@field win? integer For 'win': the window, or the current one
---@field anchor? 'NW'|'NE'|'SW'|'SE' Its corner at `row`, `col`
---@field row? number Rows down
---@field col? number Columns right

---The fonts an external window's text is drawn with. Either left out is
---`guifont`'s. The window is as many of its own cells as its config says.
---@class sarthi.WindowFont
---@field family? string|string[] One or more, in fallback order
---@field size? number In points, like `guifont`'s `:h`

---@class sarthi.WindowConfig
---@field place? sarthi.WindowPlace External windows only
---@field font? sarthi.WindowFont External windows only
---@field shadow? table Its own |sarthi-shadow| values
---@field outline? table Its own |sarthi-outline| values
---@field corners? table Its own |sarthi-corners| values
---@field padding? table Its own |sarthi-padding| values
---@field scroll? table Its own |sarthi-scroll| values

-- What only the API keeps of each window's `sarthi` table, by handle, as last given: what isn't in `w:sarthi`
local configs = {}

-- What only external windows take, and why
local EXTERNAL_ONLY = {
  place = 'floating windows are placed by Neovim',
  font = "floating windows share the main grid's cells",
}

-- The sections of the settings a window can have its own values of, kept in `w:sarthi`
local OPTIONS = { shadow = true, outline = true, corners = true, padding = true, scroll = true }

--- `config` without its `sarthi` table, for Neovim, and the table
local function split(config)
  if type(config) ~= 'table' then
    return config, nil
  end
  local plain = vim.tbl_extend('force', {}, config)
  plain.sarthi = nil
  return plain, config.sarthi
end

--- `options`, a window's `w:sarthi`, with the sections given in `sarthi` merged in, key by key
local function merge(options, sarthi)
  local merged = vim.deepcopy(options or {})
  for section in pairs(OPTIONS) do
    if sarthi[section] ~= nil then
      merged[section] = vim.tbl_extend('force', merged[section] or {}, sarthi[section])
    end
  end
  return merged
end

--- `w:sarthi` of a window that is `external` or not, with `options` (its `w:sarthi` until now) and what `sarthi` gives
--- merged; raises what's wrong with either at `level`
local function check(sarthi, options, external, level)
  if sarthi == nil then
    return options
  end
  if type(sarthi) ~= 'table' then
    error('sarthi: `sarthi` takes a table, not a ' .. type(sarthi), level)
  end
  for key, value in pairs(sarthi) do
    if not (OPTIONS[key] or EXTERNAL_ONLY[key]) then
      error(('sarthi: `sarthi` has no `%s`'):format(key), level)
    end
    if EXTERNAL_ONLY[key] and value ~= nil and not external then
      error(('sarthi: `sarthi.%s` needs `external = true`: %s'):format(key, EXTERNAL_ONLY[key]), level)
    end
    if type(value) ~= 'table' then
      error(('sarthi: `sarthi.%s` takes a table, not a %s'):format(key, type(value)), level)
    end
  end
  options = merge(options, sarthi)
  -- Sarthi decodes and checks the options, against the settings they go over.
  local ok, mistake = pcall(vim.rpcrequest, rpc.channel, 'check_window_options', options)
  if not ok then
    -- Without Neovim's "Vim:Invoking 'method' on channel …:" line before the message
    error('sarthi: `sarthi`: ' .. mistake:gsub('^Vim:Invoking.-\n', ''), level)
  elseif mistake ~= vim.NIL then
    error('sarthi: `sarthi`: ' .. mistake, level)
  end
  return options
end

--- `place` with what it's relative to as it is now, before a window is opened and maybe entered: the current window
--- for 'win' without one or 0, and the cursor for 'cursor'
local function resolve(place)
  if place ~= nil and place.relative == 'win' and (place.win == nil or place.win == 0) then
    return vim.tbl_extend('force', place, { win = vim.api.nvim_get_current_win() })
  end
  -- By the cursor where it is now, which sarthi only learns with Neovim's next redraw: relative to the current window, at
  -- the cursor's cell of it, so in its own cells if it's an external window with fonts of its own
  if place ~= nil and place.relative == 'cursor' then
    return vim.tbl_extend('force', place, {
      relative = 'win',
      win = vim.api.nvim_get_current_win(),
      row = vim.fn.winline() - 1 + (place.row or 0),
      col = vim.fn.wincol() - 1 + (place.col or 0),
    })
  end
  return place
end

--- Applies `sarthi` to `win`, placed at `place` (see `resolve`), with `options` for its `w:sarthi`, raising a mistake at
--- `level`
local function apply(win, sarthi, place, options, level)
  vim.w[win].sarthi = options
  -- At once, so sarthi has the window's style before Neovim draws it
  require('sarthi.floats').send(win)
  local mistake = rpc.request('place_window', win, place or vim.NIL)
  if mistake ~= vim.NIL then
    error('sarthi: `sarthi.place`: ' .. mistake, level)
  end
  mistake = rpc.request('window_font', win, sarthi.font or vim.NIL)
  if mistake ~= vim.NIL then
    error('sarthi: `sarthi.font`: ' .. mistake, level)
  end
  configs[win] = { place = vim.deepcopy(sarthi.place), font = vim.deepcopy(sarthi.font) }
end

---Like `vim.api.nvim_open_win()`, with a `sarthi` table in `config`.
---@param buf integer
---@param enter boolean
---@param config vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
---@return integer
function api.nvim_open_win(buf, enter, config)
  local plain, sarthi = split(config)
  local options = check(sarthi, {}, type(plain) == 'table' and plain.external, 3)
  -- What it's placed relative to, before entering it makes it the current window
  local place = sarthi and resolve(sarthi.place)
  local win = vim.api.nvim_open_win(buf, enter, plain)
  if sarthi ~= nil then
    -- A mistake in the place closes the window again, so the call does nothing but fail.
    local ok, mistake = pcall(apply, win, sarthi, place, options, 0)
    if not ok then
      vim.api.nvim_win_close(win, true)
      error(mistake, 2)
    end
  end
  return win
end

---Like `vim.api.nvim_win_set_config()`, with a `sarthi` table in `config`:
---what it gives replaces what was given before, key by key, and a section's
---values key by key too.
---@param win integer
---@param config vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
function api.nvim_win_set_config(win, config)
  win = win == 0 and vim.api.nvim_get_current_win() or win
  local plain, sarthi = split(config)
  -- External after this call: as given, or a float if given `relative`, or as it is
  local external = plain.external
  if external == nil then
    external = (plain.relative == nil or plain.relative == '') and vim.api.nvim_win_get_config(win).external
  end
  local options = check(sarthi, vim.w[win].sarthi or {}, external, 3)
  if next(plain) ~= nil then
    vim.api.nvim_win_set_config(win, plain)
  end

  local merged = vim.tbl_extend('force', configs[win] or {}, sarthi or {})
  -- No longer external: its place and fonts no longer apply.
  if not external then
    merged.place = nil
    merged.font = nil
  end
  apply(win, merged, resolve(merged.place), options, 3)
end

---Like `vim.api.nvim_win_get_config()`, with the window's `sarthi` table:
---its `w:sarthi`, and what else was given.
---@param win integer
---@return vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
function api.nvim_win_get_config(win)
  win = win == 0 and vim.api.nvim_get_current_win() or win
  local config = vim.api.nvim_win_get_config(win)
  local sarthi = vim.tbl_extend('force', vim.w[win].sarthi or {}, vim.deepcopy(configs[win] or {}))
  config.sarthi = next(sarthi) ~= nil and sarthi or nil
  return config
end

vim.api.nvim_create_autocmd('WinClosed', {
  group = vim.api.nvim_create_augroup('sarthi.api', {}),
  callback = function(args)
    configs[tonumber(args.match)] = nil
  end,
})

return api
