local channel, buffer, event = ...
if not vim.api.nvim_buf_is_loaded(buffer) then error("watch buffer is not loaded") end
local group = vim.api.nvim_create_augroup("nuvim-watch-" .. channel, {clear = true})
local timer = vim.uv.new_timer()
local closed = false
local function cleanup()
  if closed then return end
  closed = true
  timer:stop()
  timer:close()
  pcall(vim.api.nvim_del_augroup_by_id, group)
end
local function notify(payload)
  local ok, sent = pcall(vim.rpcnotify, channel, "nuvim_event", payload)
  if not ok or not sent then cleanup() end
end
vim.api.nvim_create_autocmd(event, {
  group = group, buffer = buffer,
  callback = function(args)
    local payload = {event = args.event, buffer = args.buf, path = vim.api.nvim_buf_get_name(args.buf)}
    payload.changedtick = vim.api.nvim_buf_get_changedtick(args.buf)
    if event == "DiagnosticChanged" then
      payload.diagnostics = {}
      for _, d in ipairs(vim.diagnostic.get(args.buf)) do
        payload.diagnostics[#payload.diagnostics + 1] = {
          row = d.lnum, column = d.col, end_row = d.end_lnum, end_column = d.end_col,
          severity = vim.diagnostic.severity[d.severity], message = d.message, source = d.source,
        }
      end
    end
    notify(payload)
  end,
})
vim.api.nvim_create_autocmd("BufWipeout", {group = group, buffer = buffer, once = true, callback = function()
  notify({event = "detach", buffer = buffer})
  cleanup()
  pcall(vim.fn.chanclose, channel)
end})
timer:start(250, 250, vim.schedule_wrap(function()
  if vim.tbl_isempty(vim.api.nvim_get_chan_info(channel)) then cleanup() end
end))
return true
