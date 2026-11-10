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

-- Asks nvui `method` with the arguments, and returns its answer. A failed request (e.g. arguments nvui can't decode)
-- is raised at the caller of the module's function.
local function request(method, ...)
  local ok, answer = pcall(vim.rpcrequest, channel, method, ...)
  if not ok then
    -- Without Neovim's "Vim:Invoking 'method' on channel …:" line before the message
    error('nvui: ' .. answer:gsub('^Vim:Invoking.-\n', ''), 3)
  end
  return answer
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
    -- Settings nvui can't decode (unknown keys, wrong types) fail the request; values it decodes but finds wrong
    -- (out of range) come back as the answer. Either way the mistake is raised here, at the caller's setup().
    local mistake = request('setup', merged)
    if mistake ~= vim.NIL then
      error('nvui: ' .. mistake, 2)
    end
    current = merged
  end

  --- Sets the font size, in points (4 to 96), by setting `:h` in `guifont`.
  ---@param points number
  function M.set_font_size(points)
    vim.o.guifont = request('set_font_size', vim.o.guifont, points)
  end

  --- Makes the font `delta` points bigger, or smaller if negative, by setting `:h` in `guifont`. The size stays
  --- within 4 to 96 points.
  ---@param delta number
  function M.change_font_size(delta)
    vim.o.guifont = request('change_font_size', vim.o.guifont, delta)
  end

  --- Sets `guifont` back to its value before the font size was first changed, unless it was set otherwise since.
  function M.reset_font_size()
    vim.o.guifont = request('reset_font_size', vim.o.guifont)
  end

  return M
end
