use ../common.nu resolve-server

def main [--server: string, --buffer: int] {
  let target = (resolve-server $server)
  let buffer = if $buffer == null {
    [] | nuvim scratch --server $target --filetype nuon
  } else {
    let existing = (nuvim buffers --server $target | where id == $buffer)
    if ($existing | is-empty) {
      error make {msg: $"buffer ($buffer) does not exist on ($target)"}
    }
    let kind = (nuvim lua 'return vim.bo[...].buftype' $buffer --server $target)
    if $kind != "nofile" {
      error make {msg: "scratch-refresh requires a nofile buffer"}
    }
    $existing | first
  }
  let report = (
    nuvim buffers --server $target
    | where id != $buffer.id
    | select id path filetype modified
    | to nuon --indent 2
  )
  $report | nuvim replace --server $target --buffer $buffer.id | ignore
  $buffer
}
