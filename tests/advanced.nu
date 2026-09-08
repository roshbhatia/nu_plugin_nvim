use std/assert

let server = $env.NUVIM_TEST_SERVER
let target = (["café" beta] | nuvim scratch --server $server)
let preview = (nuvim transform { str uppercase } --buffer $target.id --preview --server $server)
assert equal $preview.applied false
assert equal $preview.after "CAFÉ\nBETA"
assert equal (nuvim text --buffer $target.id --server $server | get lines) [café beta]

let applied = (nuvim transform { |text| $text | str uppercase } --buffer $target.id --server $server)
assert $applied.applied
assert equal (nuvim text --buffer $target.id --server $server | get lines) [CAFÉ BETA]
nuvim command undo --server $server | ignore
assert equal (nuvim text --buffer $target.id --server $server | get lines) [café beta]

let alternate = ([alternate] | nuvim scratch --server $server)
nuvim transform { |text|
  nuvim buffer use $alternate.id --server $server | ignore
  $text | str uppercase
} --buffer $target.id --server $server | ignore
assert equal (nuvim context --server $server | get buffer.id) $alternate.id
assert equal (nuvim text --buffer $target.id --server $server | get lines) [CAFÉ BETA]
assert equal (nuvim text --buffer $alternate.id --server $server | get lines) [alternate]

let conflict = (try {
  nuvim transform { |text|
    "intervening" | nuvim replace --buffer $target.id --server $server | ignore
    $text | str uppercase
  } --buffer $target.id --server $server
  null
} catch { |error| $error.msg })
assert ($conflict | str contains "transform conflict")
assert equal (nuvim text --buffer $target.id --server $server | get lines) [intervening]

let failure = (try {
  nuvim transform { error make {msg: "closure failed"} } --buffer $target.id --server $server
  null
} catch { |error| $error.msg })
assert ($failure | str contains "closure failed")
assert equal (nuvim text --buffer $target.id --server $server | get lines) [intervening]

"café" | nuvim replace --buffer $target.id --server $server | ignore
nuvim transform { "E" } --buffer $target.id --start-row 0 --start-column 3 --end-row 0 --end-column 5 --server $server | ignore
assert equal (nuvim text --buffer $target.id --server $server | get text) cafE
"café" | nuvim replace --buffer $target.id --server $server | ignore
let split = (try {
  nuvim transform { "bad" } --buffer $target.id --start-row 0 --start-column 4 --end-row 0 --end-column 5 --server $server
  null
} catch { |error| $error.msg })
assert ($split | str contains "splits a UTF-8 character")
nuvim buffer use $target.id --server $server | ignore
nuvim command 'execute "normal! gg0fév\<Esc>"' --server $server | ignore
nuvim transform { str uppercase } --selection --server $server | ignore
assert equal (nuvim text --buffer $target.id --server $server | get text) cafÉ

let first = ([{path: "README.md", row: 0, column: 0, text: first}] | nuvim quickfix set --action new --title first --context {owner: tests} --server $server)
let second = ([{path: "README.md", row: 1, column: 0, text: second}] | nuvim quickfix set --action new --title second --server $server)
assert ($first.id != $second.id)
[{path: "README.md", row: 2, column: 0, text: appended}] | nuvim quickfix set --action append --id $first.id --server $server | ignore
let details = (nuvim quickfix get --id $first.id --details --server $server)
assert equal $details.count 2
assert equal $details.title first
assert equal $details.context {owner: tests}
assert equal (nuvim quickfix get --server $server | get 0.text) second
assert ((nuvim quickfix history --server $server | where id == $first.id | length) == 1)
assert (nuvim quickfix history --server $server | where id == $second.id | get 0.current)
[] | nuvim quickfix set --window 0 --action new --title local --server $server | ignore
assert equal (nuvim quickfix get --window 0 --details --server $server | get title) local
assert equal (nuvim quickfix get --server $server | get 0.text) second
let missing = (try { nuvim quickfix get --id 999999 --server $server; null } catch { |error| $error.msg })
assert ($missing | str contains "ID does not exist")

