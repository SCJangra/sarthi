-- How nvui's Lua talks to nvui: over nvui's channel, which `startup.lua` sets. Internal to nvui's modules.
local M = {}

---@type integer
M.channel = nil

--- Asks nvui `method` with the arguments, and returns its answer. A failed request (e.g. arguments nvui can't decode)
--- is raised at the caller of the module's function that asked.
function M.request(method, ...)
  local ok, answer = pcall(vim.rpcrequest, M.channel, method, ...)
  if not ok then
    -- Without Neovim's "Vim:Invoking 'method' on channel …:" line before the message
    error('nvui: ' .. answer:gsub('^Vim:Invoking.-\n', ''), 3)
  end
  return answer
end

return M
