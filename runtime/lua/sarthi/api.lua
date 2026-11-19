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
---The `sarthi` table (|sarthi.WindowConfig|) takes the window's own values of
---the settings' sections about a window, with the same keys
---(|sarthi-settings|), each in place of the setting's; for those it doesn't
---give, the settings'. `shadow` and `scroll` apply to any window; `outline`,
---`corners` and `padding` only to external windows (`external = true`), as
---do `place` and `font`. The values are kept in the window's `w:sarthi`,
---marked `managed`, which a plugin can also set itself (|sarthi-w:sarthi|).
---@brief ]]
local rpc = require('sarthi.rpc')

local api = {}

---Where Sarthi places an external window, over Neovim's screen: `{ center =
---true }` centers it, as it is until placed; otherwise it's placed like a
---floating window, with its `anchor` corner at `row`, `col` of what it's
---`relative` to.
---
---Like a floating window with a border, whose frame, its padding and
---outline (|sarthi-padding|, |sarthi-outline|), takes the border's place:
---the frame's `anchor` corner is at `row`, `col`, and the window's cells
---are inside it. `row` and `col` are whole cells, a fraction dropped. A
---window it's relative to counts from its text, inside its frame, border
---and winbar, like Neovim's floats do. Like those, it's kept on the screen
---with its frame, above the command line, and with `fixed` not moved
---sideways unless anchored on the right. `offset` moves it further by
---logical pixels.
---@class sarthi.WindowPlace
---@field center? boolean Center it
---@field relative? 'editor'|'win'|'cursor' What it's placed relative to
---@field win? integer For 'win': the window, or the current one
---@field anchor? 'NW'|'NE'|'SW'|'SE' Its corner at `row`, `col`
---@field row? number Rows down
---@field col? number Columns right
---@field offset? { x?: number, y?: number } Logical pixels right and down
---@field fixed? boolean Not moved sideways to stay on the screen

---The fonts an external window's text is drawn with. Either left out is
---`guifont`'s. The window is as many of its own cells as its config says.
---@class sarthi.WindowFont
---@field family? string|string[] One or more, in fallback order
---@field size? number In points, like `guifont`'s `:h`

---@class sarthi.WindowConfig
---@field place? sarthi.WindowPlace External windows only
---@field font? sarthi.WindowFont External windows only
---@field shadow? table Its own |sarthi-shadow| values
---@field outline? table External windows only: its own |sarthi-outline| values
---@field corners? table External windows only: its own |sarthi-corners| values
---@field padding? table External windows only: its own |sarthi-padding| values
---@field scroll? table Its own |sarthi-scroll| values

-- Sarthi does the work (`crates/sarthi/src/api`); these only call it.

---Like `vim.api.nvim_open_win()`, with a `sarthi` table in `config`.
---@param buf integer
---@param enter boolean
---@param config vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
---@return integer
function api.nvim_open_win(buf, enter, config)
  return rpc.request('nvim_open_win', buf, enter, config)
end

---Like `vim.api.nvim_win_set_config()`, with a `sarthi` table in `config`:
---what it gives replaces what was given before, key by key, and a section's
---values key by key too.
---@param win integer
---@param config vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
function api.nvim_win_set_config(win, config)
  rpc.request('nvim_win_set_config', win, config)
end

---Like `vim.api.nvim_win_get_config()`, with the window's `sarthi` table:
---its `w:sarthi`, and what else was given.
---@param win integer
---@return vim.api.keyset.win_config|{ sarthi?: sarthi.WindowConfig }
function api.nvim_win_get_config(win)
  return rpc.request('nvim_win_get_config', win)
end

-- So Sarthi forgets what it kept about a window that closed
vim.api.nvim_create_autocmd('WinClosed', {
  group = vim.api.nvim_create_augroup('sarthi.api', {}),
  callback = function(args)
    rpc.notify('win_closed', tonumber(args.match))
  end,
})

return api
