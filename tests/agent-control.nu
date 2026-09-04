let server = $env.NUVIM_TEST_SERVER

let scratch = (
  [alpha "café 🌊" omega]
  | nuvim scratch --server $server --name nuvim-agent-test --filetype text
)

let moved = (nuvim cursor set 1 5 --server $server)
if $moved != {row: 1, column: 5} {
  error make {msg: $"cursor move returned ($moved | to nuon)"}
}

"!" | nuvim edit 1 5 --server $server | ignore
let edited = (nuvim text --server $server | get lines)
if $edited != [alpha "café! 🌊" omega] {
  error make {msg: $"UTF-8 edit returned ($edited | to nuon)"}
}

"pha\nCAFÉ" | nuvim edit 0 2 --end-row 1 --end-column 6 --server $server | ignore
let multiline = (nuvim text --server $server | get lines)
if $multiline != [alpha "CAFÉ 🌊" omega] {
  error make {msg: $"multiline edit returned ($multiline | to nuon)"}
}

let current = (["current buffer"] | nuvim scratch --server $server --name current-buffer)
"remote-" | nuvim edit 2 0 --buffer $scratch.id --server $server | ignore
let non_current = (nuvim text --buffer $scratch.id --server $server | get lines)
if $non_current != [alpha "CAFÉ 🌊" remote-omega] {
  error make {msg: $"non-current edit returned ($non_current | to nuon)"}
}
if (nuvim context --server $server | get buffer.id) != $current.id {
  error make {msg: "editing another buffer changed the current buffer"}
}

let raw_current = (nuvim call nvim_get_current_buf --server $server)
if $raw_current.id != $current.id or $raw_current.kind != "buffer" {
  error make {msg: $"raw handle conversion returned ($raw_current | to nuon)"}
}
let raw_count = (nuvim call nvim_buf_line_count $raw_current --server $server)
if $raw_count != 1 {
  error make {msg: $"raw call returned line count ($raw_count)"}
}

let lua_result = (
  nuvim lua 'local left, right, values = ...; return {left = left, left_type = type(left), right = right, right_type = type(right), values = values}'
    2 3 ["é" "line\ntwo"] --server $server
)
if $lua_result.left != 2 or $lua_result.right != 3 or $lua_result.values != ["é" "line\ntwo"] {
  error make {msg: $"Lua value conversion returned ($lua_result | to nuon)"}
}

nuvim lua '
  local buffer = ...
  local namespace = vim.api.nvim_create_namespace("nuvim-test")
  vim.diagnostic.set(namespace, buffer, {{
    lnum = 1,
    col = 0,
    severity = vim.diagnostic.severity.WARN,
    message = "Review café",
    source = "nuvim-test",
  }})
  return true
' $scratch.id --server $server | ignore
let diagnostic = (
  nuvim diagnostics --server $server
  | where buffer == $scratch.id
  | first
)
if $diagnostic.row != 1 or $diagnostic.message != "Review café" {
  error make {msg: $"diagnostic conversion returned ($diagnostic | to nuon)"}
}

[{path: $env.NUVIM_TEST_FILE, row: 2, column: 4, end_row: 2, end_column: 8, text: "Review café", type: "W"}]
| nuvim quickfix set --server $server --title "Nuvim test" | ignore
let quickfix = (nuvim quickfix get --server $server | first)
if $quickfix.row != 2 or $quickfix.column != 4 or $quickfix.end_column != 8 or $quickfix.text != "Review café" {
  error make {msg: $"quickfix conversion returned ($quickfix | to nuon)"}
}

let opened = (
  nuvim open --server $server $env.NUVIM_TEST_FILE
  | first
)
if ($opened.path | path expand) != ($env.NUVIM_TEST_FILE | path expand) {
  error make {msg: $"opened unexpected path ($opened.path)"}
}

nuvim buffer use $scratch.id --server $server | ignore
let command_context = (nuvim command "setlocal filetype=nuvimtest" --server $server)
if $command_context.buffer.filetype != "nuvimtest" {
  error make {msg: "Ex command did not update the current buffer"}
}

print ({
  server: $server
  buffer: $scratch.id
  cursor: (nuvim cursor --server $server)
  lines: (nuvim text --server $server | get lines)
  opened: $opened.path
  filetype: $command_context.buffer.filetype
  diagnostic: $diagnostic.message
  quickfix: $quickfix.text
  lua_values: [$lua_result.left $lua_result.right]
} | to nuon)
