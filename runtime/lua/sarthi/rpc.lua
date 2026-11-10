-- How sarthi's Lua talks to sarthi: over sarthi's channel, which `startup.lua` sets. Internal to sarthi's modules.
local M = {}

---@type integer
M.channel = nil

--- Asks sarthi `method` with the arguments, and returns its answer. A failed request (e.g. arguments sarthi can't
--- decode) is raised at the caller of the module's function that asked.
function M.request(method, ...)
  local ok, answer = pcall(vim.rpcrequest, M.channel, method, ...)
  if not ok then
    -- Without Neovim's "Vim:Invoking 'method' on channel …:" line before the message
    error('sarthi: ' .. answer:gsub('^Vim:Invoking.-\n', ''), 3)
  end
  return answer
end

--- Tells sarthi `method` with the arguments, without waiting for it
function M.notify(method, ...)
  vim.rpcnotify(M.channel, method, ...)
end

return M
