-- Makes nvui the clipboard provider for the + and * registers (:help clipboard). nvui loads this before the user's
-- config, which can still set its own g:clipboard. Copying and pasting ask nvui over its channel.
local rpc = require('nvui.rpc')

local function copy(register)
  return function(lines, regtype)
    vim.rpcrequest(rpc.channel, 'set_clipboard', register, lines, regtype)
  end
end

local function paste(register)
  return function()
    return vim.rpcrequest(rpc.channel, 'get_clipboard', register)
  end
end

vim.g.clipboard = {
  name = 'nvui',
  copy = { ['+'] = copy('+'), ['*'] = copy('*') },
  paste = { ['+'] = paste('+'), ['*'] = paste('*') },
}
