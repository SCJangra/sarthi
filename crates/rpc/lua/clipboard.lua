-- Makes nvui the clipboard provider for the + and * registers (:help clipboard). nvui runs this before the user's
-- config, which can still set its own g:clipboard. Copying and pasting ask nvui over its channel, the argument.
local channel = ...

local function copy(register)
  return function(lines, regtype)
    vim.rpcrequest(channel, 'set_clipboard', register, lines, regtype)
  end
end

local function paste(register)
  return function()
    return vim.rpcrequest(channel, 'get_clipboard', register)
  end
end

vim.g.clipboard = {
  name = 'nvui',
  copy = { ['+'] = copy('+'), ['*'] = copy('*') },
  paste = { ['+'] = paste('+'), ['*'] = paste('*') },
}
