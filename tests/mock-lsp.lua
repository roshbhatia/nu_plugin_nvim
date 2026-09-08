local buffer = ...
vim.api.nvim_buf_set_name(buffer, "nuvim-semantic-fixture")
local id = vim.lsp.start({
  name = "nuvim-test-lsp",
  cmd = function(dispatchers)
    local closing, sequence = false, 0
    return {
      request = function(method, params, callback)
        sequence = sequence + 1
        local result
        local range = {start = {line = 0, character = 3}, ["end"] = {line = 0, character = 4}}
        if method == "initialize" then
          result = {capabilities = {positionEncoding = "utf-16", documentSymbolProvider = true, referencesProvider = true, definitionProvider = true}}
        elseif method == "textDocument/documentSymbol" then
          result = {{name = "b", kind = 13, range = range, selectionRange = range}}
        elseif method == "textDocument/references" or method == "textDocument/definition" then
          assert(params.position.character == 3, "request must use UTF-16 positions")
          result = {{uri = vim.uri_from_bufnr(buffer), range = range}}
        end
        vim.schedule(function() callback(nil, result) end)
        return true, sequence
      end,
      notify = function() return true end,
      is_closing = function() return closing end,
      terminate = function() closing = true; dispatchers.on_exit(0, 0) end,
    }
  end,
}, {bufnr = buffer})
assert(vim.wait(1000, function() return vim.lsp.get_client_by_id(id).initialized end), "LSP initialization timed out")
return id
