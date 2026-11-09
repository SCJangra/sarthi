-- Makes nvui the clipboard provider for the + and * registers (:help clipboard). nvui runs this before the user's
-- config, which can still set its own g:clipboard. Copying and pasting ask nvui over its channel, which nvui named.
local channel
for _, chan in ipairs(vim.api.nvim_list_chans()) do
  if chan.client and chan.client.name == 'nvui' then
    channel = chan.id
  end
end
assert(channel, "nvui's channel not found")

local function copy(register)
  return function(lines, regtype)
    vim.rpcrequest(channel, 'nvui.set_clipboard', register, lines, regtype)
  end
end

local function paste(register)
  return function()
    return vim.rpcrequest(channel, 'nvui.get_clipboard', register)
  end
end

vim.g.clipboard = {
  name = 'nvui',
  copy = { ['+'] = copy('+'), ['*'] = copy('*') },
  paste = { ['+'] = paste('+'), ['*'] = paste('*') },
}
