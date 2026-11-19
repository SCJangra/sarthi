`require('sarthi')`, for your config: `require('sarthi').setup({ … })`. Only Sarthi has the module, so a config
shared with the TUI checks `vim.g.sarthi`, which Sarthi sets, first.
