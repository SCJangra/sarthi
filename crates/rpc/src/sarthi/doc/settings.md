Set with `require('sarthi').setup({ … })`, by section, e.g.

```lua
require('sarthi').setup({
  shadow = { opacity = 0.5 },
  outline = { enabled = true, highlight = 'Comment' },
})
```

Each section and key is optional: what isn't given has its default. `setup()` can be called again, e.g. from a keymap,
and applies at once; each call replaces what the last one gave, so what it leaves out is back to its default. A
mistake is an error at the call, and keeps the settings as they were.

Each key says whether a window can have its own value of it too ([Per-window settings](#sarthi-w:sarthi)): "global, or per
window" if it can, "global only" if not.
