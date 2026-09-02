def main [--server: string] {
  let target = if $server == null {
    nuvim servers | first | get server
  } else {
    $server
  }

  let buffer = (
    [alpha beta gamma]
    | nuvim scratch --server $target --name agent-control --filetype text
  )

  nuvim cursor set 1 2 --server $target | ignore
  "!" | nuvim edit 1 2 --server $target | ignore

  {
    server: $target
    buffer: $buffer.id
    cursor: (nuvim cursor --server $target)
    text: (nuvim text --server $target)
  }
}