let semantic = (["a😀b"] | nuvim scratch --server $server)
let fixture = (open --raw ($env.FILE_PWD | path join mock-lsp.lua))
nuvim lua $fixture $semantic.id --server $server | ignore
let references = (nuvim references --buffer $semantic.id --row 0 --column 5 --server $server)
assert equal ($references | get 0.column) 5
assert equal ($references | get 0.end_column) 6
assert equal (nuvim definition --buffer $semantic.id --row 0 --column 5 --server $server | get 0.column) 5
assert equal (nuvim symbols --buffer $semantic.id --server $server | get 0.name) b
let no_client = (try { nuvim symbols --buffer $alternate.id --server $server; null } catch { |error| $error.msg })
assert ($no_client | str contains "no attached LSP client")

let syntax = (["local answer = 42"] | nuvim scratch --filetype lua --server $server)
let nodes = (nuvim node --buffer $syntax.id --row 0 --column 7 --server $server)
assert equal ($nodes | get 0.type) identifier
assert equal ($nodes | get 0.text) answer
assert equal ($nodes | last | get type) chunk

let initial = (nuvim watch buffer --buffer $syntax.id --initial --server $server | first 2)
assert equal ($initial | get 0.event) ready
assert equal ($initial | get 1.lines) ["local answer = 42"]
assert equal ($initial | get 1.buffer) $syntax.id

let events = (nuvim watch buffer --buffer $syntax.id --server $server | each { |event|
  if $event.event == ready {
    "local answer = 43" | nuvim replace --buffer $syntax.id --server $server | ignore
  }
  $event
} | where event == nvim_buf_lines_event | first)
assert equal $events.lines ["local answer = 43"]

let diagnostics = (nuvim watch diagnostics --buffer $syntax.id --server $server | each { |event|
  if $event.event == ready {
    nuvim lua 'local b = ...; vim.diagnostic.set(vim.api.nvim_create_namespace("watch-test"), b, {{lnum=0, col=0, message="watched", severity=1}}); return true' $syntax.id --server $server | ignore
  }
  $event
} | first 2)
assert equal ($diagnostics | get 1.event) DiagnosticChanged
assert equal ($diagnostics | get 1.diagnostics.0.message) watched
assert (nuvim lua 'local b = ...; return vim.wait(1000, function() return #vim.api.nvim_get_autocmds({event="DiagnosticChanged", buffer=b}) == 0 end)' $syntax.id --server $server)

let saved = (nuvim lua 'local b = vim.api.nvim_create_buf(true, false); vim.api.nvim_buf_set_name(b, ...); vim.api.nvim_buf_set_lines(b, 0, -1, true, {"saved"}); return b' $env.NUVIM_TEST_OUTPUT --server $server)
let save_event = (nuvim watch save --buffer $saved --server $server | each { |event|
  if $event.event == ready {
    nuvim lua 'local b = ...; vim.api.nvim_buf_call(b, function() vim.cmd.write() end); return true' $saved --server $server | ignore
  }
  $event
} | first 2)
assert equal ($save_event | get 1.event) BufWritePost
assert equal (open --raw $env.NUVIM_TEST_OUTPUT | str trim) saved
assert (nuvim lua 'local b = ...; return vim.wait(1000, function() return #vim.api.nvim_get_autocmds({event="BufWritePost", buffer=b}) == 0 end)' $saved --server $server)

let detached = (nuvim watch buffer --buffer $saved --server $server | each { |event|
  if $event.event == ready {
    nuvim lua 'vim.api.nvim_buf_delete(..., {force=true}); return true' $saved --server $server | ignore
  }
  $event
} | collect)
assert equal ($detached | last | get event) nvim_buf_detach_event

let overflow = (try {
  nuvim watch buffer --buffer $syntax.id --server $server | each { |event|
    if $event.event == ready {
      nuvim lua 'local b = ...; for i = 1, 2000 do vim.api.nvim_buf_set_lines(b, 0, -1, true, {tostring(i)}) end; return true' $syntax.id --server $server | ignore
    }
    $event
  } | to nuon
  null
} catch { |error| $error.msg })
assert ($overflow | str contains "watch overflow")

print "Advanced editor workflows passed"
