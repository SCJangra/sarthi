-- `require("nvui")`, for the user's config: `require("nvui").setup({ … })`. Only nvui has the module, so a config
-- shared with the TUI checks `vim.g.nvui`, which nvui sets, first.
local rpc = require('nvui.rpc')

local M = {}

-- The settings given so far, merged
local current = {}

-- The animations in the settings, by section and key
local animations = { scroll = { 'animation' }, cursor = { 'animation', 'short_animation' } }

-- `opts` merged into the settings given so far. An animation replaces the previous one whole, since each curve has its
-- own keys.
local function merge(opts)
  local merged = vim.tbl_deep_extend('force', current, opts)
  for section, keys in pairs(animations) do
    for _, key in ipairs(keys) do
      if type(opts[section]) == 'table' and opts[section][key] ~= nil then
        merged[section][key] = opts[section][key]
      end
    end
  end
  return merged
end

--- Sets nvui's settings: values given replace their previous ones, at first their defaults. Applies at once, so it
--- can be called again later, e.g. from a keymap. A mistake is an error at the call.
---@param opts table?
function M.setup(opts)
  if opts ~= nil and type(opts) ~= 'table' then
    error('nvui: setup() takes a table, not a ' .. type(opts), 2)
  end
  local merged = merge(opts or {})
  -- Settings nvui can't decode (unknown keys, wrong types) fail the request; values it decodes but finds wrong (out
  -- of range) come back as the answer. Either way the mistake is raised here, at the caller's setup().
  local mistake = rpc.request('setup', merged)
  if mistake ~= vim.NIL then
    error('nvui: ' .. mistake, 2)
  end
  current = merged
  -- Floating windows open already get the new settings too.
  require('nvui.floats').resend()
end

--- Sets the font size, in points (4 to 96), by setting `:h` in `guifont`.
---@param points number
function M.set_font_size(points)
  vim.o.guifont = rpc.request('set_font_size', vim.o.guifont, points)
end

--- Makes the font `delta` points bigger, or smaller if negative, by setting `:h` in `guifont`. The size stays within
--- 4 to 96 points.
---@param delta number
function M.change_font_size(delta)
  vim.o.guifont = rpc.request('change_font_size', vim.o.guifont, delta)
end

--- Sets `guifont` back to its value before the font size was first changed, unless it was set otherwise since.
function M.reset_font_size()
  vim.o.guifont = rpc.request('reset_font_size', vim.o.guifont)
end

return M
