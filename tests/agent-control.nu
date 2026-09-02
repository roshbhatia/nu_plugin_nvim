let server = $env.NUVIM_TEST_SERVER

let scratch = (
  [alpha beta gamma]
  | nuvim scratch --server $server --name nuvim-agent-test --filetype text
)

let moved = (nuvim cursor set 1 2 --server $server)
if $moved != {row: 1, column: 2} {
  error make {msg: $"cursor move returned ($moved | to nuon)"}
}

"!" | nuvim edit 1 2 --server $server | ignore
let edited = (nuvim text --server $server | get lines)
if $edited != [alpha be!ta gamma] {
  error make {msg: $"range edit returned ($edited | to nuon)"}
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

{
  server: $server
  buffer: $scratch.id
  cursor: (nuvim cursor --server $server)
  lines: (nuvim text --server $server | get lines)
  opened: $opened.path
  filetype: $command_context.buffer.filetype
}
