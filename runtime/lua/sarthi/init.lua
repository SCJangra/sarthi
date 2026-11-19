---@mod sarthi Lua module
---@brief [[
---`require("sarthi")`, for your config: `require("sarthi").setup({ … })`. Only
---Sarthi has the module, so a config shared with the TUI checks `vim.g.sarthi`,
---which Sarthi sets, first.
---@brief ]]
local rpc = require('sarthi.rpc')

local sarthi = {}

-- Neovim's window functions that take a window config, with a `sarthi` table in it (see |sarthi.api|)
sarthi.api = require('sarthi.api')

---Sets Sarthi's settings: those given, and the defaults of the rest, in place
---of what an earlier call gave. Applies at once, so it can be called again
---later, e.g. from a keymap. A mistake is an error at the call.
---@param opts? table The settings (see |sarthi-settings|)
function sarthi.setup(opts)
  rpc.request('setup', opts)
end

---The settings in effect: those given to `setup()`, and the defaults of the
---rest (|sarthi-settings|), e.g. to place a window past another's padding
---when it has the user's. A copy: changing it changes nothing.
---@return table
function sarthi.settings()
  return rpc.request('get_settings')
end

---Sets the font size, in points (4 to 96), by setting `:h` in `guifont`.
---@param points number Size in points
function sarthi.set_font_size(points)
  rpc.request('set_font_size', points)
end

---Makes the font `delta` points bigger, or smaller if negative, by setting
---`:h` in `guifont`. The size stays within 4 to 96 points.
---@param delta number Points to add
function sarthi.change_font_size(delta)
  rpc.request('change_font_size', delta)
end

---Sets `guifont` back to its value before the font size was first changed,
---unless it was set otherwise since.
function sarthi.reset_font_size()
  rpc.request('reset_font_size')
end

return sarthi
