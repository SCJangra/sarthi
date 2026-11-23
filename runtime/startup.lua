-- What sarthi runs before the user's config, with its channel, its Lua modules (`lua/`, as module name and code) and the
-- files it writes where 'runtimepath' finds them (as path and content) as the arguments. The modules are embedded in
-- sarthi, so they're preloaded rather than found in 'runtimepath'.
local channel, modules, files = ...

for _, module in ipairs(modules) do
  local name, code = module[1], module[2]
  package.preload[name] = assert(loadstring(code, '=' .. name))
end

require('sarthi.rpc').channel = channel

-- What works without the user's config asking for it.
require('sarthi.clipboard')
require('sarthi.scroll')
require('sarthi.floats')

-- The help file and the Lua types are embedded too: each written where 'runtimepath' finds it when it isn't there as it
-- is, and the help file's tags made again with it.
local dir = vim.fn.stdpath('cache') .. '/sarthi'
for _, entry in ipairs(files) do
  local path, content = dir .. '/' .. entry[1], entry[2]
  local file = io.open(path)
  local written = file and file:read('*a')
  if file then
    file:close()
  end
  local help = vim.endswith(path, '.txt')
  local tags = vim.fs.dirname(path) .. '/tags'
  if written ~= content or (help and vim.fn.filereadable(tags) == 0) then
    vim.fn.mkdir(vim.fs.dirname(path), 'p')
    file = assert(io.open(path, 'w'))
    file:write(content)
    file:close()
    if help then
      vim.cmd.helptags(vim.fn.fnameescape(vim.fs.dirname(path)))
    end
  end
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
