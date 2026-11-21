The sections of the settings' `window` (`scroll_animation`, `shadow`, `outline`, `rounded_corners`, `padding`) can be
set for one window too, in its `w:sarthi` table: the same sections with the same keys, but `exclude_filetypes`. Each
key given is the window's, whatever the setting's; only the filetypes a setting excludes win over it: such a window
never has that effect, even with `enabled = true`.

```lua
vim.w[win].sarthi = { outline = { enabled = true, width = 2 } }
```

For a key it doesn't give, a window has the setting's `scroll_animation` and `shadow` (every window scrolls, and every
floating and external window casts a shadow). `outline`, `rounded_corners` and `padding` are only ever around external
windows: Neovim places floating windows in its cells, without room for them. The settings' are for windows whose
`w:sarthi` has `managed = true`; others have them only if their `w:sarthi` turns them on, with the defaults for the
keys it doesn't give.

```lua
vim.w[win].sarthi = { managed = true, padding = { size = 2 } }
```

The [window functions](#sarthi.api) set it from the `sarthi` table of a window's config, marked `managed`. A mistake in a
table set directly is shown once, and the window gets nothing of its own until it's fixed.
