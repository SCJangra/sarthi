-- What nvui runs before the user's config, with its channel and its Lua modules (`lua/`, as module name and code)
-- as the arguments. The modules are embedded in nvui, so they're preloaded rather than found in 'runtimepath'.
local channel, modules = ...

for _, module in ipairs(modules) do
  local name, code = module[1], module[2]
  package.preload[name] = assert(loadstring(code, '=' .. name))
end

require('nvui.rpc').channel = channel

-- What works without the user's config asking for it
require('nvui.clipboard')
require('nvui.scroll')
