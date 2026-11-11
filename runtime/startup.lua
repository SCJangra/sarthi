-- What sarthi runs before the user's config, with its channel, its Lua modules (`lua/`, as module name and code) and its
-- help file as the arguments. The modules are embedded in sarthi, so they're preloaded rather than found in
-- 'runtimepath'.
local channel, modules, help = ...

for _, module in ipairs(modules) do
  local name, code = module[1], module[2]
  package.preload[name] = assert(loadstring(code, '=' .. name))
end

require('sarthi.rpc').channel = channel

-- What works without the user's config asking for it
require('sarthi.clipboard')
require('sarthi.scroll')
require('sarthi.floats')

-- The help file is embedded too: written where 'runtimepath' finds it, with its tags, when it isn't there as it is.
local dir = vim.fn.stdpath('cache') .. '/sarthi'
local path = dir .. '/doc/sarthi.txt'
local file = io.open(path)
local written = file and file:read('*a')
if file then
  file:close()
end
if written ~= help or vim.fn.filereadable(dir .. '/doc/tags') == 0 then
  vim.fn.mkdir(dir .. '/doc', 'p')
  file = assert(io.open(path, 'w'))
  file:write(help)
  file:close()
  vim.cmd.helptags(vim.fn.fnameescape(dir .. '/doc'))
end
vim.opt.runtimepath:append(dir)
-- Plugin managers can set 'runtimepath' anew while the config loads (lazy.nvim does), dropping it: added again after.
vim.api.nvim_create_autocmd('VimEnter', {
  once = true,
  callback = function()
    if not vim.tbl_contains(vim.opt.runtimepath:get(), dir) then
      vim.opt.runtimepath:append(dir)
    end
  end,
})
