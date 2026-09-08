use std/assert

let server = $env.NUVIM_TEST_SERVER

let source = ('{"items":[2,1],"name":"café"}' | nuvim scratch --server $server)
nuvim command 'execute "normal! ggV\<Esc>"' --server $server | ignore
let before = (nuvim text --buffer $source.id --server $server | get text)
let preview = (do {
  source ../recipes/selection-format/main.nu
  main --server $server
})
assert equal $preview.source.buffer $source.id
assert equal (nuvim text --buffer $source.id --server $server | get text) $before
assert equal (nuvim text --buffer $preview.preview.id --server $server | get text | from json) ($before | from json)
assert equal $preview.preview.filetype json

let sortable = ([zebra alpha café] | nuvim scratch --server $server)
nuvim command 'execute "normal! ggVG\<Esc>"' --server $server | ignore
let sorted = (do {
  source ../recipes/selection-format/main.nu
  main --server $server --format sort
})
assert equal (nuvim text --buffer $sorted.preview.id --server $server | get lines) [alpha café zebra]
assert equal (nuvim text --buffer $sortable.id --server $server | get lines) [zebra alpha café]

nuvim buffer use $sortable.id --server $server | ignore
let buffer_count = (nuvim buffers --server $server | length)
let invalid_json = (try {
  do {
    source ../recipes/selection-format/main.nu
    main --server $server
  }
  false
} catch { true })
assert $invalid_json
assert equal (nuvim buffers --server $server | length) $buffer_count
assert equal (nuvim context --server $server | get buffer.id) $sortable.id

let report = (do {
  source ../recipes/scratch-refresh/main.nu
  main --server $server
})
let added = ([new] | nuvim scratch --server $server)
let buffer_count = (nuvim buffers --server $server | length)
let refreshed = (do {
  source ../recipes/scratch-refresh/main.nu
  main --server $server --buffer $report.id
})
assert equal $refreshed.id $report.id
assert equal (nuvim buffers --server $server | length) $buffer_count
assert equal (nuvim context --server $server | get buffer.id) $added.id
let contents = (nuvim text --buffer $report.id --server $server | get text | from nuon)
assert ($added.id in $contents.id)
assert ($report.id not-in $contents.id)

nuvim command enew --server $server | ignore
let normal = (nuvim context --server $server | get buffer.id)
"keep this" | nuvim replace --buffer $normal --server $server | ignore
let refused = (try {
  do {
    source ../recipes/scratch-refresh/main.nu
    main --server $server --buffer $normal
  }
  null
} catch { |error| $error.msg })
assert ($refused | str contains "requires a nofile buffer")
assert equal (nuvim text --buffer $normal --server $server | get text) "keep this"

nuvim lua '
  local buffer = ...
  vim.api.nvim_buf_set_name(buffer, "nuvim-recipe-diagnostics")
  local namespace = vim.api.nvim_create_namespace("nuvim-recipes")
  vim.diagnostic.set(namespace, buffer, {
    {lnum = 0, col = 0, severity = vim.diagnostic.severity.ERROR, message = "first", source = "recipes"},
    {lnum = 0, col = 2, severity = vim.diagnostic.severity.WARN, message = "second", source = "recipes"},
    {lnum = 0, col = 3, severity = vim.diagnostic.severity.ERROR, message = "third", source = "other"},
  })
  return true
' $normal --server $server | ignore
let results = (do {
  source ../recipes/diagnostics-quickfix/main.nu
  main --server $server --source recipes --title "Recipe errors" --open
})
assert equal $results.count 1
assert equal $results.server $server
assert equal (nuvim quickfix get --server $server | select row column text type) [{row: 0, column: 0, text: first, type: E}]
let warnings = (do {
  source ../recipes/diagnostics-quickfix/main.nu
  main --server $server --source recipes --severity warn
})
assert equal $warnings.count 1
assert equal (nuvim quickfix get --server $server | get 0.column) 2
let empty = (do {
  source ../recipes/diagnostics-quickfix/main.nu
  main --server $server --source missing
})
assert equal $empty.count 0
assert equal (nuvim quickfix get --server $server | length) 0

print "Recipe workflows passed"
