local snapshot, lines = ...
local buffer = snapshot.buffer
if not vim.api.nvim_buf_is_loaded(buffer) then error("transform conflict: target buffer was unloaded") end
if vim.api.nvim_buf_get_changedtick(buffer) ~= snapshot.changedtick then
  error("transform conflict: target buffer changed; capture and transform it again")
end
if not vim.bo[buffer].modifiable then error("target buffer is not modifiable") end
if snapshot.kind == "lines" then
  vim.api.nvim_buf_set_lines(buffer, snapshot.start.row, snapshot["end"].row, true, lines)
else
  vim.api.nvim_buf_set_text(buffer, snapshot.start.row, snapshot.start.column,
    snapshot["end"].row, snapshot["end"].column, lines)
end
return vim.api.nvim_buf_get_changedtick(buffer)
