local opts = ...
local buffer = opts.buffer or 0
if buffer == 0 then buffer = vim.api.nvim_get_current_buf() end
if not vim.api.nvim_buf_is_loaded(buffer) then error("query buffer is not loaded") end
if (opts.row == nil) ~= (opts.column == nil) then error("provide both --row and --column") end
local cursor = {1, 0}
if buffer == vim.api.nvim_get_current_buf() then cursor = vim.api.nvim_win_get_cursor(0) end
local row, column = opts.row or (cursor[1] - 1), opts.column or cursor[2]
local line = vim.api.nvim_buf_get_lines(buffer, row, row + 1, true)[1]
if not line or column > #line then error("query position is outside the buffer") end
local byte = line:byte(column + 1)
if byte and byte >= 128 and byte < 192 then error("column splits a UTF-8 character") end
local tick = vim.api.nvim_buf_get_changedtick(buffer)
if opts.operation == "nuvim node" then
  local parser = vim.treesitter.get_parser(buffer)
  parser:parse()
  local node = vim.treesitter.get_node({bufnr = buffer, pos = {row, column}})
  local output = {}
  while node do
    local sr, sc, er, ec = node:range()
    output[#output + 1] = {
      server = opts.server, buffer = buffer, changedtick = tick,
      type = node:type(), row = sr, column = sc, end_row = er, end_column = ec,
      text = vim.treesitter.get_node_text(node, buffer),
    }
    node = node:parent()
  end
  return output
end
local methods = {
  ["nuvim symbols"] = "textDocument/documentSymbol",
  ["nuvim references"] = "textDocument/references",
  ["nuvim definition"] = "textDocument/definition",
}
local method = methods[opts.operation]
local clients = vim.lsp.get_clients({bufnr = buffer, method = method})
if #clients == 0 then error("no attached LSP client supports " .. method) end
local timeout = opts.timeout or 2000
if timeout < 1 or timeout > 8000 then error("timeout must be between 1 and 8000 milliseconds") end
local results, err = vim.lsp.buf_request_sync(buffer, method, function(client)
  local position = vim.str_utfindex(line, client.offset_encoding, column, false)
  return {
    textDocument = {uri = vim.uri_from_bufnr(buffer)},
    position = {line = row, character = position},
    context = {includeDeclaration = true},
  }
end, timeout)
if err then error("LSP query failed: " .. tostring(err)) end
if vim.api.nvim_buf_get_changedtick(buffer) ~= tick then error("query conflict: buffer changed during LSP request") end
local output = {}
local function add_location(location, client, extra)
  local items = vim.lsp.util.locations_to_items({location}, client.offset_encoding)
  for _, item in ipairs(items) do
    local record = {
      server = opts.server, client = client.id, client_name = client.name,
      path = item.filename, row = item.lnum - 1, column = item.col - 1,
      end_row = item.end_lnum and item.end_lnum - 1 or nil,
      end_column = item.end_col and item.end_col - 1 or nil,
      text = item.text, changedtick = tick,
    }
    for key, value in pairs(extra or {}) do record[key] = value end
    output[#output + 1] = record
  end
end
local function add_symbol(symbol, client, container)
  local location = symbol.location or {
    uri = vim.uri_from_bufnr(buffer), range = symbol.selectionRange or symbol.range,
  }
  add_location(location, client, {name = symbol.name, kind = vim.lsp.protocol.SymbolKind[symbol.kind], container = container or symbol.containerName})
  for _, child in ipairs(symbol.children or {}) do add_symbol(child, client, symbol.name) end
end
table.sort(clients, function(a, b) return a.id < b.id end)
for _, client in ipairs(clients) do
  local response = results[client.id]
  if not response then error("LSP client returned no response: " .. client.name) end
  if response.error then error("LSP client " .. client.name .. ": " .. vim.inspect(response.error)) end
  local result = response.result
  if result and result ~= vim.NIL then
    if result.uri or result.targetUri then result = {result} end
    for _, value in ipairs(result) do
      if opts.operation == "nuvim symbols" then add_symbol(value, client)
      else add_location(value, client) end
    end
  end
end
return output
