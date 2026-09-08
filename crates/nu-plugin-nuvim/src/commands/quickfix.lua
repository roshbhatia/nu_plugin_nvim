local opts = ...
local window = opts.window
if window ~= nil then
  if window == 0 then window = vim.api.nvim_get_current_win() end
  if not vim.api.nvim_win_is_valid(window) then error("location-list window does not exist") end
end
local function get(fields)
  if window then return vim.fn.getloclist(window, fields) end
  return vim.fn.getqflist(fields)
end
local function metadata(selector)
  local fields = {id = 0, nr = 0, title = 0, size = 0, idx = 0, changedtick = 0, context = 0}
  for k, v in pairs(selector or {}) do fields[k] = v end
  local result = get(fields)
  result.server = opts.server
  result.window = window or vim.NIL
  result.count = result.size
  result.current = result.nr == get({nr = 0}).nr
  return result
end
if opts.operation == "history" then
  local output = {}
  for nr = 1, get({nr = "$"}).nr do output[#output + 1] = metadata({nr = nr}) end
  return output
end
local selector = {}
if opts.id ~= nil then
  if opts.id <= 0 or get({id = opts.id}).id ~= opts.id then error("quickfix list ID does not exist") end
  selector.id = opts.id
end
if opts.operation == "set" then
  local actions = {new = " ", append = "a", replace = "r"}
  local action = actions[opts.action or "replace"]
  if not action then error("quickfix action must be new, append, or replace") end
  if action == " " and opts.id then error("new lists cannot specify --id") end
  local fields = {items = opts.items}
  if opts.id then fields.id = opts.id end
  if opts.title then fields.title = opts.title
  elseif action ~= "a" then fields.title = "Nuvim" end
  if opts.context ~= nil then fields.context = opts.context end
  local status
  if window then status = vim.fn.setloclist(window, {}, action, fields)
  else status = vim.fn.setqflist({}, action, fields) end
  if status ~= 0 then error("Neovim rejected the quickfix update") end
  return metadata(selector)
end
local result = metadata(selector)
local fields = {items = 0}
if opts.id then fields.id = opts.id end
local output = {}
for _, item in ipairs(get(fields).items) do
  output[#output + 1] = {
    path = item.bufnr > 0 and vim.api.nvim_buf_get_name(item.bufnr) or item.filename,
    buffer = item.bufnr,
    row = item.lnum > 0 and item.lnum - 1 or nil,
    column = item.col > 0 and item.col - 1 or nil,
    end_row = item.end_lnum > 0 and item.end_lnum - 1 or nil,
    end_column = item.end_col > 0 and item.end_col - 1 or nil,
    text = item.text,
    type = item.type ~= "" and item.type or nil,
    valid = item.valid == 1,
  }
end
if opts.details then result.items = output; return result end
return output
