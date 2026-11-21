`require('sarthi').api`: Neovim's window functions that take a window config, taking a `sarthi` table in it too, for
what only Sarthi does with windows. Everything else is `vim.api`. Only Sarthi has the module, so a plugin shared with
the TUI checks `vim.g.sarthi` first, and opens its window another way otherwise.

```lua
require('sarthi').api.nvim_open_win(buf, false, {
  external = true, width = 40, height = 30,
  sarthi = {
    place = { relative = 'editor', row = 2, col = 4 },
    shadow = { opacity = 0.6 },
  },
})
```

The `sarthi` table ([`WindowConfig`](#sarthi.WindowConfig)) takes the window's own values of the sections of the settings'
`window`, with the same keys ([Settings](#sarthi-settings)), each in place of the setting's; for those it doesn't give, the
settings'. `shadow` and `scroll_animation` apply to any window; `outline`, `rounded_corners` and `padding` only to
external windows (`external = true`), as do `place` and `font`. The values are kept in the window's `w:sarthi`, marked
`managed`, which a plugin can also set itself ([Per-window settings](#sarthi-w:sarthi)).
