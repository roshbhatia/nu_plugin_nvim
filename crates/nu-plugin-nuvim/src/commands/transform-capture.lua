local opts = ...
local buffer = opts.buffer == 0 and vim.api.nvim_get_current_buf() or opts.buffer
if not vim.api.nvim_buf_is_loaded(buffer) then error("target buffer is not loaded") end
local result
if opts.selection then
  result = selection()
  result.kind = result.mode == "V" and "lines" or "text"
  if result.kind == "lines" then
    result["end"] = {row = result["end"].row + 1, column = 0}
  end
elseif #opts.range > 0 then
  local r = opts.range
  result = {
    buffer = buffer, kind = "text",
    start = {row = r[1], column = r[2]},
    ["end"] = {row = r[3], column = r[4]},
    lines = vim.api.nvim_buf_get_text(buffer, r[1], r[2], r[3], r[4], {}),
  }
else
  local lines = vim.api.nvim_buf_get_lines(buffer, 0, -1, true)
  result = {
    buffer = buffer, kind = "lines", lines = lines,
    start = {row = 0, column = 0}, ["end"] = {row = #lines, column = 0},
  }
end
local function boundary(row, col)
  local line = vim.api.nvim_buf_get_lines(buffer, row, row + 1, true)[1] or ""
  local byte = line:byte(col + 1)
  if byte and byte >= 128 and byte < 192 then error("column splits a UTF-8 character") end
end
if result.kind == "text" then
  boundary(result.start.row, result.start.column)
  boundary(result["end"].row, result["end"].column)
end
result.text = table.concat(result.lines, "\n")
result.changedtick = vim.api.nvim_buf_get_changedtick(buffer)
return result
