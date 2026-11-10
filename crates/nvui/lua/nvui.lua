-- The `nvui` Lua module, for the user's config: `require("nvui").setup({ … })`. nvui runs this before the user's
-- config, with its channel as the argument. Only nvui has the module, so a config shared with the TUI checks
-- `vim.g.nvui`, which nvui sets, first.
local channel = ...

-- The settings given so far, merged
local current = {}

-- `opts` merged into the settings given so far. A scroll animation replaces the previous one whole, since each curve
-- has its own keys.
local function merge(opts)
  local merged = vim.tbl_deep_extend('force', current, opts)
  if opts.scroll and opts.scroll.animation ~= nil then
    merged.scroll.animation = opts.scroll.animation
  end
  return merged
end

package.preload.nvui = function()
  local M = {}

  --- Sets nvui's settings: values given replace their previous ones, at first their defaults. Applies at once, so
  --- it can be called again later, e.g. from a keymap. A mistake is an error at the call.
  ---@param opts table?
  function M.setup(opts)
    if opts ~= nil and type(opts) ~= 'table' then
      error('nvui: setup() takes a table, not a ' .. type(opts), 2)
    end
    local merged = merge(opts or {})
    local mistake = vim.rpcrequest(channel, 'nvui.setup', merged)
    if mistake ~= vim.NIL then
      error('nvui: ' .. mistake, 2)
    end
    current = merged
  end

  return M
end
